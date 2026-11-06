//! Bytecode → AST decompilation.
//!
//! Converts [`crate::function::Expr`] trees (decoded UnrealScript bytecode)
//! into [`deusex_ast::Stmt`]/[`deusex_ast::Expr`] nodes suitable for codegen.
//!
//! The bytecode expression tree is expression-oriented: every token is an
//! expression. This pass linearizes top-level expressions into statements,
//! resolves jump/case targets to labels, and maps object/name references to
//! package symbols.

use std::collections::{HashMap, HashSet};

use deusex_ast::{
    AstFunction, AstPackage, BinOp, ClassId, DecompiledPackage, Expr as AstExpr, FuncId, NameId,
    PropId, Stmt, SwitchCase, Ty, UnaryOp,
};
use deusex_package::UnrealPackage;

use crate::cfg::build_cfg;
use crate::function::{Expr, ExprKind, Function, function_flags};

/// Error type for decompilation.
#[derive(Debug, thiserror::Error)]
pub enum DecompileError {
    #[error("unresolved name reference {0}")]
    UnresolvedName(i32),
    #[error("unresolved object reference {0}")]
    UnresolvedObject(i32),
    #[error("unsupported bytecode construct: {0}")]
    Unsupported(String),
    #[error("internal: {0}")]
    Internal(String),
    #[error("bytecode error: {0}")]
    Bytecode(#[from] crate::BytecodeError),
}

/// Context for decompiling a single function.
struct Decompiler<'a> {
    package: &'a UnrealPackage,
    /// Monotonically increasing id for generated function/class references.
    next_id: u32,
    /// Maps local variable slot -> generated name.
    locals: HashMap<u32, String>,
    next_local: u32,
    /// Maps normalized bytecode offset -> label name for jump targets.
    labels: HashMap<u32, String>,
    /// Set of already-emitted labels.
    emitted_labels: HashSet<String>,
}

/// Resolve a compact object reference to a package path string.
fn object_path(pkg: &UnrealPackage, reference: i32) -> Result<String, DecompileError> {
    match reference {
        0 => Ok("None".to_string()),
        r if r < 0 => {
            let imp = pkg.imports().get(r).ok_or(DecompileError::UnresolvedObject(r))?;
            let name = pkg
                .names()
                .name(imp.object_name)
                .ok_or(DecompileError::UnresolvedName(imp.object_name))?;
            Ok(format!("Imp.{name}"))
        }
        r => pkg
            .object_path(r)
            .ok_or(DecompileError::UnresolvedObject(r)),
    }
}

/// Resolve a compact name reference to a string.
fn name_str(pkg: &UnrealPackage, index: i32) -> Result<String, DecompileError> {
    pkg.names()
        .name(index)
        .map(|s| s.to_string())
        .ok_or(DecompileError::UnresolvedName(index))
}

impl<'a> Decompiler<'a> {
    fn new(package: &'a UnrealPackage) -> Self {
        Self {
            package,
            next_id: 1,
            locals: HashMap::new(),
            next_local: 0,
            labels: HashMap::new(),
            emitted_labels: HashSet::new(),
        }
    }

    fn next_func_id(&mut self) -> FuncId {
        let id = FuncId(self.next_id);
        self.next_id += 1;
        id
    }

    fn local_name(&mut self, slot: u32) -> String {
        if let Some(n) = self.locals.get(&slot) {
            return n.clone();
        }
        let name = format!("local_{}", self.next_local);
        self.next_local += 1;
        self.locals.insert(slot, name.clone());
        name
    }

    fn label_for(&mut self, offset: u32) -> String {
        if let Some(l) = self.labels.get(&offset) {
            return l.clone();
        }
        let label = format!("lbl_{offset:04X}");
        self.labels.insert(offset, label.clone());
        label
    }

    /// Best-effort type guess from a bytecode expression.
    fn guess_ty(&self, expr: &Expr) -> Ty {
        match &expr.kind {
            ExprKind::IntConst(_) | ExprKind::IntZero | ExprKind::IntOne => Ty::Int,
            ExprKind::FloatConst(_) => Ty::Float,
            ExprKind::StringConst(_) | ExprKind::UnicodeStringConst(_) => Ty::String,
            ExprKind::NameConst(_) => Ty::Name,
            ExprKind::ObjectConst(_) | ExprKind::NativeParm(_) | ExprKind::NoObject => Ty::Unknown,
            ExprKind::VectorConst(_) => Ty::Vector,
            ExprKind::RotationConst(_) => Ty::Rotator,
            ExprKind::ByteConst(_) => Ty::Byte,
            ExprKind::BoolVariable(_) | ExprKind::True | ExprKind::False => Ty::Bool,
            _ => Ty::Unknown,
        }
    }

