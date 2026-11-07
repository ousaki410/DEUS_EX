//! UnrealScript bytecode interpreter.
//!
//! The parser hands us a flat list of expressions per function, ordered by
//! normalized script offset. Control flow is expressed with explicit
//! `Jump` / `JumpIfNot` / `Case` nodes, so the interpreter is a simple program
//! counter loop rather than an AST walker.

use std::collections::HashMap;
use std::sync::Arc;

use deusex_bytecode::{Expr, ExprKind, Function};
use glam::Vec3;

use crate::error::VmError;
use crate::native::{NativeContext, NativeRegistry};
use crate::unit::PackageUnit;
use crate::value::Value;

const DEFAULT_STEP_LIMIT: u64 = 1_000_000;
const DEFAULT_CALL_DEPTH_LIMIT: usize = 128;

/// Result of executing a function.
///
/// Out parameters are reported as `(argument index, value)` pairs so the
/// caller can write the value back into the corresponding argument lvalue.
#[derive(Debug, Clone)]
pub struct ExecOutcome {
    pub return_value: Value,
    pub out_values: Vec<(usize, Value)>,
    /// If `true`, execution suspended at a latent call (e.g. `Sleep`).
    /// The caller should drive [`Vm::resume_latent`] to continue.
    pub suspended: bool,
    /// State needed to resume execution after a latent call.
    pub resume: Option<LatentResume>,
}

impl ExecOutcome {
    pub fn value(value: Value) -> Self {
        Self {
            return_value: value,
            out_values: Vec::new(),
            suspended: false,
            resume: None,
        }
    }
}

/// State needed to resume execution after a latent call suspends a frame.
///
/// Captured when a latent function (e.g. `Sleep`) is invoked; passed back to
/// [`Vm::resume_latent`] to continue execution from the saved program counter.
#[derive(Clone, Debug)]
pub struct LatentResume {
    pub unit_index: usize,
    pub function_index: usize,
    pub self_object: Option<u32>,
    /// Index into the function body to resume from (the instruction after the
    /// latent call).
    pub resume_pc: usize,
    /// Local variables at the point of suspension.
    pub vars: HashMap<i32, Value>,
    /// Return-value slot reference, if any.
    pub return_ref: Option<i32>,
    /// Active switch value, if any.
    pub switch_value: Option<Value>,
}

/// Entry in the VM's latent queue, waiting to be resumed.
#[derive(Clone, Debug)]
pub struct LatentQueueEntry {
    pub unit_index: usize,
    pub function_index: usize,
    pub self_object: Option<u32>,
    pub resume_pc: usize,
    pub vars: HashMap<i32, Value>,
    pub return_ref: Option<i32>,
    pub switch_value: Option<Value>,
}

/// A script-side object instance.
///
/// Property keys may be either full script paths
/// (`HelloWorldCommandlet.intparm`) or bare names (`intparm`). Reads prefer the
/// full path and fall back to the bare name, which keeps test setup concise.
#[derive(Debug, Clone)]
pub struct ScriptObject {
    pub path: String,
    /// The script class this object belongs to (e.g. `Actor`, `DeusExPlayer`).
    /// Used for virtual function dispatch through the class hierarchy.
    pub class_path: String,
    /// The object's current UnrealScript state (empty means the default
    /// state). Used for state-qualified function dispatch and `IsInState`.
    pub current_state: String,
    /// Whether the object has been destroyed via `Destroy()`.
    pub destroyed: bool,
    /// Collision radius (Unreal units), used by `SetCollisionSize`.
    pub collision_radius: f32,
    /// Collision height (Unreal units), used by `SetCollisionSize`.
    pub collision_height: f32,
    /// Whether the object collides with other actors, used by `SetCollision`.
    pub b_collide_actors: bool,
    /// Active timers on this object.
    pub timers: Vec<Timer>,
    /// Properties keyed by full script path or bare name.
    pub properties: HashMap<String, Value>,
}

/// A timer set via `SetTimer`.
#[derive(Debug, Clone)]
pub struct Timer {
    /// Remaining time in seconds.
    pub remaining: f32,
    /// Whether the timer repeats.
    pub b_loop: bool,
    /// Name of the function to call when the timer fires.
    pub function_name: String,
}

impl ScriptObject {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            class_path: String::new(),
            current_state: String::new(),
            destroyed: false,
            collision_radius: 0.0,
            collision_height: 0.0,
            b_collide_actors: true,
            timers: Vec::new(),
            properties: HashMap::new(),
        }
    }

    pub fn with_class(path: impl Into<String>, class: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            class_path: class.into(),
            current_state: String::new(),
            destroyed: false,
            collision_radius: 0.0,
            collision_height: 0.0,
            b_collide_actors: true,
            timers: Vec::new(),
            properties: HashMap::new(),
        }
    }

    /// Current UnrealScript state (empty means the default state).
    pub fn state(&self) -> &str {
        &self.current_state
    }

    /// Set the object's current UnrealScript state.
    pub fn set_state(&mut self, state: impl Into<String>) {
        self.current_state = state.into();
    }

    /// Whether the object has been destroyed via `Destroy()`.
    pub fn is_destroyed(&self) -> bool {
        self.destroyed
    }

    /// Mark the object as destroyed.
    pub fn destroy(&mut self) {
        self.destroyed = true;
    }

    /// Collision radius in Unreal units.
    pub fn collision_radius(&self) -> f32 {
        self.collision_radius
    }

    /// Collision height in Unreal units.
    pub fn collision_height(&self) -> f32 {
        self.collision_height
    }

    /// Set collision size (`SetCollisionSize(Radius, Height)`).
    pub fn set_collision_size(&mut self, radius: f32, height: f32) {
        self.collision_radius = radius;
        self.collision_height = height;
    }

    /// Whether the object collides with other actors.
    pub fn b_collide_actors(&self) -> bool {
        self.b_collide_actors
    }

    /// Set collision toggle (`SetCollision(bNewCollision)`).
    pub fn set_collision(&mut self, b_collide: bool) {
        self.b_collide_actors = b_collide;
    }

    /// Timers active on this object.
    pub fn timers(&self) -> &[Timer] {
        &self.timers
    }

    /// Timers active on this object (mutable).
    pub fn timers_mut(&mut self) -> &mut Vec<Timer> {
        &mut self.timers
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        if let Some(value) = self.properties.get(key) {
            return Some(value);
        }
        let short = key.rsplit('.').next()?;
        if let Some(value) = self.properties.get(short) {
            return Some(value);
        }
        // Property writes use the full script path (`WanderPoint.gazeItem`)
        // while callers often read by bare name.
        let suffix = format!(".{short}");
        self.properties
            .iter()
            .find_map(|(name, value)| name.ends_with(&suffix).then_some(value))
    }

    pub fn set(&mut self, key: impl Into<String>, value: Value) {
        self.properties.insert(key.into(), value);
    }
}

