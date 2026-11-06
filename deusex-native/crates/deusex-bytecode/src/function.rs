//! Parsed UnrealScript function representation.

use std::fmt;

use crate::instructions::Token;

/// Function flags from UE1 `EFunctionFlags`.
pub mod function_flags {
    pub const FINAL: u32 = 0x0000_0001;
    pub const DEFINED: u32 = 0x0000_0002;
    pub const ITERATOR: u32 = 0x0000_0004;
    pub const LATENT: u32 = 0x0000_0008;
    pub const PREOPERATOR: u32 = 0x0000_0010;
    pub const SINGULAR: u32 = 0x0000_0020;
    pub const NET: u32 = 0x0000_0040;
    pub const NET_RELIABLE: u32 = 0x0000_0080;
    pub const SIMULATED: u32 = 0x0000_0100;
    pub const EXEC: u32 = 0x0000_0200;
    pub const INTRINSIC: u32 = 0x0000_0400;
    pub const EVENT: u32 = 0x0000_0800;
    pub const OPERATOR: u32 = 0x0000_1000;
    pub const STATIC: u32 = 0x0000_2000;
}

/// One decoded UnrealScript expression or statement.
///
/// `offset` and `size` refer to the *normalized* script stream, where object
/// and name references occupy four bytes. This is the coordinate system used
/// by jump/case offsets in Deus Ex packages.
#[derive(Debug, Clone)]
pub struct Expr {
    pub offset: u32,
    pub size: u32,
    pub raw_offset: usize,
    pub raw_size: usize,
    pub kind: ExprKind,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    LocalVariable(i32),
    InstanceVariable(i32),
    DefaultVariable(i32),

    Return(Box<Expr>),
    Switch(u8, Box<Expr>),
    Jump(u16),
    JumpIfNot { offset: u16, condition: Box<Expr> },
    Stop,
    Assert { offset: u16, condition: Box<Expr> },
    Case { offset: u16, value: Option<Box<Expr>> },
    Nothing,
    LabelTable(Vec<(i32, u32)>),
    GotoLabel(Box<Expr>),
    EatString(Box<Expr>),
    Let { target: Box<Expr>, value: Box<Expr> },
    DynArrayElement { array: Box<Expr>, index: Box<Expr> },
    New(Vec<Expr>),
    ClassContext {
        context: Box<Expr>,
        offset: u16,
        byte: u8,
        value: Box<Expr>,
    },
    MetaCast { class: i32, value: Box<Expr> },
    LetBool { target: Box<Expr>, value: Box<Expr> },
    Unknown15,
    EndFunctionParms,
    SelfObject,
    Skip { offset: u16, value: Box<Expr> },
    Context {
        context: Box<Expr>,
        offset: u16,
        byte: u8,
        value: Box<Expr>,
    },
    ArrayElement { array: Box<Expr>, index: Box<Expr> },
    VirtualFunction { name: i32, params: Vec<Expr> },
    FinalFunction { function: i32, params: Vec<Expr> },
    IntConst(i32),
    FloatConst(f32),
    StringConst(String),
    ObjectConst(i32),
    NameConst(i32),
    RotationConst([f32; 3]),
    VectorConst([f32; 3]),
    ByteConst(u8),
    IntZero,
    IntOne,
    True,
    False,
    NativeParm(i32),
    NoObject,
    Unknown2B { byte: u8, value: Box<Expr> },
    IntConstByte(u8),
    BoolVariable(Box<Expr>),
    DynamicCast { class: i32, value: Box<Expr> },
    Iterator { value: Box<Expr>, offset: u16 },
    IteratorPop,
    IteratorNext,
    StructCmpEq { struct_ref: i32, left: Box<Expr>, right: Box<Expr> },
    StructCmpNe { struct_ref: i32, left: Box<Expr>, right: Box<Expr> },
    UnicodeStringConst(String),
    StructMember { struct_ref: i32, value: Box<Expr> },
    Conversion { opcode: u8, value: Box<Expr> },
    GlobalFunction { name: i32, params: Vec<Expr> },
    /// Extended native call. The high nibble of the opcode token stores the
    /// high byte of the native index, so indices above 255 are representable.
    ExtendedNative { index: u16, params: Vec<Expr> },
    Native { index: u8, params: Vec<Expr> },
}

