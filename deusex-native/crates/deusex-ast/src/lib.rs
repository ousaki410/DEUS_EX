//! deusex-ast - Static decompilation AST for UnrealScript.
//!
//! This crate defines a source-level AST that can be produced from decoded
//! UnrealScript bytecode (`deusex-bytecode::Expr`) and consumed by
//! `deusex-codegen` to emit Rust code. It intentionally keeps the AST simple
//! and close to UnrealScript semantics so codegen stays predictable.

use std::fmt;

/// A unique identifier for a package-local name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NameId(pub u32);

/// A unique identifier for a class within a package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClassId(pub u32);

/// A unique identifier for a function within a package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FuncId(pub u32);

/// A unique identifier for a property within a class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PropId(pub u32);

/// A unique identifier for a state within a class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StateId(pub u32);

/// A source-level type in UnrealScript.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ty {
    /// `none`
    None,
    /// `bool`
    Bool,
    /// `byte`
    Byte,
    /// `int`
    Int,
    /// `float`
    Float,
    /// `string` (Unicode string, `String` in Rust)
    String,
    /// `name` (package name index)
    Name,
    /// `vector`
    Vector,
    /// `rotator`
    Rotator,
    /// `color`
    Color,
    /// Reference to a class/object type.
    Object(ClassId),
    /// `class<SomeClass>`
    Class(ClassId),
    /// `array<T>`
    Array(Box<Ty>),
    /// Delegate type.
    Delegate,
    /// `struct` type (by name index or class id).
    Struct(NameId),
    /// `interface` type.
    Interface(ClassId),
    /// Unknown/unresolved type.
    Unknown,
}

impl Ty {
    pub fn is_scalar(&self) -> bool {
        matches!(
            self,
            Ty::None | Ty::Bool | Ty::Byte | Ty::Int | Ty::Float | Ty::Name
        )
    }

    pub fn is_object_ref(&self) -> bool {
        matches!(self, Ty::Object(_) | Ty::Class(_) | Ty::Interface(_))
    }

    pub fn default_rust_type(&self) -> &'static str {
        match self {
            Ty::None => "()",
            Ty::Bool => "bool",
            Ty::Byte => "u8",
            Ty::Int => "i32",
            Ty::Float => "f32",
            Ty::String => "String",
            Ty::Name => "Name",
            Ty::Vector => "glam::Vec3",
            Ty::Rotator => "glam::Vec3",
            Ty::Color => "glam::Vec4",
            Ty::Object(_) => "GcHandle<UObject>",
            Ty::Class(_) => "GcHandle<UClass>",
            Ty::Array(_) => "GcArray",
            Ty::Delegate => "Delegate",
            Ty::Struct(_) => "GcStruct",
            Ty::Interface(_) => "GcHandle<UInterface>",
            Ty::Unknown => "Unknown",
        }
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::None => write!(f, "none"),
            Ty::Bool => write!(f, "bool"),
            Ty::Byte => write!(f, "byte"),
            Ty::Int => write!(f, "int"),
            Ty::Float => write!(f, "float"),
            Ty::String => write!(f, "string"),
            Ty::Name => write!(f, "name"),
            Ty::Vector => write!(f, "vector"),
            Ty::Rotator => write!(f, "rotator"),
            Ty::Color => write!(f, "color"),
            Ty::Object(cid) => write!(f, "object<{cid:?}>"),
            Ty::Class(cid) => write!(f, "class<{cid:?}>"),
            Ty::Array(inner) => write!(f, "array<{inner}>"),
            Ty::Delegate => write!(f, "delegate"),
            Ty::Struct(nid) => write!(f, "struct<{nid:?}>"),
            Ty::Interface(cid) => write!(f, "interface<{cid:?}>"),
            Ty::Unknown => write!(f, "unknown"),
        }
    }
}