/// The script virtual machine.
pub struct Vm {
    units: Vec<Arc<PackageUnit>>,
    natives: NativeRegistry,
    objects: HashMap<u32, ScriptObject>,
    class_objects: HashMap<String, u32>,
    /// Class-level default variable overrides, keyed by `(class_path, property_ref)`.
    class_defaults: HashMap<(String, i32), Value>,
    /// Frames suspended at latent calls, waiting to be resumed.
    latent_queue: Vec<LatentQueueEntry>,
    /// Pending program counter set by `GotoState`; consumed by `Frame::run`
    /// after a native call returns.
    pending_pc: Option<usize>,
    next_object_id: u32,
    log: Vec<String>,
    step_limit: u64,
    call_depth_limit: usize,
    call_depth: usize,
    rng_state: u64,
    strict_asserts: bool,
    assertion_failures: u64,
}

impl Default for Vm {
    fn default() -> Self {
        Self::new()
    }
}

impl Vm {
    pub fn new() -> Self {
        Self::with_natives(NativeRegistry::standard())
    }

    pub fn with_natives(natives: NativeRegistry) -> Self {
        Self {
            units: Vec::new(),
            natives,
            objects: HashMap::new(),
            class_objects: HashMap::new(),
            class_defaults: HashMap::new(),
            latent_queue: Vec::new(),
            pending_pc: None,
            next_object_id: 1,
            log: Vec::new(),
            step_limit: DEFAULT_STEP_LIMIT,
            call_depth_limit: DEFAULT_CALL_DEPTH_LIMIT,
            call_depth: 0,
            rng_state: 0x9E37_79B9_7F4A_7C15,
            strict_asserts: false,
            assertion_failures: 0,
        }
    }

    pub fn add_unit(&mut self, unit: PackageUnit) -> usize {
        self.units.push(Arc::new(unit));
        self.units.len() - 1
    }

    /// Resume the most recently suspended latent frame, if any.
    ///
    /// Returns `Ok(None)` when the latent queue is empty. Returns `Ok(Some(outcome))`
    /// when a frame was resumed; if it suspends again (e.g. another latent call),
    /// the outcome carries `suspended: true` and a new queue entry.
    pub fn resume_latent(&mut self) -> Result<Option<ExecOutcome>, VmError> {
        let Some(entry) = self.latent_queue.pop() else {
            return Ok(None);
        };

        let unit = self
            .units
            .get(entry.unit_index)
            .cloned()
            .ok_or_else(|| VmError::FunctionNotFound(format!("unit {}", entry.unit_index)))?;
        let function = unit
            .functions
            .get(entry.function_index)
            .cloned()
            .ok_or_else(|| {
                VmError::FunctionNotFound(format!(
                    "unit {}#{}",
                    entry.unit_index, entry.function_index
                ))
            })?;

        let mut frame = Frame::new(
            unit,
            entry.unit_index,
            entry.function_index,
            function,
            entry.self_object,
            Vec::new(),
            self.step_limit,
        );
        // Restore the suspended state.
        frame.pc = entry.resume_pc;
        frame.vars = entry.vars;
        frame.return_ref = entry.return_ref;
        frame.switch_value = entry.switch_value;

        Ok(Some(frame.run(self)?))
    }

    /// Returns the number of frames waiting in the latent queue.
    pub fn latent_queue_len(&self) -> usize {
        self.latent_queue.len()
    }

    /// Queue a frame for latent resumption, as if a latent call had suspended
    /// it. Used by tests and by external drivers that implement their own
    /// latent-completion logic (e.g. a game timer firing).
    pub fn queue_latent(&mut self, entry: LatentQueueEntry) {
        self.latent_queue.push(entry);
    }

    pub fn units(&self) -> &[Arc<PackageUnit>] {
        &self.units
    }

    pub fn unit(&self, index: usize) -> Option<&PackageUnit> {
        self.units.get(index).map(Arc::as_ref)
    }

    pub fn set_step_limit(&mut self, limit: u64) {
        self.step_limit = limit;
    }

    pub fn set_call_depth_limit(&mut self, limit: usize) {
        self.call_depth_limit = limit;
    }

    /// When enabled, failed `assert` statements abort execution instead of
    /// being counted. Retail Deus Ex behaves like `strict_asserts == false`.
    pub fn set_strict_asserts(&mut self, strict: bool) {
        self.strict_asserts = strict;
    }

    pub fn assertion_failures(&self) -> u64 {
        self.assertion_failures
    }

    pub fn natives(&self) -> &NativeRegistry {
        &self.natives
    }

    pub fn natives_mut(&mut self) -> &mut NativeRegistry {
        &mut self.natives
    }

    pub fn logs(&self) -> &[String] {
        &self.log
    }

    pub fn take_logs(&mut self) -> Vec<String> {
        std::mem::take(&mut self.log)
    }

    pub fn clear_logs(&mut self) {
        self.log.clear();
    }

    pub(crate) fn push_log(&mut self, message: String) {
        self.log.push(message);
    }

    pub fn create_object(&mut self, path: impl Into<String>) -> u32 {
        let id = self.next_object_id;
        self.next_object_id += 1;
        self.objects.insert(id, ScriptObject::new(path));
        id
    }

    pub fn create_object_with_class(
        &mut self,
        path: impl Into<String>,
        class: impl Into<String>,
    ) -> u32 {
        let id = self.next_object_id;
        self.next_object_id += 1;
        self.objects.insert(id, ScriptObject::with_class(path, class));
        id
    }

    pub fn set_object_class(&mut self, id: u32, class: impl Into<String>) {
        if let Some(object) = self.objects.get_mut(&id) {
            object.class_path = class.into();
        }
    }

    /// Override a class-level default variable value.
    pub fn set_class_default(
        &mut self,
        class_path: impl Into<String>,
        property_ref: i32,
        value: Value,
    ) {
        self.class_defaults.insert((class_path.into(), property_ref), value);
    }

    /// Look up a class-level default variable override.
    pub fn class_default(&self, class_path: &str, property_ref: i32) -> Option<&Value> {
        self.class_defaults.get(&(class_path.to_string(), property_ref))
    }

    pub fn object(&self, id: u32) -> Option<&ScriptObject> {
        self.objects.get(&id)
    }

    /// Object path (e.g. `Actor`, `DeusEx.PlayerPawn`).
    pub fn object_path(&self, id: u32) -> &str {
        self.objects
            .get(&id)
            .map(|o| o.path.as_str())
            .unwrap_or("")
    }

    /// Find an object id by matching its path (case-insensitive).
    pub fn find_object_by_path(&self, path: &str) -> Option<u32> {
        self.objects
            .iter()
            .find(|(_, obj)| obj.path.eq_ignore_ascii_case(path))
            .map(|(id, _)| *id)
    }

    pub fn object_property(&self, id: u32, key: &str) -> Option<&Value> {
        self.objects.get(&id)?.get(key)
    }

    pub fn set_object_property(&mut self, id: u32, key: impl Into<String>, value: Value) {
        if let Some(object) = self.objects.get_mut(&id) {
            object.set(key, value);
        }
    }

    /// Current UnrealScript state of an object (empty means the default state).
    pub fn object_state(&self, id: u32) -> &str {
        self.objects
            .get(&id)
            .map(|o| o.state())
            .unwrap_or("")
    }

    /// Set an object's current UnrealScript state.
    pub fn set_object_state(&mut self, id: u32, state: impl Into<String>) {
        if let Some(object) = self.objects.get_mut(&id) {
            object.set_state(state);
        }
    }

