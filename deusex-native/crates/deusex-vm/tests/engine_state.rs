//! UnrealScript state machine support (`GotoState`, `IsInState`, `GetStateName`)
//! and object lifecycle/collision intrinsics (`Destroy`, `SetCollisionSize`,
//! `SetCollision`).
//!
//! Objects carry a current state; `GotoState` changes it and `IsInState` /
//! `GetStateName` query it. `Destroy` marks an object as destroyed;
//! `SetCollisionSize`/`SetCollision` configure collision properties.

use std::path::PathBuf;

use deusex_vm::{PackageUnit, Value, Vm};

fn system_dir() -> Option<PathBuf> {
    let candidates = [
        std::env::var_os("DEUSEX_GOTY_PATH").map(PathBuf::from),
        Some(PathBuf::from("/home/ou/Projects/deus_ex_goty/goty/app/system")),
    ];
    for candidate in candidates.into_iter().flatten() {
        let system = if candidate.ends_with("system") {
            candidate
        } else {
            candidate.join("system")
        };
        if system.join("Core.u").is_file() {
            return Some(system);
        }
    }
    None
}

fn core_vm() -> Option<Vm> {
    let system = system_dir()?;
    let core = PackageUnit::load(system.join("Core.u")).expect("load Core.u");
    let engine = PackageUnit::load(system.join("Engine.u")).expect("load Engine.u");
    let mut vm = Vm::new();
    vm.add_unit(core);
    vm.add_unit(engine);
    Some(vm)
}

fn find_and_exec(
    vm: &mut Vm,
    path: &str,
    self_object: Option<u32>,
    args: Vec<Value>,
) -> Value {
    let (unit_index, function_index) = vm
        .find_function(path)
        .unwrap_or_else(|| panic!("function {path} should exist"));
    vm.exec_function(unit_index, function_index, self_object, args)
        .unwrap_or_else(|e| panic!("execute {path}: {e:?}"))
        .return_value
}

#[test]
fn get_state_name_defaults_to_empty() {
    let mut vm = core_vm().expect("vm should load");

    let obj = vm.create_object("test object");
    let value = find_and_exec(&mut vm, "Object.GetStateName", Some(obj), Vec::new());
    assert_eq!(value, Value::Name(String::new()));
    assert_eq!(vm.object_state(obj), "");
}

#[test]
fn goto_state_changes_object_state() {
    let mut vm = core_vm().expect("vm should load");

    let obj = vm.create_object("test object");

    // Default state is empty.
    assert_eq!(vm.object_state(obj), "");

    // GotoState("MyState", 0) sets the state.
    find_and_exec(
        &mut vm,
        "Object.GotoState",
        Some(obj),
        vec![Value::Name("MyState".into()), Value::Int(0)],
    );
    assert_eq!(vm.object_state(obj), "MyState");

    // GetStateName reflects the new state.
    let value = find_and_exec(&mut vm, "Object.GetStateName", Some(obj), Vec::new());
    assert_eq!(value, Value::Name("MyState".into()));
}

#[test]
fn is_in_state_checks_current_state() {
    let mut vm = core_vm().expect("vm should load");

    let obj = vm.create_object("test object");

    // Not in any state initially.
    assert!(!matches!(
        find_and_exec(
            &mut vm,
            "Object.IsInState",
            Some(obj),
            vec![Value::Name("AnyState".into())]
        ),
        Value::Bool(true)
    ));

    // GotoState then IsInState.
    find_and_exec(
        &mut vm,
        "Object.GotoState",
        Some(obj),
        vec![Value::Name("Combat".into()), Value::Int(0)],
    );
    assert!(
        matches!(
            find_and_exec(
                &mut vm,
                "Object.IsInState",
                Some(obj),
                vec![Value::Name("Combat".into())]
            ),
            Value::Bool(true)
        ),
        "IsInState(Combat) should be true"
    );
    assert!(
        matches!(
            find_and_exec(
                &mut vm,
                "Object.IsInState",
                Some(obj),
                vec![Value::Name("Patrol".into())]
            ),
            Value::Bool(false)
        ),
        "IsInState(Patrol) should be false"
    );
}