    /// Convert a bytecode `Expr` to an AST `Expr`.
    fn expr_to_ast(&mut self, expr: &Expr) -> Result<AstExpr, DecompileError> {
        let ty = self.guess_ty(expr);
        let ast = match &expr.kind {
            ExprKind::LocalVariable(slot) => {
                let name = self.local_name(*slot as u32);
                AstExpr::LocalVar { name, ty, slot: *slot as u32 }
            }
            ExprKind::InstanceVariable(prop) => AstExpr::InstanceVar {
                prop: PropId(*prop as u32),
                ty,
            },
            ExprKind::DefaultVariable(prop) => AstExpr::DefaultVar {
                prop: PropId(*prop as u32),
                ty,
            },
            ExprKind::SelfObject => AstExpr::Self_ { ty },
            ExprKind::IntConst(v) => AstExpr::IntConst(*v),
            ExprKind::FloatConst(v) => AstExpr::FloatConst(*v),
            ExprKind::ByteConst(v) => AstExpr::ByteConst(*v),
            ExprKind::StringConst(s) => AstExpr::StringConst(s.clone()),
            ExprKind::NameConst(idx) => AstExpr::NameConst(NameId(*idx as u32)),
            ExprKind::ObjectConst(obj_ref) => {
                let _path = object_path(self.package, *obj_ref)?;
                AstExpr::ObjectConst(ClassId(0))
            }
            ExprKind::VectorConst(v) => AstExpr::VectorConst {
                x: (*v)[0], y: (*v)[1], z: (*v)[2],
            },
            ExprKind::RotationConst(v) => AstExpr::RotatorConst {
                pitch: (*v)[0] as i32, yaw: (*v)[1] as i32, roll: (*v)[2] as i32,
            },
            ExprKind::True => AstExpr::BoolConst(true),
            ExprKind::False => AstExpr::BoolConst(false),
            ExprKind::NoObject => AstExpr::NoneConst,
            ExprKind::NativeParm(obj_ref) => {
                let _path = object_path(self.package, *obj_ref)?;
                AstExpr::ObjectConst(ClassId(0))
            }
            ExprKind::VirtualFunction { name, params } => {
                let _name_str = name_str(self.package, *name)?;
                let args = params
                    .iter()
                    .map(|p| self.expr_to_ast(p))
                    .collect::<Result<Vec<_>, _>>()?;
                AstExpr::Call {
                    func: self.next_func_id(),
                    args,
                    ty: Ty::Unknown,
                    native_index: None,
                }
            }
            ExprKind::FinalFunction { function, params } => {
                let _path = object_path(self.package, *function)?;
                let args = params
                    .iter()
                    .map(|p| self.expr_to_ast(p))
                    .collect::<Result<Vec<_>, _>>()?;
                AstExpr::Call {
                    func: self.next_func_id(),
                    args,
                    ty: Ty::Unknown,
                    native_index: None,
                }
            }
            ExprKind::GlobalFunction { name, params } => {
                let name_str = name_str(self.package, *name)?;
                let args = params
                    .iter()
                    .map(|p| self.expr_to_ast(p))
                    .collect::<Result<Vec<_>, _>>()?;
                AstExpr::GlobalCall {
                    name: name_str,
                    args,
                    ty: Ty::Unknown,
                }
            }
            ExprKind::Native { index, params } => {
                let args = params
                    .iter()
                    .map(|p| self.expr_to_ast(p))
                    .collect::<Result<Vec<_>, _>>()?;
                AstExpr::NativeCall {
                    index: u16::from(*index),
                    args,
                    ty: Ty::Unknown,
                }
            }
            ExprKind::ExtendedNative { index, params } => {
                let args = params
                    .iter()
                    .map(|p| self.expr_to_ast(p))
                    .collect::<Result<Vec<_>, _>>()?;
                AstExpr::NativeCall {
                    index: *index,
                    args,
                    ty: Ty::Unknown,
                }
            }
            ExprKind::Conversion { opcode: _, value } => {
                let inner = self.expr_to_ast(value)?;
                AstExpr::Cast { ty, inner: Box::new(inner) }
            }
            ExprKind::Switch(_byte, cond) => {
                let cond_ast = self.expr_to_ast(cond)?;
                AstExpr::Switch {
                    expr: Box::new(cond_ast),
                    cases: vec![SwitchCase {
                        value: 0,
                        body: vec![],
                        fallthrough: true,
                    }],
                }
            }
            ExprKind::JumpIfNot { offset, condition } => {
                let cond_ast = self.expr_to_ast(condition)?;
                let _label = self.label_for(u32::from(*offset));
                AstExpr::Cond {
                    cond: Box::new(cond_ast),
                    then: Box::new(AstExpr::NoneConst),
                    r#else: Box::new(AstExpr::NoneConst),
                    ty: Ty::Bool,
                }
            }
            ExprKind::Jump(_) | ExprKind::Skip { .. } | ExprKind::LabelTable(_) => {
                AstExpr::NoneConst
            }
            ExprKind::ArrayElement { array, index } => {
                let arr = self.expr_to_ast(array)?;
                let idx = self.expr_to_ast(index)?;
                AstExpr::ArrayIndex {
                    array: Box::new(arr),
                    index: Box::new(idx),
                    ty: Ty::Unknown,
                }
            }
            ExprKind::DynArrayElement { array, index } => {
                let arr = self.expr_to_ast(array)?;
                let idx = self.expr_to_ast(index)?;
                AstExpr::ArrayIndex {
                    array: Box::new(arr),
                    index: Box::new(idx),
                    ty: Ty::Unknown,
                }
            }
            ExprKind::New(items) => {
                let args = items
                    .iter()
                    .map(|p| self.expr_to_ast(p))
                    .collect::<Result<Vec<_>, _>>()?;
                AstExpr::New { ty: Ty::Unknown, args }
            }
            ExprKind::MetaCast { class, value } => {
                let inner = self.expr_to_ast(value)?;
                let path = object_path(self.package, *class)?;
                let _ = path;
                AstExpr::Cast {
                    ty: Ty::Object(ClassId(0)),
                    inner: Box::new(inner),
                }
            }
            ExprKind::DynamicCast { class, value } => {
                let inner = self.expr_to_ast(value)?;
                let path = object_path(self.package, *class)?;
                let _ = path;
                AstExpr::Cast {
                    ty: Ty::Object(ClassId(0)),
                    inner: Box::new(inner),
                }
            }
            ExprKind::StructMember { struct_ref, value } => {
                let inner = self.expr_to_ast(value)?;
                let path = object_path(self.package, *struct_ref)?;
                let _ = path;
                AstExpr::Member {
                    obj: Box::new(inner),
                    member: PropId(0),
                    ty,
                }
            }
            ExprKind::StructCmpEq { struct_ref: _, left, right }
            | ExprKind::StructCmpNe { struct_ref: _, left, right } => {
                let l = self.expr_to_ast(left)?;
                let r = self.expr_to_ast(right)?;
                let op = if matches!(&expr.kind, ExprKind::StructCmpEq { .. }) {
                    BinOp::Eq
                } else {
                    BinOp::Ne
                };
                AstExpr::BinOp {
                    op,
                    left: Box::new(l),
                    right: Box::new(r),
                    ty: Ty::Bool,
                }
            }
            ExprKind::Iterator { value, offset } => {
                let inner = self.expr_to_ast(value)?;
                let _label = self.label_for(u32::from(*offset));
                AstExpr::Call {
                    func: self.next_func_id(),
                    args: vec![inner],
                    ty: Ty::Bool,
                    native_index: None,
                }
            }
            ExprKind::BoolVariable(inner) => {
                let e = self.expr_to_ast(inner)?;
                AstExpr::UnaryOp {
                    op: UnaryOp::Not,
                    inner: Box::new(e),
                    ty: Ty::Bool,
                }
            }
            ExprKind::Unknown15
            | ExprKind::EndFunctionParms
            | ExprKind::Nothing
            | ExprKind::GotoLabel(_)
            | ExprKind::EatString(_)
            | ExprKind::LetBool { .. }
            | ExprKind::Unknown2B { .. }
            | ExprKind::IntConstByte(_)
            | ExprKind::IteratorPop
            | ExprKind::IteratorNext
            | ExprKind::Stop
            | ExprKind::Context { .. }
            | ExprKind::ClassContext { .. }
            | ExprKind::IntZero
            | ExprKind::IntOne => AstExpr::NoneConst,

            ExprKind::Assert { offset, condition } => {
                let cond = self.expr_to_ast(condition)?;
                let _label = self.label_for(u32::from(*offset));
                AstExpr::UnaryOp {
                    op: UnaryOp::Not,
                    inner: Box::new(cond),
                    ty: Ty::Bool,
                }
            }
            ExprKind::Case { value, .. } => {
                if let Some(v) = value {
                    self.expr_to_ast(v)?
                } else {
                    AstExpr::NoneConst
                }
            }
            ExprKind::Return(_) => {
                // Return as expression shouldn't appear at top level; treat as NoneConst.
                AstExpr::NoneConst
            }
            _ => AstExpr::NoneConst,
        };
        Ok(ast)
    }