    /// Set the pending program counter for `GotoState`; consumed by
    /// [`Frame::run`] after a native call returns.
    pub(crate) fn set_pending_pc(&mut self, pc: usize) {
        self.pending_pc = Some(pc);
    }

    /// Take the pending program counter, if any.
    pub(crate) fn take_pending_pc(&mut self) -> Option<usize> {
        self.pending_pc.take()
    }

    /// Whether the object has been destroyed via `Destroy()`.
    pub fn is_object_destroyed(&self, id: u32) -> bool {
        self.objects.get(&id).map_or(false, |o| o.destroyed)
    }

    /// Collision radius of an object in Unreal units.
    pub fn object_collision_radius(&self, id: u32) -> f32 {
        self.objects.get(&id).map_or(0.0, |o| o.collision_radius)
    }

    /// Collision height of an object in Unreal units.
    pub fn object_collision_height(&self, id: u32) -> f32 {
        self.objects.get(&id).map_or(0.0, |o| o.collision_height)
    }

    /// Whether the object collides with other actors.
    pub fn object_b_collide_actors(&self, id: u32) -> bool {
        self.objects.get(&id).map_or(true, |o| o.b_collide_actors)
    }

    /// Timers active on an object.
    pub fn object_timers(&self, id: u32) -> Option<&[Timer]> {
        self.objects.get(&id).map(|o| o.timers())
    }

    /// Timers active on an object (mutable).
    pub fn object_timers_mut(&mut self, id: u32) -> Option<&mut Vec<Timer>> {
        self.objects.get_mut(&id).map(|o| o.timers_mut())
    }

    /// Add a timer to an object.
    pub fn add_object_timer(&mut self, id: u32, timer: Timer) {
        if let Some(object) = self.objects.get_mut(&id) {
            object.timers.push(timer);
        }
    }

    /// Remove all timers matching the given function name from an object.
    pub fn clear_object_timers(&mut self, id: u32, function_name: &str) {
        if let Some(object) = self.objects.get_mut(&id) {
            object.timers.retain(|t| !t.function_name.eq_ignore_ascii_case(function_name));
        }
    }

    /// Mark an object as destroyed via `Destroy()`.
    pub fn destroy_object(&mut self, id: u32) {
        if let Some(object) = self.objects.get_mut(&id) {
            object.destroy();
        }
    }

    /// Set an object's collision size via `SetCollisionSize(Radius, Height)`.
    pub fn set_object_collision_size(&mut self, id: u32, radius: f32, height: f32) {
        if let Some(object) = self.objects.get_mut(&id) {
            object.set_collision_size(radius, height);
        }
    }

    /// Set an object's collision toggle via `SetCollision(bNewCollision)`.
    pub fn set_object_collision(&mut self, id: u32, b_collide: bool) {
        if let Some(object) = self.objects.get_mut(&id) {
            object.set_collision(b_collide);
        }
    }

    pub(crate) fn next_random(&mut self) -> f32 {
        self.rng_state = self
            .rng_state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let bits = (self.rng_state >> 40) as u32;
        bits as f32 / (1u32 << 24) as f32
    }

    /// Find a function by path across all loaded units.
    pub fn find_function(&self, path: &str) -> Option<(usize, usize)> {
        for (unit_index, unit) in self.units.iter().enumerate() {
            if let Some(function_index) = unit.find_function(path) {
                return Some((unit_index, function_index));
            }
        }
        None
    }

    /// Resolve an intrinsic native index through all loaded packages.
    ///
    /// Core.u declares the operator table (indices 112+) on `Object`; other
    /// packages call those indices without redeclaring them.
    pub fn native_name(&self, index: u32) -> Option<&str> {
        self.units
            .iter()
            .find_map(|unit| unit.native_name(index))
    }

    /// Call a function by script path.
    pub fn call_function(
        &mut self,
        path: &str,
        self_object: Option<u32>,
        args: Vec<Value>,
    ) -> Result<ExecOutcome, VmError> {
        match self.find_function(path) {
            Some((unit_index, function_index)) => {
                self.exec_function(unit_index, function_index, self_object, args)
            }
            None => Err(VmError::FunctionNotFound(path.to_string())),
        }
    }

    /// Call a native function directly by script path (e.g. `Object.IsSame`),
    /// without requiring the function to be present in a package's export
    /// table. Returns the native's return value.
    pub fn call_native_by_path(
        &mut self,
        path: &str,
        self_object: Option<u32>,
        args: Vec<Value>,
    ) -> Result<Value, VmError> {
        self.call_native(path, self_object, args, path)
    }

    /// Execute one function by unit/function index.
    pub fn exec_function(
        &mut self,
        unit_index: usize,
        function_index: usize,
        self_object: Option<u32>,
        args: Vec<Value>,
    ) -> Result<ExecOutcome, VmError> {
        if self.call_depth >= self.call_depth_limit {
            return Err(VmError::CallDepth(format!(
                "unit {unit_index} function {function_index}"
            )));
        }
        self.call_depth += 1;
        let result = self.exec_function_inner(unit_index, function_index, self_object, args);
        self.call_depth -= 1;
        result
    }

    fn exec_function_inner(
        &mut self,
        unit_index: usize,
        function_index: usize,
        self_object: Option<u32>,
        args: Vec<Value>,
    ) -> Result<ExecOutcome, VmError> {
        let unit = self
            .units
            .get(unit_index)
            .cloned()
            .ok_or_else(|| VmError::FunctionNotFound(format!("unit {unit_index}")))?;
        let function = unit
            .functions
            .get(function_index)
            .cloned()
            .ok_or_else(|| VmError::FunctionNotFound(format!("unit {unit_index}#{function_index}")))?;

        // Native functions have no real script body: UE1 stores a stub that
        // references the parameters. If the runtime provides an implementation
        // for this path, dispatch to it directly. Non-intrinsic natives are
        // bound by name at load time, so path lookup is the correct mechanism
        // (their serialized native index is often 0).
        let is_intrinsic =
            function.function_flags & deusex_bytecode::function_flags::INTRINSIC != 0;
        if (is_intrinsic || function.body.is_empty()) && self.natives.contains(unit.function_path(function_index)) {
            let path = unit.function_path(function_index);
            return self
                .call_native(path, self_object, args, path)
                .map(ExecOutcome::value);
        }

        let mut frame = Frame::new(
            unit,
            unit_index,
            function_index,
            function,
            self_object,
            args,
            self.step_limit,
        );
        let mut outcome = frame.run(self)?;
        if outcome.suspended {
            if let Some(resume) = outcome.resume.take() {
                self.latent_queue.push(LatentQueueEntry {
                    unit_index: resume.unit_index,
                    function_index: resume.function_index,
                    self_object: resume.self_object,
                    resume_pc: resume.resume_pc,
                    vars: resume.vars,
                    return_ref: resume.return_ref,
                    switch_value: resume.switch_value,
                });
            }
        }
        Ok(outcome)
    }

    pub fn call_native(
        &mut self,
        name: &str,
        self_object: Option<u32>,
        args: Vec<Value>,
        caller: &str,
    ) -> Result<Value, VmError> {
        let function = self
            .natives
            .resolve(name)
            .ok_or_else(|| VmError::UnknownNative(name.to_string()))?;
        let mut context = NativeContext {
            vm: self,
            caller: caller.to_string(),
            self_object,
        };
        function(&mut context, &args)
    }