#[test]
fn destroy_marks_object_destroyed() {
    let mut vm = core_vm().expect("vm should load");

    let obj = vm.create_object("test object");
    assert!(!vm.is_object_destroyed(obj));

    // Actor.Destroy is in Engine.u
    find_and_exec(&mut vm, "Actor.Destroy", Some(obj), Vec::new());
    assert!(vm.is_object_destroyed(obj));
}

#[test]
fn set_collision_size_and_collision() {
    let mut vm = core_vm().expect("vm should load");

    let obj = vm.create_object("test object");

    // Default collision is zero.
    assert_eq!(vm.object_collision_radius(obj), 0.0);
    assert_eq!(vm.object_collision_height(obj), 0.0);
    assert!(vm.object_b_collide_actors(obj));

    // Actor.SetCollisionSize(34.0, 64.0)
    find_and_exec(
        &mut vm,
        "Actor.SetCollisionSize",
        Some(obj),
        vec![Value::Float(34.0), Value::Float(64.0)],
    );
    assert_eq!(vm.object_collision_radius(obj), 34.0);
    assert_eq!(vm.object_collision_height(obj), 64.0);

    // Actor.SetCollision(false)
    find_and_exec(&mut vm, "Actor.SetCollision", Some(obj), vec![Value::Bool(false)]);
    assert!(!vm.object_b_collide_actors(obj));

    // Actor.SetCollision(true)
    find_and_exec(&mut vm, "Actor.SetCollision", Some(obj), vec![Value::Bool(true)]);
    assert!(vm.object_b_collide_actors(obj));
}

#[test]
fn set_timer_adds_timer() {
    let mut vm = core_vm().expect("vm should load");

    let obj = vm.create_object("test object");
    assert!(vm.object_timers(obj).unwrap().is_empty());

    // Actor.SetTimer(1.0, true)
    find_and_exec(
        &mut vm,
        "Actor.SetTimer",
        Some(obj),
        vec![Value::Float(1.0), Value::Bool(true)],
    );

    let timers = vm.object_timers(obj).unwrap();
    assert_eq!(timers.len(), 1);
    assert_eq!(timers[0].remaining, 1.0);
    assert!(timers[0].b_loop);
    assert_eq!(timers[0].function_name, "Timer");
}

#[test]
fn clear_timer_removes_timer() {
    let mut vm = core_vm().expect("vm should load");

    let obj = vm.create_object("test object");

    // Add a timer first via SetTimer native.
    find_and_exec(
        &mut vm,
        "Actor.SetTimer",
        Some(obj),
        vec![Value::Float(1.0), Value::Bool(true)],
    );
    assert_eq!(vm.object_timers(obj).unwrap().len(), 1);

    // Clear timers directly via VM method.
    vm.clear_object_timers(obj, "Timer");
    assert!(vm.object_timers(obj).unwrap().is_empty());
}

#[test]
fn spawn_creates_object() {
    let mut vm = core_vm().expect("vm should load");

    // Spawn with a null class (Object(0)) returns None.
    let result = find_and_exec(&mut vm, "Actor.Spawn", None, vec![Value::Object(0)]);
    assert_eq!(result, Value::None);

    // Create a class object to spawn from.
    let class_obj = vm.create_object_with_class("Actor", "Actor");

    // Spawn a new actor from the class.
    let result = find_and_exec(
        &mut vm,
        "Actor.Spawn",
        None,
        vec![
            Value::Object(class_obj),
            Value::None, // SpawnOwner
            Value::Name("".into()), // SpawnTag
            Value::Vector(glam::Vec3::new(100.0, 200.0, 300.0)),
            Value::Rotator([0, 0, 0]),
        ],
    );

    match result {
        Value::Object(new_id) => {
            assert_ne!(new_id, 0);
            assert_ne!(new_id, class_obj);
            // Verify the spawned object has the class path.
            assert_eq!(vm.object_path(new_id), "Actor");
            // Verify location was set.
            match vm.object_property(new_id, "Location") {
                Some(Value::Vector(loc)) => {
                    assert_eq!(*loc, glam::Vec3::new(100.0, 200.0, 300.0));
                }
                other => panic!("expected Location vector, got {other:?}"),
            }
        }
        other => panic!("expected Object ID, got {other:?}"),
    }
}