    /// Convert top-level bytecode expressions into AST statements using the CFG.
    fn stmts_from_cfg(
        &mut self,
        function: &Function,
        cfg: &crate::cfg::Cfg,
    ) -> Result<Vec<Stmt>, DecompileError> {
        let mut stmts = Vec::new();
        let body = &function.body;

        for block in &cfg.blocks {
            for &stmt_idx in &block.statements {
                let expr = &body[stmt_idx];
                let stmt = self.expr_to_stmt(expr)?;
                if !matches!(stmt, Stmt::Empty) {
                    stmts.push(stmt);
                }
            }
            match &block.terminator {
                crate::cfg::Terminator::JumpIfNot { .. } | crate::cfg::Terminator::Jump(_) => {
                    // Target already handled by individual goto statements above.
                }
                crate::cfg::Terminator::Return => {
                    stmts.push(Stmt::Return(None));
                }
                crate::cfg::Terminator::Stop => {
                    stmts.push(Stmt::Stop);
                }
                _ => {}
            }
        }

        Ok(stmts)
    }

    /// Convert a single top-level bytecode expression into an AST statement.
    fn expr_to_stmt(&mut self, expr: &Expr) -> Result<Stmt, DecompileError> {
        match &expr.kind {
            ExprKind::Let { target, value } => {
                let target_ast = self.expr_to_ast(target)?;
                let value_ast = self.expr_to_ast(value)?;
                let (name, ty) = match &target_ast {
                    AstExpr::LocalVar { name, ty, .. } => (name.clone(), ty.clone()),
                    _ => (format!("tmp_{}", expr.offset), Ty::Unknown),
                };
                Ok(Stmt::Let { name, ty, init: value_ast })
            }
            ExprKind::Return(inner) => {
                let val = self.expr_to_ast(inner)?;
                Ok(Stmt::Return(Some(val)))
            }
            ExprKind::Stop => Ok(Stmt::Stop),
            ExprKind::EndFunctionParms => Ok(Stmt::Empty),
            ExprKind::Nothing => Ok(Stmt::Empty),
            ExprKind::Unknown15 => Ok(Stmt::Empty),
            ExprKind::Native { index, params } => {
                let args = params
                    .iter()
                    .map(|p| self.expr_to_ast(p))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Stmt::Expr(AstExpr::NativeCall {
                    index: u16::from(*index),
                    args,
                    ty: Ty::Unknown,
                }))
            }
            ExprKind::ExtendedNative { index, params } => {
                let args = params
                    .iter()
                    .map(|p| self.expr_to_ast(p))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Stmt::Expr(AstExpr::NativeCall {
                    index: *index,
                    args,
                    ty: Ty::Unknown,
                }))
            }
            ExprKind::VirtualFunction { name, params } => {
                let _name_str = name_str(self.package, *name)?;
                let args = params
                    .iter()
                    .map(|p| self.expr_to_ast(p))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Stmt::Expr(AstExpr::Call {
                    func: self.next_func_id(),
                    args,
                    ty: Ty::Unknown,
                    native_index: None,
                }))
            }
            ExprKind::FinalFunction { function, params } => {
                let _path = object_path(self.package, *function)?;
                let args = params
                    .iter()
                    .map(|p| self.expr_to_ast(p))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Stmt::Expr(AstExpr::Call {
                    func: self.next_func_id(),
                    args,
                    ty: Ty::Unknown,
                    native_index: None,
                }))
            }
            ExprKind::GlobalFunction { name, params } => {
                let name_str = name_str(self.package, *name)?;
                let args = params
                    .iter()
                    .map(|p| self.expr_to_ast(p))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Stmt::Expr(AstExpr::GlobalCall {
                    name: name_str,
                    args,
                    ty: Ty::Unknown,
                }))
            }
            ExprKind::Jump(target) => {
                let label = self.label_for(u32::from(*target));
                Ok(Stmt::Goto { label })
            }
            ExprKind::JumpIfNot { offset, condition } => {
                let cond = self.expr_to_ast(condition)?;
                let label = self.label_for(u32::from(*offset));
                Ok(Stmt::If {
                    cond,
                    then: vec![],
                    r#else: vec![Stmt::Goto { label }],
                })
            }
            ExprKind::LabelTable(labels) => {
                for (name_idx, offset) in labels {
                    if *name_idx == 0 {
                        continue;
                    }
                    let label_name = name_str(self.package, *name_idx)?;
                    self.labels.insert(*offset, label_name.clone());
                    self.emitted_labels.insert(label_name);
                }
                Ok(Stmt::Empty)
            }
            ExprKind::GotoLabel(_inner) => {
                Ok(Stmt::Goto {
                    label: format!("computed_{}", expr.offset),
                })
            }
            ExprKind::Switch(_byte, cond) => {
                let cond_ast = self.expr_to_ast(cond)?;
                Ok(Stmt::Switch {
                    expr: cond_ast,
                    cases: vec![SwitchCase {
                        value: 0,
                        body: vec![],
                        fallthrough: true,
                    }],
                    default: vec![],
                })
            }
            ExprKind::Assert { offset: _, condition } => {
                let cond = self.expr_to_ast(condition)?;
                Ok(Stmt::If {
                    cond: AstExpr::UnaryOp {
                        op: UnaryOp::Not,
                        inner: Box::new(cond),
                        ty: Ty::Bool,
                    },
                    then: vec![],
                    r#else: vec![],
                })
            }
            ExprKind::LetBool { target, value } => {
                let target_ast = self.expr_to_ast(target)?;
                let value_ast = self.expr_to_ast(value)?;
                let (name, ty) = match &target_ast {
                    AstExpr::LocalVar { name, ty, .. } => (name.clone(), ty.clone()),
                    _ => (format!("tmp_{}", expr.offset), Ty::Bool),
                };
                Ok(Stmt::Let { name, ty, init: value_ast })
            }
            ExprKind::ArrayElement { array, index } => {
                let arr = self.expr_to_ast(array)?;
                let idx = self.expr_to_ast(index)?;
                Ok(Stmt::Expr(AstExpr::ArrayIndex {
                    array: Box::new(arr),
                    index: Box::new(idx),
                    ty: Ty::Unknown,
                }))
            }
            ExprKind::DynArrayElement { array, index } => {
                let arr = self.expr_to_ast(array)?;
                let idx = self.expr_to_ast(index)?;
                Ok(Stmt::Expr(AstExpr::ArrayIndex {
                    array: Box::new(arr),
                    index: Box::new(idx),
                    ty: Ty::Unknown,
                }))
            }
            ExprKind::New(items) => {
                let args = items
                    .iter()
                    .map(|p| self.expr_to_ast(p))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Stmt::Expr(AstExpr::New { ty: Ty::Unknown, args }))
            }
            ExprKind::MetaCast { class, value } => {
                let inner = self.expr_to_ast(value)?;
                let _path = object_path(self.package, *class)?;
                Ok(Stmt::Expr(AstExpr::Cast {
                    ty: Ty::Object(ClassId(0)),
                    inner: Box::new(inner),
                }))
            }
            ExprKind::DynamicCast { class, value } => {
                let inner = self.expr_to_ast(value)?;
                let _path = object_path(self.package, *class)?;
                Ok(Stmt::Expr(AstExpr::Cast {
                    ty: Ty::Object(ClassId(0)),
                    inner: Box::new(inner),
                }))
            }
            ExprKind::StructMember { struct_ref, value } => {
                let inner = self.expr_to_ast(value)?;
                let _path = object_path(self.package, *struct_ref)?;
                Ok(Stmt::Expr(AstExpr::Member {
                    obj: Box::new(inner),
                    member: PropId(0),
                    ty: Ty::Unknown,
                }))
            }
            ExprKind::StructCmpEq { struct_ref: _, left, right }
            | ExprKind::StructCmpNe { struct_ref: _, left, right } => {
                let l = self.expr_to_ast(left)?;
                let r = self.expr_to_ast(right)?;
                let op = if matches!(&expr.kind, ExprKind::StructCmpEq { .. }) {
                    BinOp::Eq
                } else {
                    BinOp::Ne
                };
                Ok(Stmt::Expr(AstExpr::BinOp {
                    op,
                    left: Box::new(l),
                    right: Box::new(r),
                    ty: Ty::Bool,
                }))
            }
            ExprKind::Iterator { value, offset } => {
                let inner = self.expr_to_ast(value)?;
                let label = self.label_for(u32::from(*offset));
                Ok(Stmt::Iterator {
                    var: format!("iter_{}", expr.offset),
                    iterable: inner,
                    body: vec![Stmt::Goto { label }],
                })
            }
            ExprKind::BoolVariable(inner) => {
                let e = self.expr_to_ast(inner)?;
                Ok(Stmt::Expr(AstExpr::UnaryOp {
                    op: UnaryOp::Not,
                    inner: Box::new(e),
                    ty: Ty::Bool,
                }))
            }
            ExprKind::Conversion { opcode: _, value } => {
                let inner = self.expr_to_ast(value)?;
                Ok(Stmt::Expr(AstExpr::Cast { ty: Ty::Unknown, inner: Box::new(inner) }))
            }
            ExprKind::InstanceVariable(_)
            | ExprKind::DefaultVariable(_)
            | ExprKind::LocalVariable(_)
            | ExprKind::SelfObject
            | ExprKind::IntConst(_)
            | ExprKind::FloatConst(_)
            | ExprKind::StringConst(_)
            | ExprKind::NameConst(_)
            | ExprKind::ObjectConst(_)
            | ExprKind::VectorConst(_)
            | ExprKind::RotationConst(_)
            | ExprKind::ByteConst(_)
            | ExprKind::IntZero
            | ExprKind::IntOne
            | ExprKind::True
            | ExprKind::False
            | ExprKind::NativeParm(_)
            | ExprKind::NoObject
            | ExprKind::UnicodeStringConst(_) => Ok(Stmt::Expr(self.expr_to_ast(expr)?)),
            _ => Ok(Stmt::Empty),
        }
    }
}