    pub(crate) fn object_id_for_path(&mut self, path: String) -> u32 {
        if let Some(id) = self.class_objects.get(&path) {
            return *id;
        }
        let id = self.create_object(path.clone());
        self.class_objects.insert(path, id);
        id
    }

    /// Candidate objects for a built-in iterator function, in stable id order.
    fn iterator_candidates(&self, name: &str, args: &[Value]) -> Vec<u32> {
        let short = name.rsplit('.').next().unwrap_or(name);
        let mut candidates: Vec<u32> = match short {
            "AllObjects" => self
                .objects
                .iter()
                .filter(|(_, object)| !object.class_path.is_empty())
                .map(|(id, _)| *id)
                .collect(),
            "AllActors" => {
                let base = args.first().and_then(|value| match value {
                    Value::Object(id) => self.objects.get(id).map(|object| object.path.clone()),
                    _ => None,
                });
                let base_short = base
                    .as_deref()
                    .and_then(|path| path.rsplit('.').next())
                    .filter(|path| !path.is_empty());
                let tag = args.get(2).and_then(|value| match value {
                    Value::Name(name) if !name.is_empty() => Some(name.as_str()),
                    _ => None,
                });
                self.objects
                    .iter()
                    .filter(|(_, object)| !object.class_path.is_empty())
                    .filter(|(_, object)| match base_short {
                        // Without a loaded class hierarchy, treat every
                        // script object as an Actor subclass unless a more
                        // specific class was requested.
                        None | Some("Actor") => true,
                        Some(expected) => object
                            .class_path
                            .rsplit('.')
                            .next()
                            .map(|class| class.eq_ignore_ascii_case(expected))
                            .unwrap_or(false),
                    })
                    .filter(|(_, object)| match tag {
                        None => true,
                        Some(tag) if tag.eq_ignore_ascii_case("None") => true,
                        Some(tag) => object
                            .get("Tag")
                            .map(|value| {
                                value.to_display_string().eq_ignore_ascii_case(tag)
                            })
                            .unwrap_or(false),
                    })
                    .map(|(id, _)| *id)
                    .collect()
            }
            _ => Vec::new(),
        };
        candidates.sort_unstable();
        candidates
    }
}

/// Per-loop state for an active `foreach`/iterator.
struct IteratorState {
    /// Normalized offset of the `Iterator` expression that starts the loop.
    start: u32,
    /// Snapshot of candidate objects, taken on the first iteration so that
    /// objects created while the loop runs do not shift the cursor.
    candidates: Vec<u32>,
    cursor: usize,
    prepared: bool,
}

struct Frame {
    unit: Arc<PackageUnit>,
    unit_index: usize,
    function_index: usize,
    function: Arc<Function>,
    function_path: String,
    vars: HashMap<i32, Value>,
    self_object: Option<u32>,
    return_ref: Option<i32>,
    /// `(argument index, parameter reference)` for each out parameter.
    out_params: Vec<(usize, i32)>,
    switch_value: Option<Value>,
    iterators: Vec<IteratorState>,
    steps_left: u64,
    /// Program counter: index into `function.body`. Used when resuming a
    /// latent frame after a latent call suspends execution.
    pc: usize,
    /// Set to `true` when a latent call suspends execution; `run` checks it
    /// after evaluating a native/extended-native expression.
    latent_suspended: bool,
    /// The body index to resume from when `latent_suspended` is `true`.
    latent_resume_pc: usize,
}

impl Frame {
    fn new(
        unit: Arc<PackageUnit>,
        unit_index: usize,
        function_index: usize,
        function: Arc<Function>,
        self_object: Option<u32>,
        args: Vec<Value>,
        step_limit: u64,
    ) -> Self {
        let function_path = unit.function_path(function_index).to_string();
        let mut vars = HashMap::new();
        let mut return_ref = None;
        let mut out_params = Vec::new();
        let mut arguments = args.into_iter();
        let mut argument_index = 0usize;

        for parameter in unit.parameters_of(function.export_ref) {
            if parameter.is_return_parm() {
                return_ref = Some(parameter.reference);
                vars.entry(parameter.reference)
                    .or_insert_with(|| unit.default_value(parameter.reference));
                continue;
            }
            if !parameter.is_parm() {
                continue;
            }
            let value = arguments
                .next()
                .unwrap_or_else(|| unit.default_value(parameter.reference));
            vars.insert(parameter.reference, value);
            if parameter.is_out_parm() {
                out_params.push((argument_index, parameter.reference));
            }
            argument_index += 1;
        }

        Self {
            unit,
            unit_index,
            function_index,
            function,
            function_path,
            vars,
            self_object,
            return_ref,
            out_params,
            switch_value: None,
            iterators: Vec::new(),
            steps_left: step_limit,
            pc: 0,
            latent_suspended: false,
            latent_resume_pc: 0,
        }
    }