#[test]
fn is_same_compares_object_references() {
    let mut vm = core_vm().expect("vm should load");

    let obj1 = vm.create_object("object one");
    let obj2 = vm.create_object("object two");

    // Same object reference.
    assert!(
        matches!(
            vm.call_native_by_path("Object.IsSame", None, vec![Value::Object(obj1), Value::Object(obj1)]),
            Ok(Value::Bool(true))
        )
    );
    // Different object references.
    assert!(
        matches!(
            vm.call_native_by_path("Object.IsSame", None, vec![Value::Object(obj1), Value::Object(obj2)]),
            Ok(Value::Bool(false))
        )
    );
    // None is not same as anything.
    assert!(
        matches!(
            vm.call_native_by_path("Object.IsSame", None, vec![Value::Object(obj1), Value::Object(0)]),
            Ok(Value::Bool(false))
        )
    );
}

#[test]
fn get_class_name_returns_class_path() {
    let mut vm = core_vm().expect("vm should load");

    let obj = vm.create_object_with_class("MyActor", "Actor");
    let result = vm
        .call_native_by_path("Object.GetClassName", Some(obj), Vec::new())
        .expect("Object.GetClassName should execute");
    assert_eq!(result, Value::Name("Actor".into()));
}

#[test]
fn get_set_property_text() {
    let mut vm = core_vm().expect("vm should load");

    let obj = vm.create_object("test object");

    // GetPropertyText on unset property returns empty string.
    let result = vm
        .call_native_by_path(
            "Object.GetPropertyText",
            Some(obj),
            vec![Value::Name("MyProp".into())],
        )
        .expect("Object.GetPropertyText should execute");
    assert_eq!(result, Value::String("".into()));

    // SetPropertyText then GetPropertyText.
    vm.call_native_by_path(
        "Object.SetPropertyText",
        Some(obj),
        vec![Value::Name("MyProp".into()), Value::String("hello".into())],
    )
    .expect("Object.SetPropertyText should execute");
    let result = vm
        .call_native_by_path(
            "Object.GetPropertyText",
            Some(obj),
            vec![Value::Name("MyProp".into())],
        )
        .expect("Object.GetPropertyText should execute");
    assert_eq!(result, Value::String("hello".into()));
}

#[test]
fn class_is_child_of() {
    let mut vm = core_vm().expect("vm should load");

    let obj = vm.create_object("test object");

    // Exact match is true.
    assert!(
        matches!(
            vm.call_native_by_path(
                "Object.ClassIsChildOf",
                Some(obj),
                vec![
                    Value::Name("Actor".into()),
                    Value::Name("Actor".into())
                ]
            ),
            Ok(Value::Bool(true))
        )
    );
    // Universal base Object is true.
    assert!(
        matches!(
            vm.call_native_by_path(
                "Object.ClassIsChildOf",
                Some(obj),
                vec![Value::Name("Actor".into()), Value::Name("Object".into())]
            ),
            Ok(Value::Bool(true))
        )
    );
    // Unrelated classes are false.
    assert!(
        matches!(
            vm.call_native_by_path(
                "Object.ClassIsChildOf",
                Some(obj),
                vec![Value::Name("Pawn".into()), Value::Name("Decoration".into())]
            ),
            Ok(Value::Bool(false))
        )
    );
}

#[test]
fn dynamic_load_object_finds_loaded_object() {
    let mut vm = core_vm().expect("vm should load");

    let obj = vm.create_object_with_class("MyPackage.MyActor", "MyPackage.MyActor");

    // DynamicLoadObject finds the loaded object.
    let result = vm
        .call_native_by_path(
            "Object.DynamicLoadObject",
            None,
            vec![Value::Name("MyPackage.MyActor".into())],
        )
        .expect("Object.DynamicLoadObject should execute");
    assert_eq!(result, Value::Object(obj));

    // Non-existent object returns None.
    let result = vm.call_native_by_path(
        "Object.DynamicLoadObject",
        None,
        vec![Value::Name("NonExistent.Package".into())],
    );
    // DynamicLoadObject returns an error for not-found (stub behavior).
    assert!(result.is_err());
}

#[test]
fn localize_returns_key() {
    let mut vm = core_vm().expect("vm should load");

    let result = vm
        .call_native_by_path(
            "Object.Localize",
            None,
            vec![Value::Name("SomeLocalizationKey".into())],
        )
        .expect("Object.Localize should execute");
    assert_eq!(result, Value::String("SomeLocalizationKey".into()));
}
