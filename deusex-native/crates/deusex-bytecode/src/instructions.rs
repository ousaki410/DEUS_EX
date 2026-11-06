//! Deus Ex / UE1 UnrealScript bytecode tokens.
//!
//! The token set differs from later Unreal Engine versions. Values follow
//! OpenDE's `core/object.cc` token enum, which is based on Deus Ex GOTY.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Token(pub u8);

macro_rules! tokens {
    ($($name:ident = $value:expr),* $(,)?) => {
        impl Token {
            $(pub const $name: Token = Token($value);)*
        }
    };
}

tokens! {
    LOCAL_VARIABLE = 0x00,
    INSTANCE_VARIABLE = 0x01,
    DEFAULT_VARIABLE = 0x02,
    RETURN = 0x04,
    SWITCH = 0x05,
    JUMP = 0x06,
    JUMP_IF_NOT = 0x07,
    STOP = 0x08,
    ASSERT = 0x09,
    CASE = 0x0a,
    NOTHING = 0x0b,
    LABEL_TABLE = 0x0c,
    GOTO_LABEL = 0x0d,
    EAT_STRING = 0x0e,
    LET = 0x0f,
    DYN_ARRAY_ELEMENT = 0x10,
    NEW = 0x11,
    CLASS_CONTEXT = 0x12,
    META_CAST = 0x13,
    LET_BOOL = 0x14,
    UNKNOWN_15 = 0x15,
    END_FUNCTION_PARMS = 0x16,
    SELF_ = 0x17,
    SKIP = 0x18,
    CONTEXT = 0x19,
    ARRAY_ELEMENT = 0x1a,
    VIRTUAL_FUNCTION = 0x1b,
    FINAL_FUNCTION = 0x1c,
    INT_CONST = 0x1d,
    FLOAT_CONST = 0x1e,
    STRING_CONST = 0x1f,
    OBJECT_CONST = 0x20,
    NAME_CONST = 0x21,
    ROTATION_CONST = 0x22,
    VECTOR_CONST = 0x23,
    BYTE_CONST = 0x24,
    INT_ZERO = 0x25,
    INT_ONE = 0x26,
    TRUE = 0x27,
    FALSE = 0x28,
    NATIVE_PARM = 0x29,
    NO_OBJECT = 0x2a,
    UNKNOWN_2B = 0x2b,
    INT_CONST_BYTE = 0x2c,
    BOOL_VARIABLE = 0x2d,
    DYNAMIC_CAST = 0x2e,
    ITERATOR = 0x2f,
    ITERATOR_POP = 0x30,
    ITERATOR_NEXT = 0x31,
    STRUCT_CMP_EQ = 0x32,
    STRUCT_CMP_NE = 0x33,
    UNICODE_STRING_CONST = 0x34,
    STRUCT_MEMBER = 0x36,
    GLOBAL_FUNCTION = 0x38,
    MIN_CONVERSION = 0x39,
    MAX_CONVERSION = 0x5a,
    EXTENDED_NATIVE = 0x60,
    FIRST_NATIVE = 0x70,
}

impl Token {
    pub fn new(value: u8) -> Self {
        Self(value)
    }

    pub fn value(self) -> u8 {
        self.0
    }

    pub fn is_conversion(self) -> bool {
        (Self::MIN_CONVERSION.0..=Self::MAX_CONVERSION.0).contains(&self.0)
    }

    pub fn is_extended_native(self) -> bool {
        (Self::EXTENDED_NATIVE.0..Self::FIRST_NATIVE.0).contains(&self.0)
    }

    pub fn is_native(self) -> bool {
        self.0 >= Self::FIRST_NATIVE.0
    }

    pub fn name(self) -> &'static str {
        match self.0 {
            0x00 => "LocalVariable",
            0x01 => "InstanceVariable",
            0x02 => "DefaultVariable",
            0x03 => "Unknown03",
            0x04 => "Return",
            0x05 => "Switch",
            0x06 => "Jump",
            0x07 => "JumpIfNot",
            0x08 => "Stop",
            0x09 => "Assert",
            0x0a => "Case",
            0x0b => "Nothing",
            0x0c => "LabelTable",
            0x0d => "GotoLabel",
            0x0e => "EatString",
            0x0f => "Let",
            0x10 => "DynArrayElement",
            0x11 => "New",
            0x12 => "ClassContext",
            0x13 => "MetaCast",
            0x14 => "LetBool",
            0x15 => "Unknown15",
            0x16 => "EndFunctionParms",
            0x17 => "Self",
            0x18 => "Skip",
            0x19 => "Context",
            0x1a => "ArrayElement",
            0x1b => "VirtualFunction",
            0x1c => "FinalFunction",
            0x1d => "IntConst",
            0x1e => "FloatConst",
            0x1f => "StringConst",
            0x20 => "ObjectConst",
            0x21 => "NameConst",
            0x22 => "RotationConst",
            0x23 => "VectorConst",
            0x24 => "ByteConst",
            0x25 => "IntZero",
            0x26 => "IntOne",
            0x27 => "True",
            0x28 => "False",
            0x29 => "NativeParm",
            0x2a => "NoObject",
            0x2b => "Unknown2B",
            0x2c => "IntConstByte",
            0x2d => "BoolVariable",
            0x2e => "DynamicCast",
            0x2f => "Iterator",
            0x30 => "IteratorPop",
            0x31 => "IteratorNext",
            0x32 => "StructCmpEq",
            0x33 => "StructCmpNe",
            0x34 => "UnicodeStringConst",
            0x35 => "Unknown35",
            0x36 => "StructMember",
            0x37 => "Unknown37",
            0x38 => "GlobalFunction",
            0x39..=0x5a => "Conversion",
            0x5b..=0x5f => "UnknownExtended",
            0x60..=0x6f => "ExtendedNative",
            0x70..=0xff => "Native",
        }
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (0x{:02X})", self.name(), self.0)
    }
}
