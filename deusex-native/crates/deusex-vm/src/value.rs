//! Values manipulated by the UnrealScript interpreter.
//!
//! UnrealScript is dynamically typed at the bytecode level: operators are
//! native functions and conversions are separate opcodes. `Value` therefore
//! carries a tag plus the payload, and coercion is explicit through
//! [`Value::convert`].

use std::collections::HashMap;

use glam::Vec3;

const TAU: f32 = std::f32::consts::TAU;

/// Generic UnrealScript struct value.
///
/// Vectors and rotators keep their dedicated variants for arithmetic; other
/// structs store fields by name.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StructValue {
    pub type_name: String,
    pub fields: HashMap<String, Value>,
}

/// A runtime value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// `None`: null object reference or "no value".
    None,
    Bool(bool),
    Byte(u8),
    Int(i32),
    Float(f32),
    String(String),
    Name(String),
    Vector(Vec3),
    Rotator([i32; 3]),
    Struct(StructValue),
    /// Handle into the VM object table.
    Object(u32),
    Array(Vec<Value>),
}

impl Value {
    /// Human-readable type name, used in VM diagnostics.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Bool(_) => "bool",
            Self::Byte(_) => "byte",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::String(_) => "string",
            Self::Name(_) => "name",
            Self::Vector(_) => "vector",
            Self::Rotator(_) => "rotator",
            Self::Struct(_) => "struct",
            Self::Object(_) => "object",
            Self::Array(_) => "array",
        }
    }

    /// UnrealScript truthiness: non-zero numbers, non-empty strings, non-null
    /// objects, non-zero vectors.
    pub fn as_bool(&self) -> bool {
        match self {
            Self::None => false,
            Self::Bool(value) => *value,
            Self::Byte(value) => *value != 0,
            Self::Int(value) => *value != 0,
            Self::Float(value) => *value != 0.0,
            Self::String(value) | Self::Name(value) => !value.is_empty(),
            Self::Vector(value) => *value != Vec3::ZERO,
            Self::Rotator(value) => value.iter().any(|component| *component != 0),
            Self::Struct(value) => value.fields.values().any(Value::as_bool),
            Self::Object(_) => true,
            Self::Array(value) => !value.is_empty(),
        }
    }

    pub fn as_int(&self) -> Option<i32> {
        match self {
            Self::Byte(value) => Some(i32::from(*value)),
            Self::Int(value) => Some(*value),
            Self::Float(value) => Some(*value as i32),
            Self::Bool(value) => Some(i32::from(*value)),
            Self::String(value) | Self::Name(value) => value.trim().parse().ok(),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f32> {
        match self {
            Self::Byte(value) => Some(f32::from(*value)),
            Self::Int(value) => Some(*value as f32),
            Self::Float(value) => Some(*value),
            Self::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
            Self::String(value) | Self::Name(value) => value.trim().parse().ok(),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) | Self::Name(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<u32> {
        match self {
            Self::Object(id) => Some(*id),
            _ => None,
        }
    }

    /// Read a member of a vector, rotator, or generic struct.
    pub fn struct_member(&self, name: &str) -> Value {
        match self {
            Self::Vector(value) => match name {
                "X" => Self::Float(value.x),
                "Y" => Self::Float(value.y),
                "Z" => Self::Float(value.z),
                _ => Self::None,
            },
            Self::Rotator(value) => match name {
                "Pitch" | "P" => Self::Int(value[0]),
                "Yaw" | "Y" => Self::Int(value[1]),
                "Roll" | "R" => Self::Int(value[2]),
                _ => Self::None,
            },
            Self::Struct(value) => value.fields.get(name).cloned().unwrap_or(Self::None),
            _ => Self::None,
        }
    }

    /// Write a member of a vector, rotator, or generic struct.
    ///
    /// Returns `false` when the value is not a struct-like type.
    pub fn set_struct_member(&mut self, name: &str, value: Value) -> bool {
        match self {
            Self::Vector(vector) => match name {
                "X" => {
                    vector.x = value.as_float().unwrap_or(0.0);
                    true
                }
                "Y" => {
                    vector.y = value.as_float().unwrap_or(0.0);
                    true
                }
                "Z" => {
                    vector.z = value.as_float().unwrap_or(0.0);
                    true
                }
                _ => false,
            },
            Self::Rotator(rotator) => match name {
                "Pitch" | "P" => {
                    rotator[0] = value.as_int().unwrap_or(0);
                    true
                }
                "Yaw" | "Y" => {
                    rotator[1] = value.as_int().unwrap_or(0);
                    true
                }
                "Roll" | "R" => {
                    rotator[2] = value.as_int().unwrap_or(0);
                    true
                }
                _ => false,
            },
            Self::Struct(structure) => {
                structure.fields.insert(name.to_string(), value);
                true
            }
            _ => false,
        }
    }

    /// Structural equality for vectors, rotators, and generic structs.
    pub fn struct_equal(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Vector(left), Self::Vector(right)) => left == right,
            (Self::Rotator(left), Self::Rotator(right)) => left == right,
            (Self::Struct(left), Self::Struct(right)) => left.fields == right.fields,
            _ => self == other,
        }
    }

    /// Convert a value to its UnrealScript string representation.
    pub fn to_display_string(&self) -> String {
        match self {
            Self::None => String::new(),
            Self::Bool(true) => "True".to_string(),
            Self::Bool(false) => "False".to_string(),
            Self::Byte(value) => value.to_string(),
            Self::Int(value) => value.to_string(),
            Self::Float(value) => format_float(*value),
            Self::String(value) | Self::Name(value) => value.clone(),
            Self::Vector(value) => {
                format!(
                    "X={} Y={} Z={}",
                    format_float(value.x),
                    format_float(value.y),
                    format_float(value.z)
                )
            }
            Self::Rotator(value) => format!(
                "P={} Y={} R={}",
                value[0], value[1], value[2]
            ),
            Self::Struct(value) => {
                let mut fields: Vec<(&String, &Value)> = value.fields.iter().collect();
                fields.sort_by(|left, right| left.0.cmp(right.0));
                let items: Vec<String> = fields
                    .into_iter()
                    .map(|(name, value)| format!("{name}={}", value.to_display_string()))
                    .collect();
                format!("({})", items.join(","))
            }
            Self::Object(id) => format!("Object#{id}"),
            Self::Array(values) => {
                let items: Vec<String> = values
                    .iter()
                    .map(Value::to_display_string)
                    .collect();
                format!("({})", items.join(","))
            }
        }
    }

    /// Default value for a `UProperty` class (`IntProperty`, `StrProperty`, ...).
    pub fn default_for_property(class_name: &str) -> Self {
        match class_name {
            "ByteProperty" => Self::Byte(0),
            "IntProperty" => Self::Int(0),
            "FloatProperty" => Self::Float(0.0),
            "BoolProperty" => Self::Bool(false),
            "StrProperty" => Self::String(String::new()),
            "NameProperty" => Self::Name(String::new()),
            "VectorProperty" => Self::Vector(Vec3::ZERO),
            "RotatorProperty" => Self::Rotator([0, 0, 0]),
            "ArrayProperty" | "FixedArrayProperty" => Self::Array(Vec::new()),
            _ => Self::None,
        }
    }

    /// Apply a UnrealScript conversion opcode (`EX_ByteToInt`, ...).
    ///
    /// Opcodes cover `0x39..=0x59`; see OpenDE `script_model.h`. Returns
    /// `None` for opcodes that are not conversions.
    pub fn convert(self, opcode: u8) -> Option<Self> {
        use Value::*;

        let converted = match opcode {
            // RotatorToVector
            0x39 => Vector(match self {
                Rotator(value) => rotator_to_vector(value),
                Vector(value) => value,
                _ => Vec3::ZERO,
            }),
            // ByteToInt
            0x3a => Int(self.as_int().unwrap_or(0)),
            // ByteToBool
            0x3b => Bool(self.as_int().unwrap_or(0) != 0),
            // ByteToFloat
            0x3c => Float(self.as_int().unwrap_or(0) as f32),
            // IntToByte
            0x3d => Byte(self.as_int().unwrap_or(0) as u8),
            // IntToBool
            0x3e => Bool(self.as_int().unwrap_or(0) != 0),
            // IntToFloat
            0x3f => Float(self.as_int().unwrap_or(0) as f32),
            // BoolToByte
            0x40 => Byte(u8::from(self.as_bool())),
            // BoolToInt
            0x41 => Int(i32::from(self.as_bool())),
            // BoolToFloat
            0x42 => Float(if self.as_bool() { 1.0 } else { 0.0 }),
            // FloatToByte
            0x43 => Byte(self.as_float().unwrap_or(0.0) as u8),
            // FloatToInt
            0x44 => Int(self.as_float().unwrap_or(0.0) as i32),
            // FloatToBool
            0x45 => Bool(self.as_float().unwrap_or(0.0) != 0.0),
            // ObjectToBool
            0x47 => Bool(!matches!(self, None)),
            // NameToBool
            0x48 => Bool(!self.as_str().unwrap_or("").is_empty()),
            // StringToByte
            0x49 => Byte(
                self.as_str()
                    .and_then(|value| value.trim().parse().ok())
                    .unwrap_or(0),
            ),
            // StringToInt
            0x4a => Int(
                self.as_str()
                    .and_then(|value| value.trim().parse().ok())
                    .unwrap_or(0),
            ),
            // StringToBool
            0x4b => Bool(parse_bool(self.as_str().unwrap_or(""))),
            // StringToFloat
            0x4c => Float(
                self.as_str()
                    .and_then(|value| value.trim().parse().ok())
                    .unwrap_or(0.0),
            ),
            // StringToVector
            0x4d => Vector(parse_vector(self.as_str().unwrap_or(""))),
            // StringToRotator
            0x4e => Rotator(parse_rotator(self.as_str().unwrap_or(""))),
            // VectorToBool
            0x4f => Bool(match self {
                Vector(value) => value != Vec3::ZERO,
                _ => false,
            }),
            // VectorToRotator
            0x50 => Rotator(match self {
                Vector(value) => vector_to_rotator(value),
                Rotator(value) => value,
                _ => [0, 0, 0],
            }),
            // RotatorToBool
            0x51 => Bool(match self {
                Rotator(value) => value.iter().any(|component| *component != 0),
                _ => false,
            }),
            // ByteToString, IntToString, BoolToString, FloatToString,
            // ObjectToString, NameToString, VectorToString, RotatorToString
            0x52..=0x59 => String(self.to_display_string()),
            _ => return Option::None,
        };
        Some(converted)
    }
}

fn format_float(value: f32) -> String {
    if value.is_nan() {
        "NaN".to_string()
    } else if value.is_infinite() {
        if value.is_sign_negative() {
            "-Infinity".to_string()
        } else {
            "Infinity".to_string()
        }
    } else {
        format!("{value:.6}")
    }
}

fn parse_bool(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() {
        return false;
    }
    if text.eq_ignore_ascii_case("true") {
        return true;
    }
    if text.eq_ignore_ascii_case("false") {
        return false;
    }
    text.parse::<f32>().map(|value| value != 0.0).unwrap_or(true)
}

fn extract_floats(text: &str) -> Vec<f32> {
    let mut values = Vec::new();
    let mut current = String::new();
    for character in text.chars() {
        if character.is_ascii_digit() || matches!(character, '-' | '+' | '.' | 'e' | 'E') {
            current.push(character);
        } else if !current.is_empty() {
            push_number(&mut values, &current);
            current.clear();
        }
    }
    if !current.is_empty() {
        push_number(&mut values, &current);
    }
    values
}

fn push_number(values: &mut Vec<f32>, text: &str) {
    // Guard against malformed pieces such as `e` or `-`.
    if let Ok(value) = text.parse::<f32>() {
        values.push(value);
    }
}

fn parse_vector(text: &str) -> Vec3 {
    let values = extract_floats(text);
    Vec3::new(
        values.first().copied().unwrap_or(0.0),
        values.get(1).copied().unwrap_or(0.0),
        values.get(2).copied().unwrap_or(0.0),
    )
}

fn parse_rotator(text: &str) -> [i32; 3] {
    let values = extract_floats(text);
    [
        values.first().copied().unwrap_or(0.0) as i32,
        values.get(1).copied().unwrap_or(0.0) as i32,
        values.get(2).copied().unwrap_or(0.0) as i32,
    ]
}

/// Convert a UE1 rotator (65536 units per turn) to a direction vector.
pub fn rotator_to_vector(rotator: [i32; 3]) -> Vec3 {
    let pitch = rotator[0] as f32 * TAU / 65536.0;
    let yaw = rotator[1] as f32 * TAU / 65536.0;
    Vec3::new(
        yaw.cos() * pitch.cos(),
        yaw.sin() * pitch.cos(),
        pitch.sin(),
    )
}

/// Convert a direction vector to a UE1 rotator.
pub fn vector_to_rotator(vector: Vec3) -> [i32; 3] {
    if vector.length_squared() < 1.0e-12 {
        return [0, 0, 0];
    }
    let yaw = vector.y.atan2(vector.x);
    let pitch = vector.z.atan2((vector.x * vector.x + vector.y * vector.y).sqrt());
    [
        (pitch * 65536.0 / TAU) as i32,
        (yaw * 65536.0 / TAU) as i32,
        0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truthiness() {
        assert!(!Value::None.as_bool());
        assert!(Value::Int(1).as_bool());
        assert!(!Value::String(String::new()).as_bool());
        assert!(Value::String("x".into()).as_bool());
        assert!(Value::Object(1).as_bool());
    }

    #[test]
    fn conversions_roundtrip_common_types() {
        assert_eq!(
            Value::String("42".into()).convert(0x4a),
            Some(Value::Int(42))
        );
        assert_eq!(Value::Int(42).convert(0x53), Some(Value::String("42".into())));
        assert_eq!(Value::Int(0).convert(0x41), Some(Value::Int(0)));
        assert_eq!(Value::Int(3).convert(0x3e), Some(Value::Bool(true)));
    }

    #[test]
    fn parses_vectors() {
        assert_eq!(
            parse_vector("X=1.5 Y=2.5 Z=-3.5"),
            Vec3::new(1.5, 2.5, -3.5)
        );
        assert_eq!(parse_vector("1,2,3"), Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn defaults_match_property_classes() {
        assert_eq!(Value::default_for_property("IntProperty"), Value::Int(0));
        assert_eq!(
            Value::default_for_property("StrProperty"),
            Value::String(String::new())
        );
        assert_eq!(
            Value::default_for_property("ObjectProperty"),
            Value::None
        );
    }

    #[test]
    fn vector_members_are_readable_and_writable() {
        let mut value = Value::Vector(Vec3::ZERO);
        assert!(value.set_struct_member("X", Value::Float(2.0)));
        assert!(value.set_struct_member("Y", Value::Float(3.0)));
        assert!(value.set_struct_member("Z", Value::Int(4)));
        assert_eq!(value.struct_member("X"), Value::Float(2.0));
        assert_eq!(value.struct_member("Z"), Value::Float(4.0));
        assert!(value.struct_equal(&Value::Vector(Vec3::new(2.0, 3.0, 4.0))));
    }

    #[test]
    fn generic_struct_members_roundtrip() {
        let mut value = Value::Struct(StructValue::default());
        assert!(value.set_struct_member("Foo", Value::Int(7)));
        assert_eq!(value.struct_member("Foo"), Value::Int(7));
        assert_eq!(value.struct_member("Missing"), Value::None);
    }
}