    fn run(&mut self, vm: &mut Vm) -> Result<ExecOutcome, VmError> {
        let function = self.function.clone();
        let body = &function.body;
        if body.is_empty() {
            return Ok(self.finish());
        }

        let mut targets = HashMap::with_capacity(body.len());
        for (index, expression) in body.iter().enumerate() {
            targets.insert(expression.offset, index);
        }
        let end = function.script_size;
        let mut pc = self.pc;

        loop {
            if pc >= body.len() {
                return Ok(self.finish());
            }
            if self.steps_left == 0 {
                return Err(VmError::StepLimit(self.function_path.clone()));
            }
            self.steps_left -= 1;

            let expression = &body[pc];
            self.pc = pc;

            match &expression.kind {
                ExprKind::Jump(offset) => match self.resolve(*offset as u32, end, &targets)? {
                    Some(next) => pc = next,
                    None => return Ok(self.finish()),
                },
                ExprKind::JumpIfNot { offset, condition } => {
                    if self.eval(vm, condition)?.as_bool() {
                        pc += 1;
                    } else {
                        match self.resolve(*offset as u32, end, &targets)? {
                            Some(next) => pc = next,
                            None => return Ok(self.finish()),
                        }
                    }
                }
                ExprKind::Assert { offset, condition } => {
                    // Export builds of UE1 ignore assertion failures; the u16
                    // operand is the source line, not a jump target.
                    let value = self.eval(vm, condition)?;
                    let passed = value.as_bool();
                    if !passed {
                        if vm.strict_asserts {
                            return Err(VmError::AssertionFailed {
                                function: self.function_path.clone(),
                                line: *offset,
                            });
                        }
                        vm.assertion_failures += 1;
                    }
                    pc += 1;
                }
                ExprKind::Switch(_, value) => {
                    let switch_value = self.eval(vm, value)?;
                    self.switch_value = Some(switch_value);
                    pc += 1;
                }
                ExprKind::Case { offset, value } => {
                    let matches = match value {
                        Some(value) => {
                            let case_value = self.eval(vm, value)?;
                            values_equal(self.switch_value.as_ref(), &case_value)
                        }
                        None => true, // default case
                    };
                    if matches {
                        pc += 1;
                    } else {
                        match self.resolve(*offset as u32, end, &targets)? {
                            Some(next) => pc = next,
                            None => return Ok(self.finish()),
                        }
                    }
                }
                ExprKind::Return(value) => {
                    return if matches!(value.kind, ExprKind::Nothing) {
                        Ok(self.finish())
                    } else {
                        let value = self.eval(vm, value)?;
                        Ok(self.finish_with(value))
                    };
                }
                ExprKind::Stop => return Ok(self.finish()),
                ExprKind::Iterator { value, offset } => {
                    let start = expression.offset;
                    // Re-entering the loop head via `IteratorNext` reuses the
                    // existing state; a new `Iterator` nested inside another
                    // loop pushes a fresh one.
                    if self
                        .iterators
                        .last()
                        .map_or(true, |state| state.start != start)
                    {
                        self.iterators.push(IteratorState {
                            start,
                            candidates: Vec::new(),
                            cursor: 0,
                            prepared: false,
                        });
                    }
                    let active = self.eval_iterator(vm, value)?;
                    if active.as_bool() {
                        pc += 1;
                    } else {
                        // The compiler emits `IteratorPop` at the loop exit;
                        // leave the state in place so it can pop it.
                        match self.resolve(u32::from(*offset), end, &targets)? {
                            Some(next) => pc = next,
                            None => return Ok(self.finish()),
                        }
                    }
                }
                ExprKind::IteratorNext => {
                    let start = self
                        .iterators
                        .last()
                        .map(|state| state.start)
                        .ok_or_else(|| VmError::UnsupportedExpression {
                            kind: "IteratorNext outside an active iterator".to_string(),
                            function: self.function_path.clone(),
                        })?;
                    match self.resolve(start, end, &targets)? {
                        Some(next) => pc = next,
                        None => return Ok(self.finish()),
                    }
                }
                ExprKind::IteratorPop => {
                    self.iterators.pop();
                    pc += 1;
                }
                ExprKind::LabelTable(_)
                | ExprKind::GotoLabel(_)
                | ExprKind::EatString(_)
                | ExprKind::Nothing
                | ExprKind::EndFunctionParms
                | ExprKind::Unknown15 => {
                    pc += 1;
                }
                ExprKind::Native { index, params } => {
                    let result = self.eval_native_call(vm, u32::from(*index), params)?;
                    if self.latent_suspended {
                        self.latent_suspended = false;
                        return Ok(self.latent_outcome(result));
                    }
                    if let Some(pc_target) = vm.take_pending_pc() {
                        pc = pc_target;
                    } else {
                        pc += 1;
                    }
                }
                ExprKind::ExtendedNative { index, params } => {
                    let result = self.eval_native_call(vm, u32::from(*index), params)?;
                    if self.latent_suspended {
                        self.latent_suspended = false;
                        return Ok(self.latent_outcome(result));
                    }
                    if let Some(pc_target) = vm.take_pending_pc() {
                        pc = pc_target;
                    } else {
                        pc += 1;
                    }
                }
                _ => {
                    self.eval(vm, expression)?;
                    pc += 1;
                }
            }
        }
    }

    fn resolve(
        &self,
        offset: u32,
        end: u32,
        targets: &HashMap<u32, usize>,
    ) -> Result<Option<usize>, VmError> {
        if offset == end {
            return Ok(None);
        }
        if offset > end {
            return Err(VmError::InvalidJump {
                offset,
                function: self.function_path.clone(),
            });
        }
        targets.get(&offset).copied().map(Some).ok_or_else(|| {
            VmError::InvalidJump {
                offset,
                function: self.function_path.clone(),
            }
        })
    }

    fn read_var(&self, reference: i32) -> Value {
        self.vars
            .get(&reference)
            .cloned()
            .unwrap_or_else(|| self.unit.default_value(reference))
    }

    fn read_instance_var(&self, vm: &Vm, reference: i32) -> Value {
        let Some(object) = self.self_object else {
            return self.unit.default_value(reference);
        };
        let Some(path) = self.unit.property_path(reference) else {
            return self.unit.default_value(reference);
        };
        vm.object_property(object, &path)
            .cloned()
            .unwrap_or_else(|| self.unit.default_value(reference))
    }

    fn eval_list(&mut self, vm: &mut Vm, expressions: &[Expr]) -> Result<Vec<Value>, VmError> {
        let mut values = Vec::with_capacity(expressions.len());
        for expression in expressions {
            values.push(self.eval(vm, expression)?);
        }
        Ok(values)
    }

    fn eval_native_call(
        &mut self,
        vm: &mut Vm,
        index: u32,
        params: &[Expr],
    ) -> Result<Value, VmError> {
        let name = self
            .unit
            .native_name(index)
            .map(str::to_string)
            .or_else(|| vm.native_name(index).map(str::to_string))
            .ok_or_else(|| VmError::UnresolvedNative {
                function: self.function_path.clone(),
                index,
            })?;

        if is_short_circuit_native(&name) {
            return self.eval_short_circuit(vm, &name, params);
        }

        let args = self.eval_list(vm, params)?;

        // Detect latent calls (e.g. `Sleep`, `FinishAnim`). A latent call
        // suspends the current frame after the native returns; execution
        // resumes at the instruction following the call.
        let is_latent = self.unit.find_function(&name).is_some_and(|function_index| {
            self.unit.functions[function_index].function_flags
                & deusex_bytecode::function_flags::LATENT
                != 0
        });

        let result = if is_is_a_native(&name) {
            Value::Bool(self.self_is_a(vm, args.first()))
        } else {
            vm.call_native(&name, self.self_object, args, &self.function_path)?
        };

        if is_latent {
            self.latent_suspended = true;
            self.latent_resume_pc = self.pc + 1;
        }

        // Compound-assignment intrinsics (`+=`, `*=`, `AddAdd_Pre_*, ...`)
        // mutate their first argument; write the computed value back.
        if is_compound_assignment(&name) {
            if let Some(target) = params.first() {
                if is_assignable(target) {
                    self.assign(vm, target, result.clone())?;
                }
            }
        }
        // Post-increment / post-decrement: the native returns the old value;
        // bump the out-parameter by +/-1 and write it back.
        else if is_postincrement(&name) {
            if let Some(target) = params.first() {
                if is_assignable(target) {
                    let old = result.as_int().unwrap_or(0);
                    let incremented = if is_decrement(&name) {
                        old.wrapping_sub(1)
                    } else {
                        old.wrapping_add(1)
                    };
                    self.assign(vm, target, Value::Int(incremented))?;
                }
            }
        }
        Ok(result)
    }

    /// `&&` / `||` must not evaluate the right operand when the left one
    /// already decides the result. The compiler wraps the right operand in
    /// an `EX_Skip` expression for exactly this purpose.
    fn eval_short_circuit(
        &mut self,
        vm: &mut Vm,
        name: &str,
        params: &[Expr],
    ) -> Result<Value, VmError> {
        let short = name.rsplit('.').next().unwrap_or(name);
        let is_and = short.starts_with("AndAnd_");
        let Some(left_expr) = params.first() else {
            return Ok(Value::Bool(false));
        };
        let left = self.eval(vm, left_expr)?.as_bool();
        if is_and && !left {
            return Ok(Value::Bool(false));
        }
        if !is_and && left {
            return Ok(Value::Bool(true));
        }
        let Some(right_expr) = params.get(1) else {
            return Ok(Value::Bool(left));
        };
        Ok(Value::Bool(self.eval(vm, right_expr)?.as_bool()))
    }