impl Expr {
    pub fn token(&self) -> Option<Token> {
        // This is mostly for diagnostics; the kind is the authoritative form.
        let value = match &self.kind {
            ExprKind::LocalVariable(_) => Token::LOCAL_VARIABLE,
            ExprKind::InstanceVariable(_) => Token::INSTANCE_VARIABLE,
            ExprKind::DefaultVariable(_) => Token::DEFAULT_VARIABLE,
            ExprKind::Return(_) => Token::RETURN,
            ExprKind::Switch(..) => Token::SWITCH,
            ExprKind::Jump(_) => Token::JUMP,
            ExprKind::JumpIfNot { .. } => Token::JUMP_IF_NOT,
            ExprKind::Stop => Token::STOP,
            ExprKind::Assert { .. } => Token::ASSERT,
            ExprKind::Case { .. } => Token::CASE,
            ExprKind::Nothing => Token::NOTHING,
            ExprKind::LabelTable(_) => Token::LABEL_TABLE,
            ExprKind::GotoLabel(_) => Token::GOTO_LABEL,
            ExprKind::EatString(_) => Token::EAT_STRING,
            ExprKind::Let { .. } => Token::LET,
            ExprKind::DynArrayElement { .. } => Token::DYN_ARRAY_ELEMENT,
            ExprKind::New(_) => Token::NEW,
            ExprKind::ClassContext { .. } => Token::CLASS_CONTEXT,
            ExprKind::MetaCast { .. } => Token::META_CAST,
            ExprKind::LetBool { .. } => Token::LET_BOOL,
            ExprKind::Unknown15 => Token::UNKNOWN_15,
            ExprKind::EndFunctionParms => Token::END_FUNCTION_PARMS,
            ExprKind::SelfObject => Token::SELF_,
            ExprKind::Skip { .. } => Token::SKIP,
            ExprKind::Context { .. } => Token::CONTEXT,
            ExprKind::ArrayElement { .. } => Token::ARRAY_ELEMENT,
            ExprKind::VirtualFunction { .. } => Token::VIRTUAL_FUNCTION,
            ExprKind::FinalFunction { .. } => Token::FINAL_FUNCTION,
            ExprKind::IntConst(_) => Token::INT_CONST,
            ExprKind::FloatConst(_) => Token::FLOAT_CONST,
            ExprKind::StringConst(_) => Token::STRING_CONST,
            ExprKind::ObjectConst(_) => Token::OBJECT_CONST,
            ExprKind::NameConst(_) => Token::NAME_CONST,
            ExprKind::RotationConst(_) => Token::ROTATION_CONST,
            ExprKind::VectorConst(_) => Token::VECTOR_CONST,
            ExprKind::ByteConst(_) => Token::BYTE_CONST,
            ExprKind::IntZero => Token::INT_ZERO,
            ExprKind::IntOne => Token::INT_ONE,
            ExprKind::True => Token::TRUE,
            ExprKind::False => Token::FALSE,
            ExprKind::NativeParm(_) => Token::NATIVE_PARM,
            ExprKind::NoObject => Token::NO_OBJECT,
            ExprKind::Unknown2B { .. } => Token::UNKNOWN_2B,
            ExprKind::IntConstByte(_) => Token::INT_CONST_BYTE,
            ExprKind::BoolVariable(_) => Token::BOOL_VARIABLE,
            ExprKind::DynamicCast { .. } => Token::DYNAMIC_CAST,
            ExprKind::Iterator { .. } => Token::ITERATOR,
            ExprKind::IteratorPop => Token::ITERATOR_POP,
            ExprKind::IteratorNext => Token::ITERATOR_NEXT,
            ExprKind::StructCmpEq { .. } => Token::STRUCT_CMP_EQ,
            ExprKind::StructCmpNe { .. } => Token::STRUCT_CMP_NE,
            ExprKind::UnicodeStringConst(_) => Token::UNICODE_STRING_CONST,
            ExprKind::StructMember { .. } => Token::STRUCT_MEMBER,
            ExprKind::Conversion { opcode, .. } => Token::new(*opcode),
            ExprKind::GlobalFunction { .. } => Token::GLOBAL_FUNCTION,
            ExprKind::ExtendedNative { .. } => Token::EXTENDED_NATIVE,
            ExprKind::Native { .. } => Token::FIRST_NATIVE,
        };
        Some(value)
    }

