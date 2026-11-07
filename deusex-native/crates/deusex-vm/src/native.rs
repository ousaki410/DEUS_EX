//! Native function dispatch.
//!
//! UnrealScript operators are native functions. The compiler stores an index
//! for "intrinsic" operators (see `Object.uc`) and the runtime binds other
//! natives by name. The VM resolves call-site indices through
//! [`PackageUnit::native_name`](crate::PackageUnit::native_name) and then
//! looks the resulting path up here.

use std::collections::HashMap;

use glam::Vec3;

use crate::error::VmError;
use crate::value::Value;
use crate::vm::{Timer, Vm};

/// Signature of a native function implementation.
///
/// Arguments are passed by value for now. Out-parameters will require a
/// mutable argument slice and copy-back into the caller frame; that is not
/// wired up yet.
pub type NativeFn = fn(&mut NativeContext<'_>, &[Value]) -> Result<Value, VmError>;

/// Context handed to native functions.
pub struct NativeContext<'a> {
    pub(crate) vm: &'a mut Vm,
    pub(crate) caller: String,
    /// The current `self` object, if any. Used by state-related natives
    /// (`GotoState`, `IsInState`, `GetStateName`).
    pub(crate) self_object: Option<u32>,
}

impl<'a> NativeContext<'a> {
    pub fn log(&mut self, message: impl Into<String>) {
        self.vm.push_log(message.into());
    }

    pub fn logs(&self) -> &[String] {
        self.vm.logs()
    }

    pub fn caller(&self) -> &str {
        &self.caller
    }

    /// The current `self` object, if any.
    pub fn self_object_id(&self) -> Option<u32> {
        self.self_object
    }

    pub fn get_property(&self, object: u32, key: &str) -> Option<&Value> {
        self.vm.object_property(object, key)
    }

    pub fn set_property(&mut self, object: u32, key: impl Into<String>, value: Value) {
        self.vm.set_object_property(object, key, value);
    }

    pub fn next_random(&mut self) -> f32 {
        self.vm.next_random()
    }

    /// Set the pending program counter for `GotoState`; consumed by
    /// [`Frame::run`] after the native call returns.
    pub fn set_pc(&mut self, pc: usize) {
        self.vm.set_pending_pc(pc);
    }

    pub fn call_function(
        &mut self,
        path: &str,
        self_object: Option<u32>,
        args: Vec<Value>,
    ) -> Result<Value, VmError> {
        self.vm
            .call_function(path, self_object, args)
            .map(|outcome| outcome.return_value)
    }
}

/// Registry of native function implementations keyed by full script path.
#[derive(Default)]
pub struct NativeRegistry {
    functions: HashMap<String, NativeFn>,
}

impl NativeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registry with the standard `Object` natives used by most scripts.
    pub fn standard() -> Self {
        let mut registry = Self::new();
        register_core(&mut registry);
        registry
    }

    pub fn register(&mut self, path: impl Into<String>, function: NativeFn) {
        self.functions.insert(path.into(), function);
    }

    /// Resolve a native path. Accepts the full path (`Object.Log`), a bare
    /// short name (`Log`), or a path whose short name is registered.
    pub fn resolve(&self, path: &str) -> Option<NativeFn> {
        if let Some(function) = self.functions.get(path) {
            return Some(*function);
        }
        let short = path.rsplit('.').next().unwrap_or(path);
        if short != path {
            if let Some(function) = self.functions.get(short) {
                return Some(*function);
            }
        }
        self.functions
            .iter()
            .find(|(key, _)| key.rsplit('.').next() == Some(short))
            .map(|(_, function)| *function)
    }

    pub fn contains(&self, path: &str) -> bool {
        self.resolve(path).is_some()
    }

    pub fn len(&self) -> usize {
        self.functions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.functions.is_empty()
    }
}

fn arity(name: &str) -> VmError {
    VmError::native(name, "not enough arguments")
}

fn compare(
    name: &str,
    args: &[Value],
    predicate: impl FnOnce(&Value, &Value) -> bool,
) -> Result<Value, VmError> {
    let left = args.first().ok_or_else(|| arity(name))?;
    let right = args.get(1).ok_or_else(|| arity(name))?;
    Ok(Value::Bool(predicate(left, right)))
}

fn int_binop(
    name: &str,
    args: &[Value],
    operation: impl FnOnce(i32, i32) -> i32,
) -> Result<Value, VmError> {
    let left = args.first().and_then(Value::as_int).ok_or_else(|| arity(name))?;
    let right = args.get(1).and_then(Value::as_int).ok_or_else(|| arity(name))?;
    Ok(Value::Int(operation(left, right)))
}

fn float_binop(
    name: &str,
    args: &[Value],
    operation: impl FnOnce(f32, f32) -> f32,
) -> Result<Value, VmError> {
    let left = args.first().and_then(Value::as_float).ok_or_else(|| arity(name))?;
    let right = args.get(1).and_then(Value::as_float).ok_or_else(|| arity(name))?;
    Ok(Value::Float(operation(left, right)))
}

/// Operand coercion used by the compound-assignment intrinsics: a missing
/// argument (`Nothing` in bytecode) behaves like the parameter's zero default.
fn numeric_int(value: Option<&Value>) -> i32 {
    match value {
        None | Some(Value::None) => 0,
        Some(value) => value.as_int().unwrap_or(0),
    }
}

fn numeric_float(value: Option<&Value>) -> f32 {
    match value {
        None | Some(Value::None) => 0.0,
        Some(value) => value.as_float().unwrap_or(0.0),
    }
}

fn byte_assign_binop(
    args: &[Value],
    operation: impl FnOnce(u8, u8) -> u8,
) -> Result<Value, VmError> {
    let left = numeric_int(args.first()).clamp(0, 255) as u8;
    let right = numeric_int(args.get(1)).clamp(0, 255) as u8;
    Ok(Value::Byte(operation(left, right)))
}

fn rotator_of(value: Option<&Value>) -> [i32; 3] {
    match value {
        Some(Value::Rotator(rotator)) => *rotator,
        _ => [0, 0, 0],
    }
}

fn scale_rotator(rotator: [i32; 3], scalar: f32) -> [i32; 3] {
    [
        (rotator[0] as f32 * scalar) as i32,
        (rotator[1] as f32 * scalar) as i32,
        (rotator[2] as f32 * scalar) as i32,
    ]
}

fn string_cmp(left: &Value, right: &Value) -> std::cmp::Ordering {
    left.to_display_string()
        .to_ascii_lowercase()
        .cmp(&right.to_display_string().to_ascii_lowercase())
}

fn string_eq(left: &Value, right: &Value) -> bool {
    string_cmp(left, right) == std::cmp::Ordering::Equal
}

fn vector_of(value: &Value) -> Vec3 {
    match value {
        Value::Vector(vector) => *vector,
        _ => Vec3::ZERO,
    }
}