    /// `Actor.IsA(BaseClass)` against the current self object. Without a
    /// loaded class hierarchy only exact short-name matches (plus the
    /// universal bases `Actor`/`Object`) are recognised.
    fn self_is_a(&self, vm: &Vm, base: Option<&Value>) -> bool {
        let base_name = match base {
            Some(Value::Object(id)) => vm
                .objects
                .get(id)
                .map(|object| object.path.as_str())
                .unwrap_or(""),
            Some(Value::Name(name)) | Some(Value::String(name)) => name.as_str(),
            _ => return false,
        };
        let Some(object) = self.self_object.and_then(|id| vm.objects.get(&id)) else {
            return false;
        };
        if object.class_path.is_empty() {
            return false;
        }
        let base_short = base_name.rsplit('.').next().unwrap_or(base_name);
        if base_short.eq_ignore_ascii_case("Actor") || base_short.eq_ignore_ascii_case("Object") {
            return true;
        }
        object
            .class_path
            .rsplit('.')
            .next()
            .map(|class| class.eq_ignore_ascii_case(base_short))
            .unwrap_or(false)
    }

    /// Evaluate the expression at the head of a `foreach` loop. Built-in
    /// iterator natives (`AllActors`, `AllObjects`, ...) advance a per-loop
    /// cursor and write the yielded object into their out parameter. Other
    /// expressions fall back to normal evaluation.
    fn eval_iterator(&mut self, vm: &mut Vm, value: &Expr) -> Result<Value, VmError> {
        let (index, params) = match &value.kind {
            ExprKind::Native { index, params } => (u32::from(*index), params.as_slice()),
            ExprKind::ExtendedNative { index, params } => {
                (u32::from(*index), params.as_slice())
            }
            _ => return self.eval(vm, value),
        };

        let Some(name) = self
            .unit
            .native_name(index)
            .map(str::to_string)
            .or_else(|| vm.native_name(index).map(str::to_string))
        else {
            return self.eval(vm, value);
        };
        let Some(out_index) = iterator_out_index(&name) else {
            return self.eval(vm, value);
        };

        let args = self.eval_list(vm, params)?;
        let next = {
            let Some(state) = self.iterators.last_mut() else {
                return self.eval(vm, value);
            };
            if !state.prepared {
                state.candidates = vm.iterator_candidates(&name, &args);
                state.prepared = true;
            }
            let mut next = None;
            while state.cursor < state.candidates.len() {
                let id = state.candidates[state.cursor];
                state.cursor += 1;
                if vm.objects.contains_key(&id) {
                    next = Some(id);
                    break;
                }
            }
            next
        };

        match next {
            Some(id) => {
                if let Some(target) = params.get(out_index) {
                    if is_assignable(target) {
                        self.assign(vm, target, Value::Object(id))?;
                    }
                }
                Ok(Value::Bool(true))
            }
            None => Ok(Value::Bool(false)),
        }
    }

    fn exec_virtual_fallback(
        &mut self,
        vm: &mut Vm,
        function_name: &str,
        args: Vec<Value>,
    ) -> Result<ExecOutcome, VmError> {
        let candidates = self
            .unit
            .functions_by_short_name
            .get(&function_name.to_ascii_lowercase())
            .ok_or_else(|| VmError::FunctionNotFound(function_name.to_string()))?;
        let function_index = *candidates
            .first()
            .ok_or_else(|| VmError::FunctionNotFound(function_name.to_string()))?;
        vm.exec_function(self.unit_index, function_index, self.self_object, args)
    }

    /// Copy out-parameter values reported by the callee back into the caller's
    /// argument expressions. Only assignable targets are written.
    /// Build a suspended `ExecOutcome` from the current frame state, used when
    /// a latent call pauses execution.
    fn latent_outcome(&self, return_value: Value) -> ExecOutcome {
        ExecOutcome {
            suspended: true,
            resume: Some(LatentResume {
                unit_index: self.unit_index,
                function_index: self.function_index,
                self_object: self.self_object,
                resume_pc: self.pc + 1,
                vars: self.vars.clone(),
                return_ref: self.return_ref,
                switch_value: self.switch_value.clone(),
            }),
            return_value,
            out_values: Vec::new(),
        }
    }

    fn write_back_out_params(
        &mut self,
        vm: &mut Vm,
        args: &[Expr],
        out_values: Vec<(usize, Value)>,
    ) -> Result<(), VmError> {
        for (index, value) in out_values {
            let Some(argument) = args.get(index) else {
                continue;
            };
            if !is_assignable(argument) {
                continue;
            }
            self.assign(vm, argument, value)?;
        }
        Ok(())
    }