    pub fn children(&self) -> Vec<&Expr> {
        let mut children = Vec::new();
        match &self.kind {
            ExprKind::Return(e)
            | ExprKind::GotoLabel(e)
            | ExprKind::EatString(e)
            | ExprKind::MetaCast { value: e, .. }
            | ExprKind::Skip { value: e, .. }
            | ExprKind::BoolVariable(e)
            | ExprKind::DynamicCast { value: e, .. }
            | ExprKind::StructMember { value: e, .. }
            | ExprKind::Conversion { value: e, .. } => children.push(e.as_ref()),
            ExprKind::Switch(_, e)
            | ExprKind::JumpIfNot { condition: e, .. }
            | ExprKind::Assert { condition: e, .. } => children.push(e.as_ref()),
            ExprKind::Case { value: Some(e), .. } => children.push(e.as_ref()),
            ExprKind::Let { target, value }
            | ExprKind::LetBool { target, value }
            | ExprKind::DynArrayElement { array: target, index: value }
            | ExprKind::ArrayElement { array: target, index: value } => {
                children.push(target.as_ref());
                children.push(value.as_ref());
            }
            ExprKind::New(items) | ExprKind::VirtualFunction { params: items, .. }
            | ExprKind::FinalFunction { params: items, .. }
            | ExprKind::GlobalFunction { params: items, .. }
            | ExprKind::ExtendedNative { params: items, .. }
            | ExprKind::Native { params: items, .. } => children.extend(items.iter()),
            ExprKind::ClassContext { context, value, .. }
            | ExprKind::Context { context, value, .. } => {
                children.push(context.as_ref());
                children.push(value.as_ref());
            }
            ExprKind::Iterator { value, .. } => children.push(value.as_ref()),
            ExprKind::StructCmpEq { left, right, .. }
            | ExprKind::StructCmpNe { left, right, .. } => {
                children.push(left.as_ref());
                children.push(right.as_ref());
            }
            ExprKind::Unknown2B { value, .. } => children.push(value.as_ref()),
            _ => {}
        }
        children
    }
}

#[derive(Debug, Clone)]
pub struct Function {
    pub export_ref: i32,
    pub name: String,
    pub outer_ref: i32,
    pub outer_name: String,
    pub super_ref: i32,
    pub next_ref: i32,
    pub script_text_ref: i32,
    pub children_ref: i32,
    pub friendly_name: String,
    pub line: u32,
    pub text_pos: u32,
    /// Script size in normalized bytes (object/name refs count as 4).
    pub script_size: u32,
    /// Raw byte offset of the script inside the package.
    pub script_offset: usize,
    /// Raw number of bytes consumed by the compact bytecode.
    pub script_raw_size: usize,
    pub script: Vec<u8>,
    pub body: Vec<Expr>,
    pub native_index: i16,
    pub precedence: u8,
    pub function_flags: u32,
    pub replication_offset: Option<i16>,
    /// End of the export's serialized data.
    pub export_end: usize,
    /// Reader position after parsing the function fields.
    pub reader_end: usize,
}

impl Function {
    pub fn is_native(&self) -> bool {
        (self.function_flags & function_flags::INTRINSIC) != 0 || self.native_index != 0
    }

    pub fn is_net(&self) -> bool {
        (self.function_flags & function_flags::NET) != 0
    }

    pub fn display_name(&self) -> String {
        if self.outer_name.is_empty() {
            self.name.clone()
        } else {
            format!("{}.{}", self.outer_name, self.name)
        }
    }
}

impl fmt::Display for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} (native={}, precedence={}, flags=0x{:X}, script={} bytes/{})",
            self.display_name(),
            self.native_index,
            self.precedence,
            self.function_flags,
            self.script_raw_size,
            self.script_size
        )
    }
}