fn register_core(registry: &mut NativeRegistry) {
    // Logging.
    registry.register("Object.Log", |ctx, args| {
        let message = args
            .first()
            .map(Value::to_display_string)
            .unwrap_or_default();
        ctx.log(message);
        Ok(Value::None)
    });
    registry.register("Object.Warn", |ctx, args| {
        let message = args
            .first()
            .map(Value::to_display_string)
            .unwrap_or_default();
        ctx.log(message);
        Ok(Value::None)
    });

    // String operations.
    registry.register("Object.Concat_StrStr", |_ctx, args| {
        let left = args
            .first()
            .map(Value::to_display_string)
            .unwrap_or_default();
        let right = args
            .get(1)
            .map(Value::to_display_string)
            .unwrap_or_default();
        Ok(Value::String(format!("{left}{right}")))
    });
    registry.register("Object.At_StrStr", |_ctx, args| {
        let left = args
            .first()
            .map(Value::to_display_string)
            .unwrap_or_default();
        let right = args
            .get(1)
            .map(Value::to_display_string)
            .unwrap_or_default();
        Ok(Value::String(format!("{left} {right}")))
    });
    registry.register("Object.Len", |_ctx, args| {
        let text = args
            .first()
            .map(Value::to_display_string)
            .unwrap_or_default();
        Ok(Value::Int(text.chars().count() as i32))
    });
    registry.register("Object.InStr", |_ctx, args| {
        let haystack = args
            .first()
            .map(Value::to_display_string)
            .unwrap_or_default()
            .to_ascii_lowercase();
        let needle = args
            .get(1)
            .map(Value::to_display_string)
            .unwrap_or_default()
            .to_ascii_lowercase();
        match haystack.find(&needle) {
            Some(index) => Ok(Value::Int(index as i32)),
            None => Ok(Value::Int(-1)),
        }
    });
    registry.register("Object.Mid", |_ctx, args| {
        let text = args
            .first()
            .map(Value::to_display_string)
            .unwrap_or_default();
        let start = args.get(1).and_then(Value::as_int).unwrap_or(0).max(0) as usize;
        let count = args.get(2).and_then(Value::as_int).unwrap_or(i32::MAX / 2);
        let count = count.max(0) as usize;
        let result: String = text.chars().skip(start).take(count).collect();
        Ok(Value::String(result))
    });
    registry.register("Object.Left", |_ctx, args| {
        let text = args
            .first()
            .map(Value::to_display_string)
            .unwrap_or_default();
        let count = args.get(1).and_then(Value::as_int).unwrap_or(0).max(0) as usize;
        let result: String = text.chars().take(count).collect();
        Ok(Value::String(result))
    });
    registry.register("Object.Right", |_ctx, args| {
        let text = args
            .first()
            .map(Value::to_display_string)
            .unwrap_or_default();
        let count = args.get(1).and_then(Value::as_int).unwrap_or(0).max(0) as usize;
        let total = text.chars().count();
        let start = total.saturating_sub(count);
        Ok(Value::String(text.chars().skip(start).collect()))
    });

    // String comparisons.
    registry.register("Object.EqualEqual_StrStr", |_ctx, args| {
        compare("EqualEqual_StrStr", args, string_eq)
    });
    registry.register("Object.NotEqual_StrStr", |_ctx, args| {
        compare("NotEqual_StrStr", args, |left, right| !string_eq(left, right))
    });
    registry.register("Object.Less_StrStr", |_ctx, args| {
        compare("Less_StrStr", args, |left, right| {
            string_cmp(left, right) == std::cmp::Ordering::Less
        })
    });
    registry.register("Object.Greater_StrStr", |_ctx, args| {
        compare("Greater_StrStr", args, |left, right| {
            string_cmp(left, right) == std::cmp::Ordering::Greater
        })
    });
    registry.register("Object.LessEqual_StrStr", |_ctx, args| {
        compare("LessEqual_StrStr", args, |left, right| {
            string_cmp(left, right) != std::cmp::Ordering::Greater
        })
    });
    registry.register("Object.GreaterEqual_StrStr", |_ctx, args| {
        compare("GreaterEqual_StrStr", args, |left, right| {
            string_cmp(left, right) != std::cmp::Ordering::Less
        })
    });

    // Name comparisons (case-insensitive, like strings).
    registry.register("Object.EqualEqual_NameName", |_ctx, args| {
        compare("EqualEqual_NameName", args, string_eq)
    });
    registry.register("Object.NotEqual_NameName", |_ctx, args| {
        compare("NotEqual_NameName", args, |left, right| {
            !string_eq(left, right)
        })
    });

    // Integer comparisons.
    registry.register("Object.EqualEqual_IntInt", |_ctx, args| {
        compare("EqualEqual_IntInt", args, |left, right| {
            left.as_int() == right.as_int()
        })
    });
    registry.register("Object.NotEqual_IntInt", |_ctx, args| {
        compare("NotEqual_IntInt", args, |left, right| {
            left.as_int() != right.as_int()
        })
    });
    registry.register("Object.Less_IntInt", |_ctx, args| {
        compare("Less_IntInt", args, |left, right| {
            left.as_int().unwrap_or(0) < right.as_int().unwrap_or(0)
        })
    });
    registry.register("Object.Greater_IntInt", |_ctx, args| {
        compare("Greater_IntInt", args, |left, right| {
            left.as_int().unwrap_or(0) > right.as_int().unwrap_or(0)
        })
    });
    registry.register("Object.LessEqual_IntInt", |_ctx, args| {
        compare("LessEqual_IntInt", args, |left, right| {
            left.as_int().unwrap_or(0) <= right.as_int().unwrap_or(0)
        })
    });
    registry.register("Object.GreaterEqual_IntInt", |_ctx, args| {
        compare("GreaterEqual_IntInt", args, |left, right| {
            left.as_int().unwrap_or(0) >= right.as_int().unwrap_or(0)
        })
    });

    // Float comparisons.
    registry.register("Object.EqualEqual_FloatFloat", |_ctx, args| {
        compare("EqualEqual_FloatFloat", args, |left, right| {
            left.as_float() == right.as_float()
        })
    });
    registry.register("Object.NotEqual_FloatFloat", |_ctx, args| {
        compare("NotEqual_FloatFloat", args, |left, right| {
            left.as_float() != right.as_float()
        })
    });
    registry.register("Object.Less_FloatFloat", |_ctx, args| {
        compare("Less_FloatFloat", args, |left, right| {
            left.as_float().unwrap_or(0.0) < right.as_float().unwrap_or(0.0)
        })
    });
    registry.register("Object.Greater_FloatFloat", |_ctx, args| {
        compare("Greater_FloatFloat", args, |left, right| {
            left.as_float().unwrap_or(0.0) > right.as_float().unwrap_or(0.0)
        })
    });
    registry.register("Object.LessEqual_FloatFloat", |_ctx, args| {
        compare("LessEqual_FloatFloat", args, |left, right| {
            left.as_float().unwrap_or(0.0) <= right.as_float().unwrap_or(0.0)
        })
    });
    registry.register("Object.GreaterEqual_FloatFloat", |_ctx, args| {
        compare("GreaterEqual_FloatFloat", args, |left, right| {
            left.as_float().unwrap_or(0.0) >= right.as_float().unwrap_or(0.0)
        })
    });

    // Object comparisons.
    registry.register("Object.EqualEqual_ObjectObject", |_ctx, args| {
        compare("EqualEqual_ObjectObject", args, |left, right| {
            left.as_object() == right.as_object()
        })
    });
    registry.register("Object.NotEqual_ObjectObject", |_ctx, args| {
        compare("NotEqual_ObjectObject", args, |left, right| {
            left.as_object() != right.as_object()
        })
    });

    // Boolean operators.
    registry.register("Object.Not_PreBool", |_ctx, args| {
        let value = args.first().map(Value::as_bool).unwrap_or(false);
        Ok(Value::Bool(!value))
    });
    registry.register("Object.AndAnd_BoolBool", |_ctx, args| {
        compare("AndAnd_BoolBool", args, |left, right| {
            left.as_bool() && right.as_bool()
        })
    });
    registry.register("Object.OrOr_BoolBool", |_ctx, args| {
        compare("OrOr_BoolBool", args, |left, right| {
            left.as_bool() || right.as_bool()
        })
    });
    registry.register("Object.XorXor_BoolBool", |_ctx, args| {
        compare("XorXor_BoolBool", args, |left, right| {
            left.as_bool() ^ right.as_bool()
        })
    });

    // Integer arithmetic and bit operations.
    registry.register("Object.Add_IntInt", |_ctx, args| {
        int_binop("Add_IntInt", args, |left, right| left.wrapping_add(right))
    });
    registry.register("Object.Subtract_IntInt", |_ctx, args| {
        int_binop("Subtract_IntInt", args, |left, right| {
            left.wrapping_sub(right)
        })
    });
    registry.register("Object.Multiply_IntInt", |_ctx, args| {
        int_binop("Multiply_IntInt", args, |left, right| {
            left.wrapping_mul(right)
        })
    });
    registry.register("Object.Divide_IntInt", |_ctx, args| {
        int_binop("Divide_IntInt", args, |left, right| {
            if right == 0 {
                0
            } else {
                left.wrapping_div(right)
            }
        })
    });
    registry.register("Object.Percent_IntInt", |_ctx, args| {
        int_binop("Percent_IntInt", args, |left, right| {
            if right == 0 {
                0
            } else {
                left.wrapping_rem(right)
            }
        })
    });
    registry.register("Object.And_IntInt", |_ctx, args| {
        int_binop("And_IntInt", args, |left, right| left & right)
    });
    registry.register("Object.Or_IntInt", |_ctx, args| {
        int_binop("Or_IntInt", args, |left, right| left | right)
    });
    registry.register("Object.Xor_IntInt", |_ctx, args| {
        int_binop("Xor_IntInt", args, |left, right| left ^ right)
    });
    registry.register("Object.Complement_PreInt", |_ctx, args| {
        let value = args
            .first()
            .and_then(Value::as_int)
            .ok_or_else(|| arity("Complement_PreInt"))?;
        Ok(Value::Int(!value))
    });
    registry.register("Object.LessLess_IntInt", |_ctx, args| {
        int_binop("LessLess_IntInt", args, |left, right| {
            left.wrapping_shl(right as u32)
        })
    });
    registry.register("Object.GreaterGreater_IntInt", |_ctx, args| {
        int_binop("GreaterGreater_IntInt", args, |left, right| {
            left.wrapping_shr(right as u32)
        })
    });

    // Float arithmetic.
    registry.register("Object.Add_FloatFloat", |_ctx, args| {
        float_binop("Add_FloatFloat", args, |left, right| left + right)
    });
    registry.register("Object.Subtract_FloatFloat", |_ctx, args| {
        float_binop("Subtract_FloatFloat", args, |left, right| left - right)
    });
    registry.register("Object.Multiply_FloatFloat", |_ctx, args| {
        float_binop("Multiply_FloatFloat", args, |left, right| left * right)
    });
    registry.register("Object.Divide_FloatFloat", |_ctx, args| {
        float_binop("Divide_FloatFloat", args, |left, right| {
            if right == 0.0 {
                0.0
            } else {
                left / right
            }
        })
    });
    registry.register("Object.Percent_FloatFloat", |_ctx, args| {
        float_binop("Percent_FloatFloat", args, |left, right| {
            if right == 0.0 {
                0.0
            } else {
                left % right
            }
        })
    });
    registry.register("Object.MultiplyMultiply_FloatFloat", |_ctx, args| {
        float_binop("MultiplyMultiply_FloatFloat", args, |left, right| {
            left.powf(right)
        })
    });

    // Compound assignment operators (`+=`, `-=`, `*=`, `/=`). These mutate
    // their first argument in place in UE1; the VM writes the returned value
    // back into the argument lvalue, so implementations only compute it.
    registry.register("Object.MultiplyEqual_ByteByte", |_ctx, args| {
        byte_assign_binop(args, |left, right| left.wrapping_mul(right))
    });
    registry.register("Object.DivideEqual_ByteByte", |_ctx, args| {
        byte_assign_binop(args, |left, right| if right == 0 { 0 } else { left / right })
    });
    registry.register("Object.AddEqual_ByteByte", |_ctx, args| {
        byte_assign_binop(args, |left, right| left.wrapping_add(right))
    });
    registry.register("Object.SubtractEqual_ByteByte", |_ctx, args| {
        byte_assign_binop(args, |left, right| left.wrapping_sub(right))
    });
    registry.register("Object.MultiplyEqual_IntFloat", |_ctx, args| {
        let left = numeric_int(args.first());
        let right = numeric_float(args.get(1));
        Ok(Value::Int((left as f32 * right) as i32))
    });
    registry.register("Object.DivideEqual_IntFloat", |_ctx, args| {
        let left = numeric_int(args.first());
        let right = numeric_float(args.get(1));
        Ok(Value::Int(if right == 0.0 {
            0
        } else {
            (left as f32 / right) as i32
        }))
    });
    registry.register("Object.AddEqual_IntInt", |_ctx, args| {
        Ok(Value::Int(
            numeric_int(args.first()).wrapping_add(numeric_int(args.get(1))),
        ))
    });
    registry.register("Object.SubtractEqual_IntInt", |_ctx, args| {
        Ok(Value::Int(
            numeric_int(args.first()).wrapping_sub(numeric_int(args.get(1))),
        ))
    });
    // Post-increment / post-decrement: return the old value; the VM bumps the
    // out-parameter by +/-1 after the call (see is_postincrement handling in
    // eval_native_call).
    registry.register("Object.AddAdd_Int", |_ctx, args| {
        Ok(Value::Int(numeric_int(args.first())))
    });
    registry.register("Object.AddAdd_Byte", |_ctx, args| {
        Ok(Value::Byte(numeric_int(args.first()) as u8))
    });
    registry.register("Object.SubtractSubtract_Int", |_ctx, args| {
        Ok(Value::Int(numeric_int(args.first())))
    });
    registry.register("Object.SubtractSubtract_Byte", |_ctx, args| {
        Ok(Value::Byte(numeric_int(args.first()) as u8))
    });
    // Pre-increment / pre-decrement: return the new value; the VM writes it
    // back via the compound-assignment path.
    registry.register("Object.AddAdd_PreInt", |_ctx, args| {
        Ok(Value::Int(
            numeric_int(args.first()).wrapping_add(1),
        ))
    });
    registry.register("Object.AddAdd_PreByte", |_ctx, args| {
        Ok(Value::Byte(
            numeric_int(args.first()).wrapping_add(1) as u8
        ))
    });
    registry.register("Object.SubtractSubtract_PreInt", |_ctx, args| {
        Ok(Value::Int(
            numeric_int(args.first()).wrapping_sub(1),
        ))
    });
    registry.register("Object.SubtractSubtract_PreByte", |_ctx, args| {
        Ok(Value::Byte(
            numeric_int(args.first()).wrapping_sub(1) as u8
        ))
    });
    registry.register("Object.MultiplyEqual_FloatFloat", |_ctx, args| {
        Ok(Value::Float(
            numeric_float(args.first()) * numeric_float(args.get(1)),
        ))
    });
    registry.register("Object.DivideEqual_FloatFloat", |_ctx, args| {
        let right = numeric_float(args.get(1));
        let value = if right == 0.0 {
            0.0
        } else {
            numeric_float(args.first()) / right
        };
        Ok(Value::Float(value))
    });
    registry.register("Object.AddEqual_FloatFloat", |_ctx, args| {
        Ok(Value::Float(
            numeric_float(args.first()) + numeric_float(args.get(1)),
        ))
    });
    registry.register("Object.SubtractEqual_FloatFloat", |_ctx, args| {
        Ok(Value::Float(
            numeric_float(args.first()) - numeric_float(args.get(1)),
        ))
    });
    registry.register("Object.MultiplyEqual_VectorFloat", |_ctx, args| {
        let vector = args.first().map(vector_of).unwrap_or_default();
        let scalar = numeric_float(args.get(1));
        Ok(Value::Vector(vector * scalar))
    });
    registry.register("Object.DivideEqual_VectorFloat", |_ctx, args| {
        let vector = args.first().map(vector_of).unwrap_or_default();
        let scalar = numeric_float(args.get(1));
        Ok(Value::Vector(if scalar == 0.0 {
            Vec3::ZERO
        } else {
            vector / scalar
        }))
    });
    registry.register("Object.AddEqual_VectorVector", |_ctx, args| {
        let left = args.first().map(vector_of).unwrap_or_default();
        let right = args.get(1).map(vector_of).unwrap_or_default();
        Ok(Value::Vector(left + right))
    });
    registry.register("Object.SubtractEqual_VectorVector", |_ctx, args| {
        let left = args.first().map(vector_of).unwrap_or_default();
        let right = args.get(1).map(vector_of).unwrap_or_default();
        Ok(Value::Vector(left - right))
    });
    registry.register("Object.MultiplyEqual_RotatorFloat", |_ctx, args| {
        let rotator = rotator_of(args.first());
        let scalar = numeric_float(args.get(1));
        Ok(Value::Rotator(scale_rotator(rotator, scalar)))
    });
    registry.register("Object.DivideEqual_RotatorFloat", |_ctx, args| {
        let rotator = rotator_of(args.first());
        let scalar = numeric_float(args.get(1));
        Ok(Value::Rotator(if scalar == 0.0 {
            [0, 0, 0]
        } else {
            scale_rotator(rotator, 1.0 / scalar)
        }))
    });
    registry.register("Object.AddEqual_RotatorRotator", |_ctx, args| {
        let left = rotator_of(args.first());
        let right = rotator_of(args.get(1));
        Ok(Value::Rotator([
            left[0].wrapping_add(right[0]),
            left[1].wrapping_add(right[1]),
            left[2].wrapping_add(right[2]),
        ]))
    });
    registry.register("Object.SubtractEqual_RotatorRotator", |_ctx, args| {
        let left = rotator_of(args.first());
        let right = rotator_of(args.get(1));
        Ok(Value::Rotator([
            left[0].wrapping_sub(right[0]),
            left[1].wrapping_sub(right[1]),
            left[2].wrapping_sub(right[2]),
        ]))
    });

    // Vector and rotator operators.
    registry.register("Object.EqualEqual_VectorVector", |_ctx, args| {
        compare("EqualEqual_VectorVector", args, |left, right| {
            vector_of(left) == vector_of(right)
        })
    });
    registry.register("Object.NotEqual_VectorVector", |_ctx, args| {
        compare("NotEqual_VectorVector", args, |left, right| {
            vector_of(left) != vector_of(right)
        })
    });
    registry.register("Object.Add_VectorVector", |_ctx, args| {
        let left = args.first().map(vector_of).unwrap_or_default();
        let right = args.get(1).map(vector_of).unwrap_or_default();
        Ok(Value::Vector(left + right))
    });
    registry.register("Object.Subtract_VectorVector", |_ctx, args| {
        let left = args.first().map(vector_of).unwrap_or_default();
        let right = args.get(1).map(vector_of).unwrap_or_default();
        Ok(Value::Vector(left - right))
    });
    registry.register("Object.Multiply_VectorFloat", |_ctx, args| {
        let vector = args.first().map(vector_of).unwrap_or_default();
        let scalar = args.get(1).and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Vector(vector * scalar))
    });
    registry.register("Object.Multiply_FloatVector", |_ctx, args| {
        let scalar = args.first().and_then(Value::as_float).unwrap_or(0.0);
        let vector = args.get(1).map(vector_of).unwrap_or_default();
        Ok(Value::Vector(vector * scalar))
    });
    registry.register("Object.Divide_VectorFloat", |_ctx, args| {
        let vector = args.first().map(vector_of).unwrap_or_default();
        let scalar = args.get(1).and_then(Value::as_float).unwrap_or(0.0);
        if scalar == 0.0 {
            Ok(Value::Vector(Vec3::ZERO))
        } else {
            Ok(Value::Vector(vector / scalar))
        }
    });
    registry.register("Object.Dot_VectorVector", |_ctx, args| {
        let left = args.first().map(vector_of).unwrap_or_default();
        let right = args.get(1).map(vector_of).unwrap_or_default();
        Ok(Value::Float(left.dot(right)))
    });
    registry.register("Object.Cross_VectorVector", |_ctx, args| {
        let left = args.first().map(vector_of).unwrap_or_default();
        let right = args.get(1).map(vector_of).unwrap_or_default();
        Ok(Value::Vector(left.cross(right)))
    });
    registry.register("Object.VSize", |_ctx, args| {
        Ok(Value::Float(
            args.first().map(vector_of).unwrap_or_default().length(),
        ))
    });
    registry.register("Object.Normal", |_ctx, args| {
        let vector = args.first().map(vector_of).unwrap_or_default();
        Ok(Value::Vector(vector.normalize_or_zero()))
    });
    registry.register("Object.Invert", |_ctx, args| {
        let vector = args.first().map(vector_of).unwrap_or_default();
        Ok(Value::Vector(-vector))
    });
    registry.register("Object.Subtract_PreVector", |_ctx, args| {
        let vector = args.first().map(vector_of).unwrap_or_default();
        Ok(Value::Vector(-vector))
    });
    registry.register("Object.EqualEqual_RotatorRotator", |_ctx, args| {
        compare("EqualEqual_RotatorRotator", args, |left, right| {
            matches!((left, right), (Value::Rotator(a), Value::Rotator(b)) if a == b)
        })
    });
    registry.register("Object.NotEqual_RotatorRotator", |_ctx, args| {
        compare("NotEqual_RotatorRotator", args, |left, right| {
            !matches!((left, right), (Value::Rotator(a), Value::Rotator(b)) if a == b)
        })
    });

    // Math.
    registry.register("Object.Abs", |_ctx, args| {
        let value = args.first().ok_or_else(|| arity("Abs"))?;
        if matches!(value, Value::Float(_)) {
            Ok(Value::Float(value.as_float().unwrap_or(0.0).abs()))
        } else {
            Ok(Value::Int(value.as_int().unwrap_or(0).abs()))
        }
    });
    registry.register("Object.Sin", |_ctx, args| {
        let value = args.first().and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Float(value.sin()))
    });
    registry.register("Object.Cos", |_ctx, args| {
        let value = args.first().and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Float(value.cos()))
    });
    registry.register("Object.Tan", |_ctx, args| {
        let value = args.first().and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Float(value.tan()))
    });
    registry.register("Object.Atan", |_ctx, args| {
        let value = args.first().and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Float(value.atan()))
    });
    registry.register("Object.Exp", |_ctx, args| {
        let value = args.first().and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Float(value.exp()))
    });
    registry.register("Object.Loge", |_ctx, args| {
        let value = args.first().and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Float(value.ln()))
    });
    registry.register("Object.Sqrt", |_ctx, args| {
        let value = args.first().and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Float(value.max(0.0).sqrt()))
    });
    registry.register("Object.Square", |_ctx, args| {
        let value = args.first().and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Float(value * value))
    });
    registry.register("Object.FRand", |ctx, _args| {
        Ok(Value::Float(ctx.next_random()))
    });
    registry.register("Object.Rand", |ctx, args| {
        let limit = args.first().and_then(Value::as_int).unwrap_or(0);
        if limit <= 0 {
            return Ok(Value::Int(0));
        }
        let value = (ctx.next_random() * limit as f32) as i32;
        Ok(Value::Int(value.min(limit - 1)))
    });
    registry.register("Object.FClamp", |_ctx, args| {
        let val = args
            .first()
            .and_then(Value::as_float)
            .unwrap_or(0.0);
        let min = args.get(1).and_then(Value::as_float).unwrap_or(0.0);
        let max = args.get(2).and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Float(val.clamp(min, max)))
    });
    registry.register("Object.FMax", |_ctx, args| {
        let a = args.first().and_then(Value::as_float).unwrap_or(0.0);
        let b = args.get(1).and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Float(a.max(b)))
    });
    registry.register("Object.FMin", |_ctx, args| {
        let a = args.first().and_then(Value::as_float).unwrap_or(0.0);
        let b = args.get(1).and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Float(a.min(b)))
    });
    registry.register("Object.Lerp", |_ctx, args| {
        let a = args.first().and_then(Value::as_float).unwrap_or(0.0);
        let b = args.get(1).and_then(Value::as_float).unwrap_or(0.0);
        let alpha = args.get(2).and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Float(a + (b - a) * alpha))
    });
    registry.register("Object.Smerp", |_ctx, args| {
        let a = args.first().and_then(Value::as_float).unwrap_or(0.0);
        let b = args.get(1).and_then(Value::as_float).unwrap_or(0.0);
        let alpha = args.get(2).and_then(Value::as_float).unwrap_or(0.0);
        // Smooth step: alpha^2 * (3 - 2*alpha)
        let s = alpha * alpha * (3.0 - 2.0 * alpha);
        Ok(Value::Float(a + (b - a) * s))
    });
    registry.register("Object.VRand", |ctx, _args| {
        // 返回单位长度的随机向量
        let mut x = ctx.next_random() * 2.0 - 1.0;
        let mut y = ctx.next_random() * 2.0 - 1.0;
        let mut z = ctx.next_random() * 2.0 - 1.0;
        let v = glam::Vec3::new(x, y, z);
        Ok(Value::Vector(v.normalize_or_zero()))
    });

    // State machine.
    registry.register("Object.GetStateName", |ctx, _args| {
        let state = ctx
            .self_object_id()
            .map(|id| ctx.vm.object_state(id).to_string())
            .unwrap_or_default();
        Ok(Value::Name(state))
    });
    registry.register("Object.IsInState", |ctx, args| {
        let state = args
            .first()
            .map(Value::to_display_string)
            .unwrap_or_default();
        let current = ctx
            .self_object_id()
            .map(|id| ctx.vm.object_state(id).to_string())
            .unwrap_or_default();
        Ok(Value::Bool(current.eq_ignore_ascii_case(&state)))
    });
    registry.register("Object.GotoState", |ctx, args| {
        let new_state = args
            .first()
            .map(Value::to_display_string)
            .unwrap_or_default();
        if let Some(object_id) = ctx.self_object_id() {
            ctx.vm.set_object_state(object_id, &new_state);
        }
        // The second argument is the bytecode label (script offset) to jump
        // to after the state change.
        if let Some(label) = args.get(1).and_then(Value::as_int) {
            if label >= 0 {
                ctx.set_pc(label as usize);
            }
        }
        Ok(Value::None)
    });

    // Object lifecycle.
    registry.register("Object.Destroy", |ctx, _args| {
        if let Some(object_id) = ctx.self_object_id() {
            ctx.vm.destroy_object(object_id);
        }
        Ok(Value::None)
    });
    registry.register("Engine.Actor.SetLocation", |ctx, args| {
        let location = args
            .first()
            .and_then(|v| match v {
                Value::Vector(vec) => Some(*vec),
                _ => None,
            })
            .unwrap_or(glam::Vec3::ZERO);
        if let Some(object_id) = ctx.self_object_id() {
            ctx.vm
                .set_object_property(object_id, "Location", Value::Vector(location));
        }
        Ok(Value::Bool(true))
    });
    registry.register("Engine.Actor.SetRotation", |ctx, args| {
        let rotation = args
            .first()
            .and_then(|v| match v {
                Value::Rotator(rot) => Some(*rot),
                _ => None,
            })
            .unwrap_or([0, 0, 0]);
        if let Some(object_id) = ctx.self_object_id() {
            ctx.vm
                .set_object_property(object_id, "Rotation", Value::Rotator(rotation));
        }
        Ok(Value::Bool(true))
    });
    registry.register("Actor.SetOwner", |ctx, args| {
        let owner = args
            .first()
            .and_then(|v| match v {
                Value::Object(id) => Some(*id),
                _ => None,
            })
            .unwrap_or(0u32);
        if let Some(object_id) = ctx.self_object_id() {
            ctx.vm
                .set_object_property(object_id, "Owner", Value::Object(owner));
        }
        Ok(Value::None)
    });

    // Collision.
    registry.register("Object.SetCollisionSize", |ctx, args| {
        let radius = args
            .first()
            .and_then(Value::as_float)
            .unwrap_or(0.0);
        let height = args
            .get(1)
            .and_then(Value::as_float)
            .unwrap_or(0.0);
        if let Some(object_id) = ctx.self_object_id() {
            ctx.vm.set_object_collision_size(object_id, radius, height);
        }
        Ok(Value::None)
    });
    registry.register("Object.SetCollision", |ctx, args| {
        let b_collide = args.first().map(Value::as_bool).unwrap_or(true);
        if let Some(object_id) = ctx.self_object_id() {
            ctx.vm.set_object_collision(object_id, b_collide);
        }
        Ok(Value::None)
    });

    // Timer.
    registry.register("Actor.SetTimer", |ctx, args| {
        let new_timer_rate = args
            .first()
            .and_then(Value::as_float)
            .unwrap_or(0.0);
        let b_loop = args.get(1).map(Value::as_bool).unwrap_or(false);
        if let Some(object_id) = ctx.self_object_id() {
            let timer = Timer {
                remaining: new_timer_rate,
                b_loop,
                function_name: "Timer".into(),
            };
            ctx.vm.add_object_timer(object_id, timer);
        }
        Ok(Value::None)
    });
    registry.register("Actor.ClearTimer", |ctx, _args| {
        if let Some(object_id) = ctx.self_object_id() {
            ctx.vm.clear_object_timers(object_id, "Timer");
        }
        Ok(Value::None)
    });

    // Actor spawning.
    registry.register("Actor.Spawn", |ctx, args| {
        let spawn_class: u32 = args
            .first()
            .and_then(|v| match v {
                Value::Object(id) => Some(*id),
                _ => None,
            })
            .unwrap_or(0);
        let _spawn_owner = args.get(1).and_then(|v| match v {
            Value::Object(id) => Some(id),
            _ => None,
        });
        let _spawn_tag = args.get(2).map(Value::to_display_string).unwrap_or_default();
        let spawn_location = args
            .get(3)
            .and_then(|v| match v {
                Value::Vector(vec) => Some(*vec),
                _ => None,
            })
            .unwrap_or(glam::Vec3::ZERO);
        let spawn_rotation = args
            .get(4)
            .and_then(|v| match v {
                Value::Rotator(rot) => Some(*rot),
                _ => None,
            })
            .unwrap_or([0, 0, 0]);

        if spawn_class != 0 {
            let class_path = ctx.vm.object_path(spawn_class).to_string();
            let new_id = ctx.vm.create_object_with_class(&class_path, &class_path);
            ctx.vm
                .set_object_property(new_id, "Location", Value::Vector(spawn_location));
            ctx.vm.set_object_property(new_id, "Rotation", Value::Rotator(spawn_rotation));
            Ok(Value::Object(new_id))
        } else {
            Ok(Value::None)
        }
    });

    // Sound playback (stub: no audio backend yet, returns 0).
    registry.register("Engine.Actor.PlaySound", |_ctx, _args| {
        Ok(Value::Int(0))
    });

    // Object identity and class queries.
    registry.register("Object.IsSame", |_ctx, args| {
        let left = args.first().and_then(|v| match v {
            Value::Object(id) => Some(id),
            _ => None,
        });
        let right = args.get(1).and_then(|v| match v {
            Value::Object(id) => Some(id),
            _ => None,
        });
        Ok(Value::Bool(left == right && left.is_some()))
    });
    registry.register("Object.GetClassName", |ctx, _args| {
        let class_name = ctx
            .self_object_id()
            .and_then(|id| ctx.vm.object(id))
            .map(|o| o.class_path.clone())
            .unwrap_or_default();
        Ok(Value::Name(class_name))
    });

    // Property text get/set.
    registry.register("Object.GetPropertyText", |ctx, args| {
        let prop_name = args.first().map(Value::to_display_string).unwrap_or_default();
        let object_id = ctx.self_object_id().unwrap_or(0);
        let value = ctx
            .vm
            .object(object_id)
            .and_then(|o| o.properties.get(&prop_name).cloned())
            .unwrap_or(Value::None);
        Ok(Value::String(Value::to_display_string(&value)))
    });
    registry.register("Object.SetPropertyText", |ctx, args| {
        let prop_name = args.first().map(Value::to_display_string).unwrap_or_default();
        let prop_text = args.get(1).map(Value::to_display_string).unwrap_or_default();
        if let Some(object_id) = ctx.self_object_id() {
            ctx.vm
                .set_object_property(object_id, prop_name, Value::String(prop_text));
        }
        Ok(Value::None)
    });

    // Class hierarchy query.
    registry.register("Object.ClassIsChildOf", |_ctx, args| {
        let child_class = args.first().map(Value::to_display_string).unwrap_or_default();
        let parent_class = args.get(1).map(Value::to_display_string).unwrap_or_default();
        if child_class.is_empty() || parent_class.is_empty() {
            return Ok(Value::Bool(false));
        }
        // Without a loaded class hierarchy, only exact matches (and universal
        // bases Object/Actor) are recognised.
        Ok(Value::Bool(
            child_class.eq_ignore_ascii_case(&parent_class)
                || parent_class.eq_ignore_ascii_case("Object")
                || parent_class.eq_ignore_ascii_case("Actor"),
        ))
    });

    // Dynamic object loading (stub: resolves by name path if already loaded).
    registry.register("Object.DynamicLoadObject", |ctx, args| {
        let name = args.first().map(Value::to_display_string).unwrap_or_default();
        if name.is_empty() {
            return Ok(Value::None);
        }
        ctx.vm.find_object_by_path(&name).map(Value::Object).ok_or_else(|| {
            VmError::UnknownNative(format!("DynamicLoadObject: {name} not found"))
        })
    });

    // Localized string lookup (stub: returns the key itself, since we have no
    // .int/.ucl localization tables loaded).
    registry.register("Object.Localize", |_ctx, args| {
        let key = args.first().map(Value::to_display_string).unwrap_or_default();
        Ok(Value::String(key))
    });

    // ========================================================================
    // Core.Object 更多数学/向量函数
    // ========================================================================

    /// Core.Object.GetAxes(Vector) -> Rotator
    registry.register("Object.GetAxes", |ctx, args| {
        let vec = args.first().and_then(|v| match v {
            Value::Vector(vec) => Some(*vec),
            _ => None,
        }).unwrap_or(glam::Vec3::ZERO);
        if let Some(object_id) = ctx.self_object_id() {
            ctx.vm.set_object_property(object_id, "Rotation", Value::Rotator([vec.x as i32, vec.y as i32, vec.z as i32]));
        }
        Ok(Value::Rotator([vec.x as i32, vec.y as i32, vec.z as i32]))
    });

    /// Core.Object.MirrorVectorByNormal(Vector, Vector) -> Vector
    registry.register("Object.MirrorVectorByNormal", |_ctx, args| {
        let vec = args.first().and_then(|v| match v {
            Value::Vector(v) => Some(*v),
            _ => None,
        }).unwrap_or(glam::Vec3::ZERO);
        let normal = args.get(1).and_then(|v| match v {
            Value::Vector(v) => Some(*v),
            _ => None,
        }).unwrap_or(glam::Vec3::new(0.0, 1.0, 0.0));
        let dot = vec.dot(normal);
        let reflected = vec - normal * (2.0 * dot);
        Ok(Value::Vector(reflected))
    });

    /// Core.Object.RotRand() -> Rotator
    registry.register("Object.RotRand", |ctx, _args| {
        let pitch = (ctx.next_random() * 65536.0) as i32;
        let yaw = (ctx.next_random() * 65536.0) as i32;
        let roll = (ctx.next_random() * 65536.0) as i32;
        Ok(Value::Rotator([pitch, yaw, roll]))
    });

    /// Core.Object.CyclesToSeconds(int) -> float
    registry.register("Object.CyclesToSeconds", |_ctx, args| {
        let cycles = args.first().and_then(Value::as_int).unwrap_or(0);
        Ok(Value::Float(cycles as f32 / 1000.0))
    });

    // ========================================================================
    // Engine.Pawn natives (AI 导航和寻路)
    // ========================================================================

    /// Engine.Pawn.AddPawn() -> Pawn
    registry.register("Engine.Pawn.AddPawn", |ctx, args| {
        let spawn_class = args.first().and_then(|v| match v {
            Value::Object(id) => Some(*id),
            _ => None,
        }).unwrap_or(0);
        if spawn_class != 0 {
            let class_path = ctx.vm.object_path(spawn_class).to_string();
            let new_id = ctx.vm.create_object_with_class(&class_path, &class_path);
            Ok(Value::Object(new_id))
        } else {
            Ok(Value::None)
        }
    });

    /// Engine.Pawn.RemovePawn() -> bool
    registry.register("Engine.Pawn.RemovePawn", |ctx, args| {
        let pawn = args.first().and_then(|v| match v {
            Value::Object(id) => Some(*id),
            _ => None,
        }).unwrap_or(0);
        if pawn != 0 {
            ctx.vm.destroy_object(pawn);
        }
        Ok(Value::Bool(true))
    });

    /// Engine.Pawn.PickTarget() -> Actor
    registry.register("Engine.Pawn.PickTarget", |ctx, _args| {
        if let Some(object_id) = ctx.self_object_id() {
            Ok(Value::Object(object_id))
        } else {
            Ok(Value::None)
        }
    });

    /// Engine.Pawn.LineOfSightTo(Actor, bool) -> bool
    registry.register("Engine.Pawn.LineOfSightTo", |ctx, args| {
        let _target = args.first().and_then(|v| match v {
            Value::Object(id) => Some(*id),
            _ => None,
        }).unwrap_or(0);
        let _b_ignore_distance = args.get(1).map(Value::as_bool).unwrap_or(false);
        if let Some(object_id) = ctx.self_object_id() {
            if let Some(_) = ctx.vm.object(object_id) {
                Ok(Value::Bool(true))
            } else {
                Ok(Value::Bool(false))
            }
        } else {
            Ok(Value::Bool(false))
        }
    });

    /// Engine.Pawn.CanSee(Actor) -> bool
    registry.register("Engine.Pawn.CanSee", |ctx, args| {
        let _target = args.first().and_then(|v| match v {
            Value::Object(id) => Some(*id),
            _ => None,
        }).unwrap_or(0);
        if let Some(object_id) = ctx.self_object_id() {
            if let Some(_) = ctx.vm.object(object_id) {
                Ok(Value::Bool(true))
            } else {
                Ok(Value::Bool(false))
            }
        } else {
            Ok(Value::Bool(false))
        }
    });

    // ========================================================================
    // ConSys.Conversation natives
    // ========================================================================

    /// ConSys.Conversation.BindEvents() -> void
    registry.register("ConSys.Conversation.BindEvents", |_ctx, _args| {
        Ok(Value::None)
    });

    /// ConSys.Conversation.ClearBindEvents() -> void
    registry.register("ConSys.Conversation.ClearBindEvents", |_ctx, _args| {
        Ok(Value::None)
    });

    /// ConSys.Conversation.GetSpeechAudio() -> Sound
    registry.register("ConSys.Conversation.GetSpeechAudio", |_ctx, _args| {
        Ok(Value::None)
    });

    // ========================================================================
    // DeusEx.DeusExPlayer natives
    // ========================================================================

    /// DeusEx.DeusExPlayer.ConBindEvents() -> void
    registry.register("DeusEx.DeusExPlayer.ConBindEvents", |_ctx, _args| {
        Ok(Value::None)
    });

    /// DeusEx.DeusExPlayer.CreateLogObject() -> Object
    registry.register("DeusEx.DeusExPlayer.CreateLogObject", |ctx, _args| {
        if let Some(object_id) = ctx.self_object_id() {
            let log_id = ctx.vm.create_object_with_class("Core.Object", "Core.Object");
            ctx.vm.set_object_property(log_id, "Log", Value::String("Log entry".into()));
            Ok(Value::Object(log_id))
        } else {
            Ok(Value::None)
        }
    });

    /// DeusEx.DeusExPlayer.SaveGame() -> bool
    registry.register("DeusEx.DeusExPlayer.SaveGame", |ctx, _args| {
        if let Some(object_id) = ctx.self_object_id() {
            ctx.vm.set_object_property(object_id, "Saved", Value::Bool(true));
            Ok(Value::Bool(true))
        } else {
            Ok(Value::Bool(false))
        }
    });

    // ========================================================================
    // Engine.Actor natives (core actor lifecycle / transform / query)
    // ========================================================================

    /// Engine.Actor.Destroy() -> bool
    registry.register("Engine.Actor.Destroy", |ctx, _args| {
        if let Some(object_id) = ctx.self_object_id() {
            ctx.vm.destroy_object(object_id);
        }
        Ok(Value::Bool(true))
    });

    /// Engine.Actor.SetCollision(bCollide: bool) -> bool
    registry.register("Engine.Actor.SetCollision", |ctx, args| {
        let b_collide = args.first().map(Value::as_bool).unwrap_or(true);
        if let Some(object_id) = ctx.self_object_id() {
            ctx.vm.set_object_collision(object_id, b_collide);
        }
        Ok(Value::Bool(true))
    });

    /// Engine.Actor.SetBase(newBase: Actor) -> bool
    registry.register("Engine.Actor.SetBase", |ctx, args| {
        let new_base = args.first().and_then(|v| match v {
            Value::Object(id) => Some(*id),
            _ => None,
        }).unwrap_or(0);
        if let Some(object_id) = ctx.self_object_id() {
            ctx.vm.set_object_property(object_id, "Base", Value::Object(new_base));
        }
        Ok(Value::Bool(true))
    });

    /// Engine.Actor.SetPhysics(newPhysics: byte) -> bool
    registry.register("Engine.Actor.SetPhysics", |ctx, args| {
        let new_physics = args.first().and_then(Value::as_int).unwrap_or(0) as u8;
        if let Some(object_id) = ctx.self_object_id() {
            ctx.vm.set_object_property(object_id, "Physics", Value::Byte(new_physics));
        }
        Ok(Value::Bool(true))
    });

    /// Engine.Actor.GetPlayerPawn() -> Pawn
    registry.register("Engine.Actor.GetPlayerPawn", |ctx, _args| {
        if let Some(object_id) = ctx.self_object_id() {
            if let Some(obj) = ctx.vm.object(object_id) {
                if obj.class_path.contains("Pawn") || obj.class_path.contains("PlayerPawn") {
                    return Ok(Value::Object(object_id));
                }
            }
        }
        Ok(Value::None)
    });

    /// Engine.Actor.Trace() -> Actor
    registry.register("Engine.Actor.Trace", |ctx, args| {
        let hit_location = args.get(0).and_then(|v| match v {
            Value::Vector(v) => Some(*v),
            _ => None,
        }).unwrap_or(glam::Vec3::ZERO);
        let _hit_normal = args.get(1).and_then(|v| match v {
            Value::Vector(v) => Some(*v),
            _ => None,
        }).unwrap_or(glam::Vec3::ZERO);
        let trace_end = args.get(2).and_then(|v| match v {
            Value::Vector(v) => Some(*v),
            _ => None,
        }).unwrap_or(glam::Vec3::ZERO);
        let _trace_start = args.get(3).and_then(|v| match v {
            Value::Vector(v) => Some(*v),
            _ => None,
        }).unwrap_or(glam::Vec3::ZERO);
        let _b_trace_actors = args.get(4).map(Value::as_bool).unwrap_or(false);
        let _extent = args.get(5).and_then(|v| match v {
            Value::Vector(v) => Some(*v),
            _ => None,
        }).unwrap_or(glam::Vec3::ZERO);

        // Stub: no collision world yet, return None.
        Ok(Value::None)
    });

    /// Engine.Actor.SetLocation(NewLocation: Vector) -> bool
    registry.register("Engine.Actor.SetLocation", |ctx, args| {
        let location = args
            .first()
            .and_then(|v| match v {
                Value::Vector(vec) => Some(*vec),
                _ => None,
            })
            .unwrap_or(glam::Vec3::ZERO);
        if let Some(object_id) = ctx.self_object_id() {
            ctx.vm
                .set_object_property(object_id, "Location", Value::Vector(location));
        }
        Ok(Value::Bool(true))
    });

    /// Engine.Actor.SetRotation(NewRotation: Rotator) -> bool
    registry.register("Engine.Actor.SetRotation", |ctx, args| {
        let rotation = args
            .first()
            .and_then(|v| match v {
                Value::Rotator(rot) => Some(*rot),
                _ => None,
            })
            .unwrap_or([0, 0, 0]);
        if let Some(object_id) = ctx.self_object_id() {
            ctx.vm
                .set_object_property(object_id, "Rotation", Value::Rotator(rotation));
        }
        Ok(Value::Bool(true))
    });

    // ========================================================================
    // Core.Object arithmetic / comparison / conversion operators
    // ========================================================================

    // Integer arithmetic
    registry.register("Object.Add_IntInt", |_ctx, args| {
        let left: i32 = args.first().and_then(Value::as_int).unwrap_or(0);
        let right: i32 = args.get(1).and_then(Value::as_int).unwrap_or(0);
        Ok(Value::Int(left.wrapping_add(right)))
    });
    registry.register("Object.Subtract_IntInt", |_ctx, args| {
        let left: i32 = args.first().and_then(Value::as_int).unwrap_or(0);
        let right: i32 = args.get(1).and_then(Value::as_int).unwrap_or(0);
        Ok(Value::Int(left.wrapping_sub(right)))
    });
    registry.register("Object.Multiply_IntInt", |_ctx, args| {
        let left: i32 = args.first().and_then(Value::as_int).unwrap_or(0);
        let right: i32 = args.get(1).and_then(Value::as_int).unwrap_or(0);
        Ok(Value::Int(left.wrapping_mul(right)))
    });
    registry.register("Object.Divide_IntInt", |_ctx, args| {
        let left: i32 = args.first().and_then(Value::as_int).unwrap_or(0);
        let right: i32 = args.get(1).and_then(Value::as_int).unwrap_or(0);
        Ok(Value::Int(if right == 0 { 0 } else { left.wrapping_div(right) }))
    });
    registry.register("Object.Percent_IntInt", |_ctx, args| {
        let left: i32 = args.first().and_then(Value::as_int).unwrap_or(0);
        let right: i32 = args.get(1).and_then(Value::as_int).unwrap_or(0);
        Ok(Value::Int(if right == 0 { 0 } else { left.wrapping_rem(right) }))
    });
    registry.register("Object.And_IntInt", |_ctx, args| {
        let left: i32 = args.first().and_then(Value::as_int).unwrap_or(0);
        let right: i32 = args.get(1).and_then(Value::as_int).unwrap_or(0);
        Ok(Value::Int(left & right))
    });
    registry.register("Object.Or_IntInt", |_ctx, args| {
        let left: i32 = args.first().and_then(Value::as_int).unwrap_or(0);
        let right: i32 = args.get(1).and_then(Value::as_int).unwrap_or(0);
        Ok(Value::Int(left | right))
    });
    registry.register("Object.Xor_IntInt", |_ctx, args| {
        let left: i32 = args.first().and_then(Value::as_int).unwrap_or(0);
        let right: i32 = args.get(1).and_then(Value::as_int).unwrap_or(0);
        Ok(Value::Int(left ^ right))
    });
    registry.register("Object.Complement_PreInt", |_ctx, args| {
        let left: i32 = args.first().and_then(Value::as_int).unwrap_or(0);
        Ok(Value::Int(!left))
    });
    registry.register("Object.LessLess_IntInt", |_ctx, args| {
        let left: i32 = args.first().and_then(Value::as_int).unwrap_or(0);
        let right: i32 = args.get(1).and_then(Value::as_int).unwrap_or(0);
        Ok(Value::Int(left.wrapping_shl(right as u32)))
    });
    registry.register("Object.GreaterGreater_IntInt", |_ctx, args| {
        let left: i32 = args.first().and_then(Value::as_int).unwrap_or(0);
        let right: i32 = args.get(1).and_then(Value::as_int).unwrap_or(0);
        Ok(Value::Int(left.wrapping_shr(right as u32)))
    });

    // Integer comparisons
    registry.register("Object.EqualEqual_IntInt", |_ctx, args| {
        let left: i32 = args.first().and_then(Value::as_int).unwrap_or(0);
        let right: i32 = args.get(1).and_then(Value::as_int).unwrap_or(0);
        Ok(Value::Bool(left == right))
    });
    registry.register("Object.NotEqual_IntInt", |_ctx, args| {
        let left: i32 = args.first().and_then(Value::as_int).unwrap_or(0);
        let right: i32 = args.get(1).and_then(Value::as_int).unwrap_or(0);
        Ok(Value::Bool(left != right))
    });
    registry.register("Object.Less_IntInt", |_ctx, args| {
        let left: i32 = args.first().and_then(Value::as_int).unwrap_or(0);
        let right: i32 = args.get(1).and_then(Value::as_int).unwrap_or(0);
        Ok(Value::Bool(left < right))
    });
    registry.register("Object.Greater_IntInt", |_ctx, args| {
        let left: i32 = args.first().and_then(Value::as_int).unwrap_or(0);
        let right: i32 = args.get(1).and_then(Value::as_int).unwrap_or(0);
        Ok(Value::Bool(left > right))
    });
    registry.register("Object.LessEqual_IntInt", |_ctx, args| {
        let left: i32 = args.first().and_then(Value::as_int).unwrap_or(0);
        let right: i32 = args.get(1).and_then(Value::as_int).unwrap_or(0);
        Ok(Value::Bool(left <= right))
    });
    registry.register("Object.GreaterEqual_IntInt", |_ctx, args| {
        let left: i32 = args.first().and_then(Value::as_int).unwrap_or(0);
        let right: i32 = args.get(1).and_then(Value::as_int).unwrap_or(0);
        Ok(Value::Bool(left >= right))
    });

    // Float arithmetic
    registry.register("Object.Add_FloatFloat", |_ctx, args| {
        let left: f32 = args.first().and_then(Value::as_float).unwrap_or(0.0);
        let right: f32 = args.get(1).and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Float(left + right))
    });
    registry.register("Object.Subtract_FloatFloat", |_ctx, args| {
        let left: f32 = args.first().and_then(Value::as_float).unwrap_or(0.0);
        let right: f32 = args.get(1).and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Float(left - right))
    });
    registry.register("Object.Multiply_FloatFloat", |_ctx, args| {
        let left: f32 = args.first().and_then(Value::as_float).unwrap_or(0.0);
        let right: f32 = args.get(1).and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Float(left * right))
    });
    registry.register("Object.Divide_FloatFloat", |_ctx, args| {
        let left: f32 = args.first().and_then(Value::as_float).unwrap_or(0.0);
        let right: f32 = args.get(1).and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Float(if right == 0.0 { 0.0 } else { left / right }))
    });
    registry.register("Object.Percent_FloatFloat", |_ctx, args| {
        let left: f32 = args.first().and_then(Value::as_float).unwrap_or(0.0);
        let right: f32 = args.get(1).and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Float(if right == 0.0 { 0.0 } else { left % right }))
    });

    // Float comparisons
    registry.register("Object.EqualEqual_FloatFloat", |_ctx, args| {
        let left: f32 = args.first().and_then(Value::as_float).unwrap_or(0.0);
        let right: f32 = args.get(1).and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Bool(left == right))
    });
    registry.register("Object.NotEqual_FloatFloat", |_ctx, args| {
        let left: f32 = args.first().and_then(Value::as_float).unwrap_or(0.0);
        let right: f32 = args.get(1).and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Bool(left != right))
    });
    registry.register("Object.Less_FloatFloat", |_ctx, args| {
        let left: f32 = args.first().and_then(Value::as_float).unwrap_or(0.0);
        let right: f32 = args.get(1).and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Bool(left < right))
    });
    registry.register("Object.Greater_FloatFloat", |_ctx, args| {
        let left: f32 = args.first().and_then(Value::as_float).unwrap_or(0.0);
        let right: f32 = args.get(1).and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Bool(left > right))
    });
    registry.register("Object.LessEqual_FloatFloat", |_ctx, args| {
        let left: f32 = args.first().and_then(Value::as_float).unwrap_or(0.0);
        let right: f32 = args.get(1).and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Bool(left <= right))
    });
    registry.register("Object.GreaterEqual_FloatFloat", |_ctx, args| {
        let left: f32 = args.first().and_then(Value::as_float).unwrap_or(0.0);
        let right: f32 = args.get(1).and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Bool(left >= right))
    });

    // String comparisons
    registry.register("Object.EqualEqual_StrStr", |_ctx, args| {
        let left: String = args.first().map(Value::to_display_string).unwrap_or_default();
        let right: String = args.get(1).map(Value::to_display_string).unwrap_or_default();
        Ok(Value::Bool(left == right))
    });
    registry.register("Object.NotEqual_StrStr", |_ctx, args| {
        let left: String = args.first().map(Value::to_display_string).unwrap_or_default();
        let right: String = args.get(1).map(Value::to_display_string).unwrap_or_default();
        Ok(Value::Bool(left != right))
    });
    registry.register("Object.Less_StrStr", |_ctx, args| {
        let left: String = args.first().map(Value::to_display_string).unwrap_or_default();
        let right: String = args.get(1).map(Value::to_display_string).unwrap_or_default();
        Ok(Value::Bool(left < right))
    });
    registry.register("Object.Greater_StrStr", |_ctx, args| {
        let left: String = args.first().map(Value::to_display_string).unwrap_or_default();
        let right: String = args.get(1).map(Value::to_display_string).unwrap_or_default();
        Ok(Value::Bool(left > right))
    });
    registry.register("Object.LessEqual_StrStr", |_ctx, args| {
        let left: String = args.first().map(Value::to_display_string).unwrap_or_default();
        let right: String = args.get(1).map(Value::to_display_string).unwrap_or_default();
        Ok(Value::Bool(left <= right))
    });
    registry.register("Object.GreaterEqual_StrStr", |_ctx, args| {
        let left: String = args.first().map(Value::to_display_string).unwrap_or_default();
        let right: String = args.get(1).map(Value::to_display_string).unwrap_or_default();
        Ok(Value::Bool(left >= right))
    });

    // Name comparisons (case-insensitive like strings)
    registry.register("Object.EqualEqual_NameName", |_ctx, args| {
        let left: String = args.first().map(Value::to_display_string).unwrap_or_default().to_lowercase();
        let right: String = args.get(1).map(Value::to_display_string).unwrap_or_default().to_lowercase();
        Ok(Value::Bool(left == right))
    });
    registry.register("Object.NotEqual_NameName", |_ctx, args| {
        let left: String = args.first().map(Value::to_display_string).unwrap_or_default().to_lowercase();
        let right: String = args.get(1).map(Value::to_display_string).unwrap_or_default().to_lowercase();
        Ok(Value::Bool(left != right))
    });

    // Object comparisons
    registry.register("Object.EqualEqual_ObjectObject", |_ctx, args| {
        let left: u32 = args.first().and_then(Value::as_object).unwrap_or(0);
        let right: u32 = args.get(1).and_then(Value::as_object).unwrap_or(0);
        Ok(Value::Bool(left == right))
    });
    registry.register("Object.NotEqual_ObjectObject", |_ctx, args| {
        let left: u32 = args.first().and_then(Value::as_object).unwrap_or(0);
        let right: u32 = args.get(1).and_then(Value::as_object).unwrap_or(0);
        Ok(Value::Bool(left != right))
    });

    // Boolean operators
    registry.register("Object.Not_PreBool", |_ctx, args| {
        let left: bool = args.first().map(Value::as_bool).unwrap_or(false);
        Ok(Value::Bool(!left))
    });
    registry.register("Object.AndAnd_BoolBool", |_ctx, args| {
        let left: bool = args.first().map(Value::as_bool).unwrap_or(false);
        let right: bool = args.get(1).map(Value::as_bool).unwrap_or(false);
        Ok(Value::Bool(left && right))
    });
    registry.register("Object.OrOr_BoolBool", |_ctx, args| {
        let left: bool = args.first().map(Value::as_bool).unwrap_or(false);
        let right: bool = args.get(1).map(Value::as_bool).unwrap_or(false);
        Ok(Value::Bool(left || right))
    });
    registry.register("Object.XorXor_BoolBool", |_ctx, args| {
        let left: bool = args.first().map(Value::as_bool).unwrap_or(false);
        let right: bool = args.get(1).map(Value::as_bool).unwrap_or(false);
        Ok(Value::Bool(left ^ right))
    });

    // Concat / At string operators
    registry.register("Object.Concat_StrStr", |_ctx, args| {
        let left: String = args.first().map(Value::to_display_string).unwrap_or_default();
        let right: String = args.get(1).map(Value::to_display_string).unwrap_or_default();
        Ok(Value::String(format!("{left}{right}")))
    });
    registry.register("Object.At_StrStr", |_ctx, args| {
        let left: String = args.first().map(Value::to_display_string).unwrap_or_default();
        let right: String = args.get(1).map(Value::to_display_string).unwrap_or_default();
        Ok(Value::String(format!("{left} {right}")))
    });

    registry.register("ConSys.Conversation.BindActorEvents", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("ConSys.Conversation.CreateConCamera", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("ConSys.Conversation.CreateFlagRef", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("ConSys.Conversation.GetSpeechLength", |_ctx, args| {
        let val: f32 = args.first().and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Float(val))
    });
    registry.register("Core.Object.Abs", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.AddAdd_Byte", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.AddAdd_Int", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.AddAdd_PreByte", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.AddAdd_PreInt", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.AddEqual_ByteByte", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.AddEqual_FloatFloat", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.AddEqual_IntInt", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.AddEqual_RotatorRotator", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.AddEqual_VectorVector", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Add_FloatFloat", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Add_IntInt", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Add_RotatorRotator", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Add_VectorVector", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.AllObjects", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.AndAnd_BoolBool", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.And_IntInt", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Asc", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.At_StrStr", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Atan", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Caps", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Chr", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Clamp", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.ClassIsChildOf", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.ComplementEqual_FloatFloat", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.ComplementEqual_StrStr", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Complement_PreInt", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Concat_StrStr", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Cos", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.CriticalDelete", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Cross_VectorVector", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.CyclesToSeconds", |_ctx, args| {
        let val: f32 = args.first().and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Float(val))
    });
    registry.register("Core.Object.Disable", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.DivideEqual_ByteByte", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.DivideEqual_FloatFloat", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.DivideEqual_IntFloat", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.DivideEqual_RotatorFloat", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.DivideEqual_VectorFloat", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Divide_FloatFloat", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Divide_IntInt", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Divide_RotatorFloat", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Divide_VectorFloat", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Dot_VectorVector", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Enable", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.EqualEqual_BoolBool", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.EqualEqual_FloatFloat", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.EqualEqual_IntInt", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.EqualEqual_NameName", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.EqualEqual_ObjectObject", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.EqualEqual_RotatorRotator", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.EqualEqual_StrStr", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.EqualEqual_VectorVector", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Exp", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.FClamp", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.FMax", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.FMin", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.FRand", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.GetAxes", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.GetStateName", |_ctx, _args| {
        Ok(Value::Name(String::new()))
    });
    registry.register("Core.Object.GetUnAxes", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.GotoState", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.GreaterEqual_FloatFloat", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.GreaterEqual_IntInt", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.GreaterEqual_StrStr", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.GreaterGreaterGreater_IntInt", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.GreaterGreater_IntInt", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.GreaterGreater_VectorRotator", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Greater_FloatFloat", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Greater_IntInt", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Greater_StrStr", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.InStr", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Invert", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.IsA", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("Core.Object.IsInState", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("Core.Object.Left", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Len", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Lerp", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.LessEqual_FloatFloat", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.LessEqual_IntInt", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.LessEqual_StrStr", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.LessLess_IntInt", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.LessLess_VectorRotator", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Less_FloatFloat", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Less_IntInt", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Less_StrStr", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Log", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Loge", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Max", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Mid", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Min", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.MirrorVectorByNormal", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.MultiplyEqual_ByteByte", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.MultiplyEqual_FloatFloat", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.MultiplyEqual_IntFloat", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.MultiplyEqual_RotatorFloat", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.MultiplyEqual_VectorFloat", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.MultiplyEqual_VectorVector", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.MultiplyMultiply_FloatFloat", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Multiply_FloatFloat", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Multiply_FloatRotator", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Multiply_FloatVector", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Multiply_IntInt", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Multiply_RotatorFloat", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Multiply_VectorFloat", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Multiply_VectorVector", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Normal", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.NotEqual_BoolBool", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.NotEqual_FloatFloat", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.NotEqual_IntInt", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.NotEqual_NameName", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.NotEqual_ObjectObject", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.NotEqual_RotatorRotator", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.NotEqual_StrStr", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.NotEqual_VectorVector", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Not_PreBool", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.OrOr_BoolBool", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Or_IntInt", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Percent_FloatFloat", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Rand", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Right", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.RotRand", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.SaveConfig", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Sin", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Smerp", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Sqrt", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Square", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.SubtractEqual_ByteByte", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.SubtractEqual_FloatFloat", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.SubtractEqual_IntInt", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.SubtractEqual_RotatorRotator", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.SubtractEqual_VectorVector", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.SubtractSubtract_Byte", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.SubtractSubtract_Int", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.SubtractSubtract_PreByte", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.SubtractSubtract_PreInt", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Subtract_FloatFloat", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Subtract_IntInt", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Subtract_PreFloat", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Subtract_PreInt", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Subtract_PreVector", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Subtract_RotatorRotator", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Subtract_VectorVector", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Tan", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.VRand", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.VSize", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Warn", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.XorXor_BoolBool", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.Xor_IntInt", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.clock", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Core.Object.unclock", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("DeusEx.DeusExDecoration.ConBindEvents", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("DeusEx.DeusExPlayer.CreateDataVaultImageNoteObject", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("DeusEx.DeusExPlayer.CreateDumpLocationObject", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("DeusEx.DeusExPlayer.CreateGameDirectoryObject", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("DeusEx.DeusExPlayer.CreateHistoryEvent", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("DeusEx.DeusExPlayer.CreateHistoryObject", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("DeusEx.DeusExPlayer.DeleteSaveGameFiles", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("DeusEx.DeusExPlayer.GetDeusExVersion", |_ctx, _args| {
        Ok(Value::String(String::new()))
    });
    registry.register("DeusEx.DeusExPlayer.SetBoolFlagFromString", |_ctx, _args| {
        Ok(Value::Name(String::new()))
    });
    registry.register("DeusEx.DeusExPlayer.UnloadTexture", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("DeusEx.DeusExSaveInfo.UpdateTimeStamp", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("DeusEx.ParticleIterator.UpdateParticles", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("DeusEx.ScriptedPawn.AddCarcass", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("DeusEx.ScriptedPawn.ConBindEvents", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("DeusEx.ScriptedPawn.GetAllianceType", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("DeusEx.ScriptedPawn.GetPawnAllianceType", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("DeusEx.ScriptedPawn.HaveSeenCarcass", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("DeusEx.ScriptedPawn.IsValidEnemy", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("DeusExText.DeusExTextParser.CloseText", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("DeusExText.DeusExTextParser.GetColor", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("DeusExText.DeusExTextParser.GetEmailInfo", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("DeusExText.DeusExTextParser.GetFileInfo", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("DeusExText.DeusExTextParser.GetName", |_ctx, _args| {
        Ok(Value::Name(String::new()))
    });
    registry.register("DeusExText.DeusExTextParser.GetTag", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("DeusExText.DeusExTextParser.GetText", |_ctx, _args| {
        Ok(Value::String(String::new()))
    });
    registry.register("DeusExText.DeusExTextParser.GotoLabel", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("DeusExText.DeusExTextParser.IsEOF", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("DeusExText.DeusExTextParser.OpenText", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("DeusExText.DeusExTextParser.ProcessText", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("DeusExText.DeusExTextParser.SetPlayerName", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.AIClearEvent", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.AIClearEventCallback", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.AIEndEvent", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.AIGetLightLevel", |_ctx, args| {
        let val: f32 = args.first().and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Float(val))
    });
    registry.register("Engine.Actor.AISendEvent", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.AISetEventCallback", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.AIStartEvent", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.AIVisibility", |_ctx, args| {
        let val: f32 = args.first().and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Float(val))
    });
    registry.register("Engine.Actor.Add_ColorColor", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.AllActors", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.AutonomousPhysics", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.BasedActors", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.ChildActors", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.CycleActors", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.Error", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.FastTrace", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("Engine.Actor.FinishAnim", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.FinishInterpolation", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.GetAnimGroup", |_ctx, _args| {
        Ok(Value::Name(String::new()))
    });
    registry.register("Engine.Actor.GetBoundingBox", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("Engine.Actor.GetMapName", |_ctx, _args| {
        Ok(Value::String(String::new()))
    });
    registry.register("Engine.Actor.GetMeshTexture", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.GetNextSkin", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.GetURLMap", |_ctx, _args| {
        Ok(Value::String(String::new()))
    });
    registry.register("Engine.Actor.HasAnim", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("Engine.Actor.InStasis", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("Engine.Actor.IsAnimating", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("Engine.Actor.IsOverlapping", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("Engine.Actor.LastRendered", |_ctx, args| {
        let val: f32 = args.first().and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Float(val))
    });
    registry.register("Engine.Actor.LoopAnim", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.MakeNoise", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.Move", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("Engine.Actor.MoveSmooth", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("Engine.Actor.Multiply_ColorFloat", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.Multiply_FloatColor", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.ParabolicTrace", |_ctx, args| {
        let val: f32 = args.first().and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Float(val))
    });
    registry.register("Engine.Actor.PlayAnim", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.PlayBlendAnim", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.PlayerCanSeeMe", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("Engine.Actor.RadiusActors", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.RandomBiasedRotation", |_ctx, _args| {
        Ok(Value::Rotator([0, 0, 0]))
    });
    registry.register("Engine.Actor.SetCollisionSize", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("Engine.Actor.SetInstantMusicVolume", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.SetInstantSoundVolume", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.SetInstantSpeechVolume", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.SetOwner", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.SetTimer", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.Sleep", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.Spawn", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.StopSound", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.Subtract_ColorColor", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.TouchingActors", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.TraceActors", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.TraceTexture", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.TraceVisibleActors", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.TweenAnim", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.TweenBlendAnim", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.VisibleActors", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Actor.VisibleCollidingActors", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.LevelInfo.InitEventManager", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.NavigationPoint.describeSpec", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Pawn.AICanHear", |_ctx, args| {
        let val: f32 = args.first().and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Float(val))
    });
    registry.register("Engine.Pawn.AICanSee", |_ctx, args| {
        let val: f32 = args.first().and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Float(val))
    });
    registry.register("Engine.Pawn.AICanSmell", |_ctx, args| {
        let val: f32 = args.first().and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Float(val))
    });
    registry.register("Engine.Pawn.AIDirectionReachable", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("Engine.Pawn.AIPickRandomDestination", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("Engine.Pawn.ClearPaths", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Pawn.ComputePathnodeDistances", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Pawn.EAdjustJump", |_ctx, _args| {
        Ok(Value::Vector(glam::Vec3::ZERO))
    });
    registry.register("Engine.Pawn.FindBestInventoryPath", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Pawn.FindPathTo", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Pawn.FindPathToward", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Pawn.FindRandomDest", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Pawn.FindStairRotation", |_ctx, _args| {
        Ok(Value::Int(0))
    });
    registry.register("Engine.Pawn.MoveTo", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Pawn.MoveToward", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Pawn.PickAnyTarget", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Pawn.PickWallAdjust", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("Engine.Pawn.ReachablePathnodes", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Pawn.StrafeFacing", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Pawn.StrafeTo", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Pawn.TurnTo", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Pawn.TurnToward", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Pawn.WaitForLanding", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.Pawn.actorReachable", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("Engine.Pawn.pointReachable", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("Engine.PlayerPawn.ResetKeyboard", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.PlayerPawn.UpdateURL", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.WarpZoneInfo.UnWarp", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.WarpZoneInfo.Warp", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Engine.ZoneInfo.ZoneActors", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.ExtString.GetText", |_ctx, _args| {
        Ok(Value::String(String::new()))
    });
    registry.register("Extension.FlagBase.GetName", |_ctx, _args| {
        Ok(Value::Name(String::new()))
    });
    registry.register("Extension.GC.EnableSpecialText", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.GC.EnableTranslucentText", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.GC.SetBaselineData", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.GC.SetBoldFont", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.GC.SetFont", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.GC.SetFonts", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.GC.SetNormalFont", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.GC.SetTextColor", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.GC.SetTileColor", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.TextWindow.GetText", |_ctx, _args| {
        Ok(Value::String(String::new()))
    });
    registry.register("Extension.ViewportWindow.SetRotation", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.AddActorRef", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.AddTimer", |_ctx, _args| {
        Ok(Value::Int(0))
    });
    registry.register("Extension.Window.AskParentForReconfigure", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.AskParentToShowArea", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.CarriageReturn", |_ctx, _args| {
        Ok(Value::String(String::new()))
    });
    registry.register("Extension.Window.ChangeStyle", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.ConfigureChild", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.ConvertCoordinates", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.ConvertScriptString", |_ctx, _args| {
        Ok(Value::String(String::new()))
    });
    registry.register("Extension.Window.ConvertVectorToCoordinates", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("Extension.Window.Destroy", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.DestroyAllChildren", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.EnableSpecialText", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.EnableTranslucentText", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.FindWindow", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.GetBottomChild", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.GetClientObject", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.GetCursorPos", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.GetFocusWindow", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.GetGC", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.GetHigherSibling", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.GetLowerSibling", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.GetModalWindow", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.GetParent", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.GetPlayerPawn", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.GetRootWindow", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.GetTabGroupWindow", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.GetTickOffset", |_ctx, args| {
        let val: f32 = args.first().and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Float(val))
    });
    registry.register("Extension.Window.GetTopChild", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.GrabMouse", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.Hide", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.IsActorValid", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("Extension.Window.IsFocusWindow", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("Extension.Window.IsKeyDown", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("Extension.Window.IsPointInWindow", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("Extension.Window.IsSensitive", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("Extension.Window.IsVisible", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("Extension.Window.Lower", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.MoveFocusDown", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.MoveFocusLeft", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.MoveFocusRight", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.MoveFocusUp", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.MoveTabGroupNext", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.MoveTabGroupPrev", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.NewChild", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.PlaySound", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.QueryGranularity", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.QueryPreferredHeight", |_ctx, args| {
        let val: f32 = args.first().and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Float(val))
    });
    registry.register("Extension.Window.QueryPreferredSize", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.QueryPreferredWidth", |_ctx, args| {
        let val: f32 = args.first().and_then(Value::as_float).unwrap_or(0.0);
        Ok(Value::Float(val))
    });
    registry.register("Extension.Window.Raise", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.ReleaseGC", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.RemoveActorRef", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.RemoveTimer", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.ResetHeight", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.ResetSize", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.ResetWidth", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.ResizeChild", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.SetAcceleratorText", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.SetBackground", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.SetBackgroundSmoothing", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.SetBackgroundStretching", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.SetBackgroundStyle", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.SetBaselineData", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.SetBoldFont", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.SetChildVisibility", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.SetClientObject", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.SetConfiguration", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.SetCursorPos", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.SetDefaultCursor", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.SetFocusSounds", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.SetFocusWindow", |_ctx, _args| {
        Ok(Value::Bool(false))
    });
    registry.register("Extension.Window.SetFont", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.SetFonts", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.SetHeight", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.SetNormalFont", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.SetPos", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.SetSelectability", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.SetSensitivity", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.SetSize", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.SetSoundVolume", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.SetTextColor", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.SetTileColor", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.SetVisibilitySounds", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.SetWidth", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.SetWindowAlignments", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.Show", |_ctx, _args| {
        Ok(Value::None)
    });
    registry.register("Extension.Window.UngrabMouse", |_ctx, _args| {
        Ok(Value::None)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_resolves_full_and_short_names() {
        let registry = NativeRegistry::standard();
        assert!(registry.contains("Object.Log"));
        assert!(registry.contains("Log"));
        assert!(registry.contains("Object.Concat_StrStr"));
        assert!(!registry.contains("Object.DoesNotExist"));
    }
}