    fn eval(&mut self, vm: &mut Vm, expression: &Expr) -> Result<Value, VmError> {
        let value = match &expression.kind {
            ExprKind::LocalVariable(reference) | ExprKind::NativeParm(reference) => {
                self.read_var(*reference)
            }
            ExprKind::InstanceVariable(reference) => self.read_instance_var(vm, *reference),
            ExprKind::DefaultVariable(reference) => {
                // Class-level default overrides take precedence over the
                // property's declared default.
                if let Some(object_id) = self.self_object {
                    if let Some(object) = vm.objects.get(&object_id) {
                        if let Some(value) = vm.class_default(&object.class_path, *reference) {
                            return Ok(value.clone());
                        }
                    }
                }
                self.unit.default_value(*reference)
            }
            ExprKind::Return(value) => {
                return if matches!(value.kind, ExprKind::Nothing) {
                    Ok(self.finish().return_value)
                } else {
                    self.eval(vm, value)
                };
            }
            ExprKind::Nothing | ExprKind::NoObject => Value::None,
            ExprKind::IntZero => Value::Int(0),
            ExprKind::IntOne => Value::Int(1),
            ExprKind::True => Value::Bool(true),
            ExprKind::False => Value::Bool(false),
            ExprKind::IntConst(value) => Value::Int(*value),
            ExprKind::IntConstByte(value) => Value::Int(i32::from(*value)),
            ExprKind::ByteConst(value) => Value::Byte(*value),
            ExprKind::FloatConst(value) => Value::Float(*value),
            ExprKind::StringConst(value) | ExprKind::UnicodeStringConst(value) => {
                Value::String(value.clone())
            }
            ExprKind::VectorConst(value) => {
                Value::Vector(Vec3::new(value[0], value[1], value[2]))
            }
            ExprKind::RotationConst(value) => {
                Value::Rotator([value[0] as i32, value[1] as i32, value[2] as i32])
            }
            ExprKind::NameConst(reference) => {
                let name = self
                    .unit
                    .package
                    .name(*reference)
                    .unwrap_or("")
                    .to_string();
                Value::Name(name)
            }
            ExprKind::ObjectConst(reference) => {
                if *reference == 0 {
                    Value::None
                } else {
                    let path = self
                        .unit
                        .package
                        .object_path(*reference)
                        .unwrap_or_default();
                    Value::Object(vm.object_id_for_path(path))
                }
            }
            ExprKind::SelfObject => self.self_object.map(Value::Object).unwrap_or(Value::None),
            ExprKind::Let { target, value } | ExprKind::LetBool { target, value } => {
                let value = self.eval(vm, value)?;
                self.assign(vm, target, value.clone())?;
                value
            }
            ExprKind::BoolVariable(inner) => self.eval(vm, inner)?,
            ExprKind::Native { index, params } => {
                self.eval_native_call(vm, u32::from(*index), params)?
            }
            ExprKind::ExtendedNative { index, params } => {
                self.eval_native_call(vm, u32::from(*index), params)?
            }
            ExprKind::VirtualFunction { name, params } => {
                let function_name = self
                    .unit
                    .package
                    .name(*name)
                    .unwrap_or("")
                    .to_string();
                let args = self.eval_list(vm, params)?;

                // Virtual functions are dispatched through the self object's
                // state and class hierarchy. Resolution order:
                //   1. `State.Function` (state-qualified, e.g. `Combat.Touch`)
                //   2. `Class.Function` (class-qualified, e.g. `Actor.Touch`)
                //   3. Short-name fallback (`exec_virtual_fallback`)
                let target = self.self_object.and_then(|object_id| {
                    let object = vm.objects.get(&object_id)?;
                    if object.class_path.is_empty() {
                        return None;
                    }
                    // 1. State-qualified path.
                    if !object.current_state.is_empty() {
                        if let Some(target) = vm.find_function(&format!(
                            "{}.{}",
                            object.current_state, function_name
                        )) {
                            return Some(target);
                        }
                    }
                    // 2. Class-qualified path.
                    vm.find_function(&format!("{}.{}", object.class_path, function_name))
                });

                let outcome = match target {
                    Some((unit_idx, func_idx)) => {
                        vm.exec_function(unit_idx, func_idx, self.self_object, args)?
                    }
                    None => self.exec_virtual_fallback(vm, &function_name, args)?,
                };
                self.write_back_out_params(vm, params, outcome.out_values)?;
                outcome.return_value
            }
            ExprKind::FinalFunction { function, params } => {
                let path = self
                    .unit
                    .package
                    .object_path(*function)
                    .unwrap_or_default();
                let args = self.eval_list(vm, params)?;
                let outcome = vm.call_function(&path, self.self_object, args)?;
                self.write_back_out_params(vm, params, outcome.out_values)?;
                outcome.return_value
            }
            ExprKind::GlobalFunction { name, params } => {
                let function_name = self
                    .unit
                    .package
                    .name(*name)
                    .unwrap_or("")
                    .to_string();
                let args = self.eval_list(vm, params)?;
                let outcome = vm.call_function(&function_name, self.self_object, args)?;
                self.write_back_out_params(vm, params, outcome.out_values)?;
                outcome.return_value
            }
            ExprKind::Conversion { opcode, value } => {
                let value = self.eval(vm, value)?;
                value.convert(*opcode).ok_or_else(|| {
                    VmError::UnsupportedExpression {
                        kind: format!("conversion 0x{opcode:02X}"),
                        function: self.function_path.clone(),
                    }
                })?
            }
            ExprKind::MetaCast { value, .. } | ExprKind::DynamicCast { value, .. } => {
                self.eval(vm, value)?
            }
            ExprKind::Context { context, value, .. }
            | ExprKind::ClassContext { context, value, .. } => {
                // Evaluate `value` with the context object as `self` so
                // instance variables and virtual calls resolve against it.
                let context_value = self.eval(vm, context)?;
                let previous = self.self_object;
                self.self_object = match context_value {
                    Value::Object(id) => Some(id),
                    _ => None,
                };
                let result = self.eval(vm, value);
                self.self_object = previous;
                result?
            }
            ExprKind::ArrayElement { array, index }
            | ExprKind::DynArrayElement { array, index } => {
                let arr = self.eval(vm, array)?;
                let idx = self.eval(vm, index)?
                    .as_int()
                    .ok_or_else(|| {
                        VmError::UnsupportedExpression {
                            kind: "array index must be an integer".to_string(),
                            function: self.function_path.clone(),
                        }
                    })? as usize;
                match &arr {
                    Value::Array(elements) => {
                        if idx >= elements.len() {
                            return Err(VmError::UnsupportedExpression {
                                kind: format!(
                                    "array index {idx} out of bounds (len {})",
                                    elements.len()
                                ),
                                function: self.function_path.clone(),
                            });
                        }
                        elements[idx].clone()
                    }
                    _ => {
                        return Err(VmError::UnsupportedExpression {
                            kind: "array element target is not an array".to_string(),
                            function: self.function_path.clone(),
                        });
                    }
                }
            }
            ExprKind::StructMember { struct_ref, value } => {
                let member = self
                    .unit
                    .package
                    .object_name(*struct_ref)
                    .unwrap_or("")
                    .to_string();
                let base = self.eval(vm, value)?;
                base.struct_member(&member)
            }
            ExprKind::StructCmpEq { left, right, .. } => {
                let left = self.eval(vm, left)?;
                let right = self.eval(vm, right)?;
                Value::Bool(left.struct_equal(&right))
            }
            ExprKind::StructCmpNe { left, right, .. } => {
                let left = self.eval(vm, left)?;
                let right = self.eval(vm, right)?;
                Value::Bool(!left.struct_equal(&right))
            }
            ExprKind::Skip { value, .. } | ExprKind::Unknown2B { value, .. } => {
                self.eval(vm, value)?
            }
            ExprKind::GotoLabel(value) => {
                self.eval(vm, value)?;
                Value::None
            }
            ExprKind::LabelTable(_) | ExprKind::EatString(_) | ExprKind::EndFunctionParms => {
                Value::None
            }
            ExprKind::IteratorPop | ExprKind::IteratorNext | ExprKind::Unknown15 => Value::None,
            ExprKind::Switch(_, value) => {
                let value = self.eval(vm, value)?;
                self.switch_value = Some(value.clone());
                value
            }
            ExprKind::New(_)
            | ExprKind::Iterator { .. }
            | ExprKind::Jump(_)
            | ExprKind::JumpIfNot { .. }
            | ExprKind::Assert { .. }
            | ExprKind::Case { .. }
            | ExprKind::Stop => {
                return Err(VmError::UnsupportedExpression {
                    kind: expression
                        .token()
                        .map(|token| token.name().to_string())
                        .unwrap_or_else(|| "unknown".to_string()),
                    function: self.function_path.clone(),
                });
            }
        };
        Ok(value)
    }