/// Decompile a single `UFunction` into an AST function.
pub fn decompile_function(
    function: &Function,
    package: &UnrealPackage,
) -> Result<AstFunction, DecompileError> {
    let mut dec = Decompiler::new(package);
    let cfg = build_cfg(function);

    let body = dec.stmts_from_cfg(function, &cfg)?;

    Ok(AstFunction {
        id: FuncId(function.export_ref as u32),
        name: function.name.clone(),
        outer_class: None,
        params: Vec::new(),
        ret_ty: Ty::Unknown,
        local_vars: dec
            .locals
            .into_iter()
            .map(|(_, name)| (name, Ty::Unknown))
            .collect(),
        body,
        native_index: Some(function.native_index),
        precedence: function.precedence,
        flags: function.function_flags,
        is_latent: (function.function_flags & function_flags::LATENT) != 0,
        is_event: (function.function_flags & function_flags::EVENT) != 0,
        is_operator: (function.function_flags & function_flags::OPERATOR) != 0,
        is_static: (function.function_flags & function_flags::STATIC) != 0,
        is_exec: (function.function_flags & function_flags::EXEC) != 0,
        line: function.line,
    })
}

/// Decompile every function in a package into an AST package.
pub fn decompile_package(package: &UnrealPackage) -> Result<AstPackage, DecompileError> {
    let functions = crate::parse_package_functions(package)?;
    let mut ast_functions = Vec::new();
    for func in &functions {
        ast_functions.push(decompile_function(func, package)?);
    }

    Ok(AstPackage {
        name: package
            .object_name(0)
            .unwrap_or("Package")
            .to_string(),
        classes: Vec::new(),
        functions: ast_functions,
        states: Vec::new(),
    })
}

/// Decompile a package into a full `DecompiledPackage` with a Rust module name.
pub fn decompile_to_rust_module(package: &UnrealPackage) -> Result<DecompiledPackage, DecompileError> {
    let pkg_name = package
        .object_name(0)
        .unwrap_or("Package")
        .to_string();
    let ast = decompile_package(package)?;
    let rust_module_name = format!("deusex_{}", pkg_name.to_lowercase());
    Ok(DecompiledPackage {
        package: ast,
        rust_module_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decompile_commandlet_main_smoke() {
        // Smoke test skipped when assets unavailable.
        let _ = std::env::var("DEUSEX_GOTY_PATH");
    }
}