/// AST expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// A local variable reference.
    LocalVar { name: String, ty: Ty, slot: u32 },
    /// An instance variable reference.
    InstanceVar { prop: PropId, ty: Ty },
    /// A default variable reference.
    DefaultVar { prop: PropId, ty: Ty },
    /// `self`
    Self_ { ty: Ty },
    /// Integer literal.
    IntConst(i32),
    /// Float literal.
    FloatConst(f32),
    /// Byte literal.
    ByteConst(u8),
    /// String literal.
    StringConst(String),
    /// Name literal.
    NameConst(NameId),
    /// Object reference literal.
    ObjectConst(ClassId),
    /// Vector literal.
    VectorConst { x: f32, y: f32, z: f32 },
    /// Rotator literal.
    RotatorConst { pitch: i32, yaw: i32, roll: i32 },
    /// `None`
    NoneConst,
    /// `true` / `false`
    BoolConst(bool),
    /// Binary operator.
    BinOp {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
        ty: Ty,
    },
    /// Unary operator.
    UnaryOp { op: UnaryOp, inner: Box<Expr>, ty: Ty },
    /// Function call (virtual or final).
    Call {
        func: FuncId,
        args: Vec<Expr>,
        ty: Ty,
        native_index: Option<i16>,
    },
    /// Global function call.
    GlobalCall { name: String, args: Vec<Expr>, ty: Ty },
    /// Native operator call.
    NativeCall { index: u16, args: Vec<Expr>, ty: Ty },
    /// Cast / conversion.
    Cast { ty: Ty, inner: Box<Expr> },
    /// Member access (`obj.prop` or `struct.member`).
    Member { obj: Box<Expr>, member: PropId, ty: Ty },
    /// Array element access (`arr[i]`).
    ArrayIndex { array: Box<Expr>, index: Box<Expr>, ty: Ty },
    /// `new` expression.
    New { ty: Ty, args: Vec<Expr> },
    /// `switch` on expression value.
    Switch { expr: Box<Expr>, cases: Vec<SwitchCase> },
    /// Conditional `?:` (represented as If in statements).
    Cond { cond: Box<Expr>, then: Box<Expr>, r#else: Box<Expr>, ty: Ty },
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOp {
    Add, Sub, Mul, Div, Mod,
    Eq, Ne, Lt, Gt, Le, Ge,
    And, Or, Xor, Shl, Shr,
    Assign, AddAssign, SubAssign, MulAssign, DivAssign,
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    Neg, Not, BitNot, PreInc, PreDec, PostInc, PostDec,
}

/// A `switch` case.
#[derive(Debug, Clone, PartialEq)]
pub struct SwitchCase {
    pub value: i32,
    pub body: Vec<Stmt>,
    pub fallthrough: bool,
}

/// AST statement.
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// `let x = expr;`
    Let { name: String, ty: Ty, init: Expr },
    /// `x = expr;`
    Assign { target: Box<Expr>, value: Expr },
    /// `if cond { ... } else { ... }`
    If { cond: Expr, then: Vec<Stmt>, r#else: Vec<Stmt> },
    /// `goto label;`
    Goto { label: String },
    /// `return expr;`
    Return(Option<Expr>),
    /// `stop;`
    Stop,
    /// A labeled statement.
    Labeled { label: String, body: Box<Stmt> },
    /// A block of statements.
    Block(Vec<Stmt>),
    /// Expression statement.
    Expr(Expr),
    /// Latent function call (`Sleep`, `FinishAnim`, etc.).
    LatentCall { func: FuncId, args: Vec<Expr>, latent_action: String },
    /// `switch` statement.
    Switch { expr: Expr, cases: Vec<SwitchCase>, default: Vec<Stmt> },
    /// `foreach` / `iterator` loop.
    Iterator { var: String, iterable: Expr, body: Vec<Stmt> },
    /// Empty statement.
    Empty,
}

/// Function parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Ty,
    pub is_out: bool,
    pub is_optional: bool,
    pub default: Option<Expr>,
}

/// A decoded function in AST form.
#[derive(Debug, Clone, PartialEq)]
pub struct AstFunction {
    pub id: FuncId,
    pub name: String,
    pub outer_class: Option<ClassId>,
    pub params: Vec<Param>,
    pub ret_ty: Ty,
    pub local_vars: Vec<(String, Ty)>,
    pub body: Vec<Stmt>,
    pub native_index: Option<i16>,
    pub precedence: u8,
    pub flags: u32,
    pub is_latent: bool,
    pub is_event: bool,
    pub is_operator: bool,
    pub is_static: bool,
    pub is_exec: bool,
    pub line: u32,
}

/// A class property.
#[derive(Debug, Clone, PartialEq)]
pub struct AstProperty {
    pub id: PropId,
    pub name: String,
    pub ty: Ty,
    pub offset: u32,
    pub array_dim: u32,
    pub flags: u32,
    pub category: Option<String>,
    pub default: Option<Expr>,
}

/// A state declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct AstState {
    pub id: StateId,
    pub name: String,
    pub super_state: Option<StateId>,
    pub label_funcs: Vec<AstFunction>,
}

/// A class definition.
#[derive(Debug, Clone, PartialEq)]
pub struct AstClass {
    pub id: ClassId,
    pub name: String,
    pub super_class: Option<ClassId>,
    pub flags: u32,
    pub properties: Vec<AstProperty>,
    pub functions: Vec<AstFunction>,
    pub states: Vec<AstState>,
    pub defaults: Vec<(PropId, Expr)>,
}

/// A package (`.u` file).
#[derive(Debug, Clone, PartialEq)]
pub struct AstPackage {
    pub name: String,
    pub classes: Vec<AstClass>,
    pub functions: Vec<AstFunction>,
    pub states: Vec<AstState>,
}

/// Top-level decompilation output for one package.
#[derive(Debug, Clone, PartialEq)]
pub struct DecompiledPackage {
    pub package: AstPackage,
    pub rust_module_name: String,
}

impl AstFunction {
    pub fn display_name(&self) -> String {
        match self.outer_class {
            Some(cid) => format!("Class{cid:?}.{}", self.name),
            None => self.name.clone(),
        }
    }
}

impl fmt::Display for AstFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ty_default_rust_type() {
        assert_eq!(Ty::Int.default_rust_type(), "i32");
        assert_eq!(Ty::Float.default_rust_type(), "f32");
        assert_eq!(Ty::String.default_rust_type(), "String");
        assert_eq!(Ty::Bool.default_rust_type(), "bool");
        assert_eq!(Ty::None.default_rust_type(), "()");
    }

    #[test]
    fn ty_is_object_ref() {
        assert!(!Ty::Int.is_object_ref());
        assert!(Ty::Object(ClassId(1)).is_object_ref());
        assert!(Ty::Class(ClassId(2)).is_object_ref());
    }
}