    fn assign(&mut self, vm: &mut Vm, target: &Expr, value: Value) -> Result<(), VmError> {
        match &target.kind {
            ExprKind::LocalVariable(reference) | ExprKind::NativeParm(reference) => {
                self.vars.insert(*reference, value);
                Ok(())
            }
            ExprKind::InstanceVariable(reference) => {
                if let (Some(object), Some(path)) =
                    (self.self_object, self.unit.property_path(*reference))
                {
                    vm.set_object_property(object, path, value);
                }
                Ok(())
            }
            ExprKind::BoolVariable(inner) => self.assign(vm, inner, value),
            ExprKind::StructMember {
                struct_ref,
                value: base,
            } => {
                let member = self
                    .unit
                    .package
                    .object_name(*struct_ref)
                    .unwrap_or("")
                    .to_string();
                let mut base_value = self.eval(vm, base)?;
                if !base_value.set_struct_member(&member, value) {
                    return Err(VmError::UnsupportedExpression {
                        kind: format!("assignment to struct member `{member}`"),
                        function: self.function_path.clone(),
                    });
                }
                self.assign(vm, base, base_value)
            }
            ExprKind::ArrayElement { array, index }
            | ExprKind::DynArrayElement { array, index } => {
                let mut arr = self.eval(vm, array)?;
                let idx = self.eval(vm, index)?
                    .as_int()
                    .ok_or_else(|| {
                        VmError::UnsupportedExpression {
                            kind: "array index must be an integer".to_string(),
                            function: self.function_path.clone(),
                        }
                    })? as usize;
                match &mut arr {
                    Value::Array(elements) => {
                        if idx >= elements.len() {
                            return Err(VmError::UnsupportedExpression {
                                kind: format!(
                                    "array index {idx} out of bounds (len {})",
                                    elements.len()
                                ),
                                function: self.function_path.clone(),
                            });
                        }
                        elements[idx] = value;
                    }
                    _ => {
                        return Err(VmError::UnsupportedExpression {
                            kind: "array element assignment target is not an array".to_string(),
                            function: self.function_path.clone(),
                        });
                    }
                }
                // 将修改后的数组写回数组变量（例如 InstanceVariable(S)）
                self.assign(vm, array, arr)
            }
            ExprKind::Context { context: ctx_expr, value: prop_expr, .. }
            | ExprKind::ClassContext { context: ctx_expr, value: prop_expr, .. } => {
                let obj = self.eval(vm, ctx_expr)?;
                let object_id = match obj {
                    Value::Object(id) => id,
                    // Context target may evaluate to None before the object
                    // is fully initialized; treat as object 0 (no-op).
                    Value::None => 0u32,
                    other => {
                        return Err(VmError::UnsupportedExpression {
                            kind: format!("Context assignment target is not an object: {other:?}"),
                            function: self.function_path.clone(),
                        })
                    }
                };
                let prop = self.eval(vm, prop_expr)?;
                let prop_name = match prop {
                    Value::String(s) => s,
                    Value::Name(s) => s,
                    Value::None => return Ok(()), // no property name yet, skip
                    other => {
                        return Err(VmError::UnsupportedExpression {
                            kind: format!("Context property name must be string/name, got {other:?}"),
                            function: self.function_path.clone(),
                        })
                    }
                };
                vm.set_object_property(object_id, &prop_name, value);
                Ok(())
            }
            ExprKind::MetaCast { value: inner, .. } => self.assign(vm, inner, value),
            _ => Err(VmError::UnsupportedExpression {
                kind: format!(
                    "assignment target {}",
                    target
                        .token()
                        .map(|token| token.name().to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                ),
                function: self.function_path.clone(),
            }),
        }
    }

    fn finish(&mut self) -> ExecOutcome {
        let return_value = match self.return_ref {
            Some(reference) => self.read_var(reference),
            None => Value::None,
        };
        self.finish_with(return_value)
    }

    fn finish_with(&mut self, return_value: Value) -> ExecOutcome {
        let out_values = self
            .out_params
            .iter()
            .map(|(index, reference)| (*index, self.read_var(*reference)))
            .collect();
        ExecOutcome {
            return_value,
            out_values,
            suspended: false,
            resume: None,
        }
    }
}

/// Whether a native is a boolean short-circuit operator (`&&`, `||`).
fn is_short_circuit_native(name: &str) -> bool {
    let short = name.rsplit('.').next().unwrap_or(name);
    short.starts_with("AndAnd_") || short.starts_with("OrOr_")
}

/// Whether a native name is `Actor.IsA`.
fn is_is_a_native(name: &str) -> bool {
    name.rsplit('.').next() == Some("IsA")
}

/// Index of the out parameter that receives the yielded object for built-in
/// iterator natives. `None` means the native is not a built-in iterator.
fn iterator_out_index(name: &str) -> Option<usize> {
    match name.rsplit('.').next().unwrap_or(name) {
        "AllActors" => Some(1),
        "AllObjects" => Some(0),
        _ => None,
    }
}

/// Whether a native name is one of the compound-assignment intrinsics
/// (`AddEqual_IntInt`, `MultiplyEqual_VectorFloat`, ...). `ComplementEqual_*`
/// is the approximate-equality operator and is deliberately excluded.
fn is_compound_assignment(name: &str) -> bool {
    let short = name.rsplit('.').next().unwrap_or(name);
    [
        "MultiplyEqual_",
        "DivideEqual_",
        "AddEqual_",
        "SubtractEqual_",
        "AddAdd_Pre_",
        "SubtractSubtract_Pre_",
    ]
    .iter()
    .any(|prefix| short.starts_with(prefix))
}

/// Whether a native is a post-increment/post-decrement (`AddAdd_Int`,
/// `AddAdd_Byte`, `SubtractSubtract_Int`, `SubtractSubtract_Byte`). These
/// return the old value; the VM bumps the out-parameter by +/-1 after the
/// call.
fn is_postincrement(name: &str) -> bool {
    let short = name.rsplit('.').next().unwrap_or(name);
    short.starts_with("AddAdd_") || short.starts_with("SubtractSubtract_")
}

/// Whether a post-increment/decrement native decrements (rather than
/// increments) its out-parameter.
fn is_decrement(name: &str) -> bool {
    name.rsplit('.').next() == Some("SubtractSubtract_Int")
        || name.rsplit('.').next() == Some("SubtractSubtract_Byte")
}

/// Whether an expression can be used as an assignment target.
fn is_assignable(expression: &Expr) -> bool {
    matches!(
        &expression.kind,
        ExprKind::LocalVariable(_)
            | ExprKind::NativeParm(_)
            | ExprKind::InstanceVariable(_)
            | ExprKind::BoolVariable(_)
            | ExprKind::StructMember { .. }
            | ExprKind::MetaCast { .. }
    )
}

fn values_equal(left: Option<&Value>, right: &Value) -> bool {
    let Some(left) = left else {
        return false;
    };
    if let (Some(a), Some(b)) = (left.as_int(), right.as_int()) {
        return a == b;
    }
    match (left, right) {
        (Value::Name(a), Value::Name(b))
        | (Value::String(a), Value::String(b)) => a.eq_ignore_ascii_case(b),
        _ => left == right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_compare_numerically_and_case_insensitively() {
        assert!(values_equal(Some(&Value::Int(3)), &Value::Byte(3)));
        assert!(values_equal(
            Some(&Value::Name("Foo".into())),
            &Value::Name("foo".into())
        ));
        assert!(!values_equal(Some(&Value::Int(3)), &Value::Int(4)));
        assert!(!values_equal(None, &Value::Int(0)));
    }

    #[test]
    fn object_property_falls_back_to_short_name() {
        let mut object = ScriptObject::new("Test");
        object.set("intparm", Value::Int(7));
        assert_eq!(
            object.get("HelloWorldCommandlet.intparm"),
            Some(&Value::Int(7))
        );
    }
}
