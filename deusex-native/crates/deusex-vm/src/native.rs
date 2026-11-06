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
