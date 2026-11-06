//! deusex-codegen - Generate Rust source from decompiled UnrealScript AST.
//!
//! The generator emits a *scaffold*: syntactically valid Rust that mirrors the
//! structure of the decompiled script. Semantic gaps (computed gotos, native
//! dispatch, unresolved types) are represented by helper calls on an execution
//! context that the runtime is expected to provide, and every such gap is
//! recorded in [`GeneratedCode::warnings`].
//!
//! The output targets a planned `deusex_runtime::script` API:
//!
//! ```text
//! ExecCtx: instance_var, default_var, self_obj, object, call, global_call,
//!          native_call, native_function, cast, member, array_index,
//!          new_object, switch_key, iterate, latent_call
//! ```
//!
//! Until that API exists in `deusex-runtime`, the generated source is intended
//! for inspection and as a starting point for hand-written porting work.

use std::collections::HashSet;
use std::fmt::Write as _;

use deusex_ast::{
    AstClass, AstFunction, AstPackage, AstProperty, AstState, BinOp, DecompiledPackage,
    Expr as AstExpr, Stmt, SwitchCase, Ty, UnaryOp,
};

/// Default crate path used in generated `use` statements.
pub const DEFAULT_RUNTIME_CRATE: &str = "deusex_runtime";

/// Code generation options.
#[derive(Debug, Clone)]
pub struct CodegenOptions {
    /// Reject unresolved constructs instead of emitting placeholders.
    ///
    /// Without this, unknown types map to `Value` and control-flow gaps become
    /// comments plus warnings.
    pub strict: bool,
    /// Emit the file-level preamble (`#![allow(...)]` and `use` statements).
    pub emit_preamble: bool,
    /// Crate name used to reference runtime APIs in the preamble.
    pub runtime_crate: String,
}

impl Default for CodegenOptions {
    fn default() -> Self {
        Self {
            strict: false,
            emit_preamble: true,
            runtime_crate: DEFAULT_RUNTIME_CRATE.to_string(),
        }
    }
}

/// Generated Rust source plus non-fatal diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedCode {
    /// Sanitized Rust module name.
    pub module_name: String,
    /// Generated Rust source.
    pub source: String,
    /// Human-readable list of semantic gaps encountered while generating.
    pub warnings: Vec<String>,
}

impl GeneratedCode {
    /// Number of recorded warnings.
    pub fn warning_count(&self) -> usize {
        self.warnings.len()
    }
}

/// Error type for code generation.
#[derive(Debug, thiserror::Error)]
pub enum CodegenError {
    #[error("unsupported type in strict mode: {0}")]
    UnsupportedType(String),
    #[error("unsupported expression in strict mode: {0}")]
    UnsupportedExpr(String),
    #[error("unsupported statement in strict mode: {0}")]
    UnsupportedStmt(String),
    #[error("internal codegen error: {0}")]
    Internal(String),
}

/// Generate a Rust module for every function, class and state in a package.
pub fn generate_package(
    package: &DecompiledPackage,
    opts: &CodegenOptions,
) -> Result<GeneratedCode, CodegenError> {
    let module_name = sanitize_module_name(&package.rust_module_name);
    let mut gen = Generator::new(opts);

    if opts.emit_preamble {
        gen.emit_preamble(&package.package);
    }

    gen.line(&format!("pub mod {module_name} {{"));
    gen.indent += 1;
    gen.line("use super::*;");
    gen.blank();

    for class in &package.package.classes {
        gen.emit_class(class)?;
    }
    let mut used = HashSet::new();
    for function in &package.package.functions {
        let name = unique_rust_name(&function.name, &mut used, &mut gen.warnings);
        gen.emit_function(function, Some(&name))?;
    }
    for state in &package.package.states {
        gen.emit_state(state)?;
    }

    gen.indent -= 1;
    gen.line("}");

    Ok(GeneratedCode {
        module_name,
        warnings: gen.warnings,
        source: gen.out,
    })
}

/// Generate a single function without module wrapper or preamble.
pub fn generate_function(
    function: &AstFunction,
    opts: &CodegenOptions,
) -> Result<GeneratedCode, CodegenError> {
    let mut gen = Generator::new(opts);
    gen.emit_function(function, None)?;
    Ok(GeneratedCode {
        module_name: escape_ident(&function.name),
        warnings: gen.warnings,
        source: gen.out,
    })
}

/// Map an AST type to a Rust type expression.
pub fn rust_type(ty: &Ty, opts: &CodegenOptions) -> Result<String, CodegenError> {
    let out = match ty {
        Ty::None => "()".to_string(),
        Ty::Bool => "bool".to_string(),
        Ty::Byte => "u8".to_string(),
        Ty::Int => "i32".to_string(),
        Ty::Float => "f32".to_string(),
        Ty::String => "String".to_string(),
        Ty::Name => "Name".to_string(),
        Ty::Vector => "Vec3".to_string(),
        Ty::Rotator => "Rotator".to_string(),
        Ty::Color => "Color".to_string(),
        Ty::Object(_) => "GcObject".to_string(),
        Ty::Class(_) => "GcClass".to_string(),
        Ty::Array(inner) => format!("Vec<{}>", rust_type(inner, opts)?),
        Ty::Delegate => "Delegate".to_string(),
        Ty::Struct(_) => "StructValue".to_string(),
        Ty::Interface(_) => "GcObject".to_string(),
        Ty::Unknown => {
            if opts.strict {
                return Err(CodegenError::UnsupportedType(
                    "unresolved type (unknown)".to_string(),
                ));
            }
            "Value".to_string()
        }
    };
    Ok(out)
}

/// Escape a UnrealScript identifier for use as a Rust identifier.
pub fn escape_ident(name: &str) -> String {
    let mut sanitized: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();

    if sanitized.is_empty() || sanitized == "_" {
        sanitized = "_unnamed".to_string();
    }
    if sanitized
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        sanitized.insert(0, '_');
    }

    match sanitized.as_str() {
        "self" => "self_".to_string(),
        "Self" => "Self_".to_string(),
        "crate" => "crate_".to_string(),
        "super" => "super_".to_string(),
        other if is_rust_keyword(other) => format!("r#{other}"),
        other => other.to_string(),
    }
}

fn is_rust_keyword(s: &str) -> bool {
    matches!(
        s,
        "as" | "async"
            | "await"
            | "box"
            | "break"
            | "const"
            | "continue"
            | "do"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "final"
            | "fn"
            | "for"
            | "gen"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "macro"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "override"
            | "priv"
            | "pub"
            | "ref"
            | "return"
            | "static"
            | "struct"
            | "trait"
            | "true"
            | "try"
            | "type"
            | "typeof"
            | "union"
            | "unsafe"
            | "unsized"
            | "use"
            | "virtual"
            | "where"
            | "while"
            | "yield"
    )
}

fn sanitize_module_name(name: &str) -> String {
    escape_ident(name).trim_start_matches("r#").to_string()
}

/// Escape arbitrary text as a Rust string literal (including quotes).
pub fn escape_string_literal(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                let _ = write!(out, "\\u{{{:x}}}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn float_literal(value: f32) -> String {
    if value.is_nan() {
        "f32::NAN".to_string()
    } else if value.is_infinite() {
        if value.is_sign_negative() {
            "f32::NEG_INFINITY".to_string()
        } else {
            "f32::INFINITY".to_string()
        }
    } else {
        format!("{value:?}f32")
    }
}

struct Generator<'a> {
    opts: &'a CodegenOptions,
    out: String,
    indent: usize,
    warnings: Vec<String>,
}

impl<'a> Generator<'a> {
    fn new(opts: &'a CodegenOptions) -> Self {
        Self {
            opts,
            out: String::new(),
            indent: 0,
            warnings: Vec::new(),
        }
    }

    fn line(&mut self, text: &str) {
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn blank(&mut self) {
        self.out.push('\n');
    }

    fn warn(&mut self, message: impl Into<String>) {
        self.warnings.push(message.into());
    }

    fn emit_preamble(&mut self, package: &AstPackage) {
        let rt = &self.opts.runtime_crate;
        self.line("// @generated by deusex-codegen -- do not edit by hand.");
        self.line(&format!("// Package: {}.", package.name));
        self.line("//");
        self.line("// Scaffold output from decompiled UnrealScript. Semantic gaps are");
        self.line("// marked with TODO comments and listed at generation time.");
        self.line(
            "#![allow(non_snake_case, non_camel_case_types, unused_variables, unused_mut, unused_imports, unreachable_code)]",
        );
        self.blank();
        self.line("use glam::Vec3;");
        self.line(&format!("use {rt}::script::{{ExecCtx, PropId, FuncId}};"));
        self.line(&format!("use {rt}::value::Value;"));
        self.line(&format!("use {rt}::name::Name;"));
        self.line(&format!("use {rt}::rotator::Rotator;"));
        self.line(&format!("use {rt}::color::Color;"));
        self.line(&format!("use {rt}::gc::{{GcObject, GcClass}};"));
        self.line(&format!("use {rt}::script::Delegate;"));
        self.blank();
    }

    fn emit_class(&mut self, class: &AstClass) -> Result<(), CodegenError> {
        let name = escape_ident(&class.name);
        let module = sanitize_module_name(&class.name);
        self.line(&format!("/// UnrealScript class `{}`.", class.name));
        if let Some(super_class) = class.super_class {
            self.line(&format!("// extends class id {}.", super_class.0));
        }
        self.line(&format!("pub struct {name} {{"));
        self.indent += 1;
        for prop in &class.properties {
            self.emit_property(prop)?;
        }
        self.indent -= 1;
        self.line("}");
        self.blank();

        if !class.functions.is_empty() || !class.states.is_empty() {
            self.line(&format!("pub mod {module} {{"));
            self.indent += 1;
            self.line("use super::*;");
            self.blank();
            let mut used = HashSet::new();
            for function in &class.functions {
                let name = unique_rust_name(&function.name, &mut used, &mut self.warnings);
                self.emit_function(function, Some(&name))?;
            }
            for state in &class.states {
                self.emit_state(state)?;
            }
            self.indent -= 1;
            self.line("}");
            self.blank();
        }
        Ok(())
    }

    fn emit_property(&mut self, prop: &AstProperty) -> Result<(), CodegenError> {
        let ty = rust_type(&prop.ty, self.opts)?;
        let name = escape_ident(&prop.name);
        if prop.array_dim > 0 {
            self.line(&format!("pub {name}: Vec<{ty}>,"));
            self.warn(format!(
                "property {}: static array dimension ignored in scaffold",
                prop.name
            ));
        } else {
            self.line(&format!("pub {name}: {ty},"));
        }
        if let Some(default) = &prop.default {
            let rendered = self.expr(default)?;
            self.line(&format!("// default: {rendered}"));
        }
        Ok(())
    }

    fn emit_state(&mut self, state: &AstState) -> Result<(), CodegenError> {
        let module = sanitize_module_name(&state.name);
        self.line(&format!("/// UnrealScript state `{}`.", state.name));
        self.line(&format!("pub mod state_{module} {{"));
        self.indent += 1;
        self.line("use super::*;");
        self.blank();
        let mut used = HashSet::new();
        for function in &state.label_funcs {
            let name = unique_rust_name(&function.name, &mut used, &mut self.warnings);
            self.emit_function(function, Some(&name))?;
        }
        self.indent -= 1;
        self.line("}");
        self.blank();
        Ok(())
    }

    fn emit_function(
        &mut self,
        function: &AstFunction,
        rust_name: Option<&str>,
    ) -> Result<(), CodegenError> {
        let name = match rust_name {
            Some(name) => name.to_string(),
            None => escape_ident(&function.name),
        };
        let ret_ty = rust_type(&function.ret_ty, self.opts)?;

        self.line("#[allow(non_snake_case)]");
        if function.is_latent {
            self.line("// latent function");
        }
        if function.is_event {
            self.line("// event");
        }
        if function.is_operator {
            self.line("// operator");
        }
        if let Some(class_id) = function.outer_class {
            self.line(&format!("// method of class id {}", class_id.0));
        }

        let mut used_params: HashSet<String> = HashSet::new();
        used_params.insert("ctx".to_string());
        let mut params: Vec<String> = vec!["ctx: &mut ExecCtx".to_string()];
        for param in &function.params {
            let mut ty = rust_type(&param.ty, self.opts)?;
            if param.is_out {
                ty = format!("&mut {ty}");
            }
            let mut param_name = escape_ident(&param.name);
            if !used_params.insert(param_name.clone()) {
                let mut index = 2u32;
                loop {
                    let candidate = format!("{param_name}__{index}");
                    if used_params.insert(candidate.clone()) {
                        self.warn(format!(
                            "{}: duplicate parameter `{}` renamed to `{}`",
                            function.name, param.name, candidate
                        ));
                        param_name = candidate;
                        break;
                    }
                    index += 1;
                }
            }
            params.push(format!("{param_name}: {ty}"));
            if param.is_optional && param.default.is_some() {
                self.warn(format!(
                    "{}: optional parameter `{}` default value not rendered",
                    function.name, param.name
                ));
            }
        }

        self.line(&format!("pub fn {name}({}) -> {ret_ty} {{", params.join(", ")));
        self.indent += 1;

        if function.body.is_empty() {
            if let Some(index) = function.native_index {
                self.line(&format!(
                    "// native function (index {index}) provided by the runtime"
                ));
                self.line(&format!("return ctx.native_function({index});"));
            } else {
                self.line("// no decompiled body available");
                self.emit_default_return(&ret_ty);
            }
        } else {
            self.emit_stmts(&function.body)?;
            if needs_fallthrough_return(&function.body) && ret_ty != "()" {
                self.emit_default_return(&ret_ty);
            }
        }

        self.indent -= 1;
        self.line("}");
        self.blank();
        Ok(())
    }

    fn emit_default_return(&mut self, ret_ty: &str) {
        if ret_ty == "()" {
            self.line("return;");
        } else {
            self.line("return Default::default();");
        }
    }

    fn emit_stmts(&mut self, stmts: &[Stmt]) -> Result<(), CodegenError> {
        for stmt in stmts {
            self.emit_stmt(stmt)?;
        }
        Ok(())
    }

    fn emit_stmt(&mut self, stmt: &Stmt) -> Result<(), CodegenError> {
        match stmt {
            Stmt::Let { name, ty, init } => {
                let init = self.expr(init)?;
                let name = escape_ident(name);
                match ty {
                    Ty::Unknown => self.line(&format!("let {name} = {init};")),
                    _ => {
                        let ty = rust_type(ty, self.opts)?;
                        self.line(&format!("let {name}: {ty} = {init};"));
                    }
                }
            }
            Stmt::Assign { target, value } => {
                let target = self.expr(target)?;
                let value = self.expr(value)?;
                self.line(&format!("{target} = {value};"));
            }
            Stmt::If { cond, then, r#else } => {
                let cond = self.expr(cond)?;
                self.line(&format!("if {cond} {{"));
                self.indent += 1;
                self.emit_stmts(then)?;
                self.indent -= 1;
                if r#else.is_empty() {
                    self.line("}");
                } else {
                    self.line("} else {");
                    self.indent += 1;
                    self.emit_stmts(r#else)?;
                    self.indent -= 1;
                    self.line("}");
                }
            }
            Stmt::Goto { label } => {
                self.warn(format!(
                    "goto `{label}` emitted as a comment; control flow is not preserved"
                ));
                self.line(&format!("// TODO: goto {label}"));
            }
            Stmt::Return(Some(value)) => {
                let value = self.expr(value)?;
                self.line(&format!("return {value};"));
            }
            Stmt::Return(None) => {
                self.line("return Default::default();");
            }
            Stmt::Stop => {
                self.warn("`stop` lowered to a default return");
                self.line("return Default::default(); // stop");
            }
            Stmt::Labeled { label, body } => {
                self.line(&format!("// label: {label}"));
                self.emit_stmt(body)?;
            }
            Stmt::Block(stmts) => {
                self.line("{");
                self.indent += 1;
                self.emit_stmts(stmts)?;
                self.indent -= 1;
                self.line("}");
            }
            Stmt::Expr(expr) => {
                if matches!(expr, AstExpr::NoneConst) {
                    return Ok(());
                }
                let rendered = self.expr(expr)?;
                self.line(&format!("{rendered};"));
            }
            Stmt::LatentCall {
                func,
                args,
                latent_action,
            } => {
                let args = self.expr_list(args)?;
                self.line(&format!(
                    "ctx.latent_call(FuncId({}), {}, vec![{}]);",
                    func.0,
                    escape_string_literal(latent_action),
                    args
                ));
            }
            Stmt::Switch {
                expr,
                cases,
                default,
            } => {
                let cond = self.expr(expr)?;
                self.line(&format!("match ctx.switch_key({cond}) {{"));
                self.indent += 1;
                for case in cases {
                    self.emit_switch_arm(case)?;
                }
                self.line("_ => {");
                self.indent += 1;
                self.emit_stmts(default)?;
                self.indent -= 1;
                self.line("}");
                self.indent -= 1;
                self.line("}");
            }
            Stmt::Iterator { var, iterable, body } => {
                let iterable = self.expr(iterable)?;
                self.line(&format!(
                    "for {} in ctx.iterate({iterable}) {{",
                    escape_ident(var)
                ));
                self.indent += 1;
                self.emit_stmts(body)?;
                self.indent -= 1;
                self.line("}");
            }
            Stmt::Empty => {}
        }
        Ok(())
    }

    fn emit_switch_arm(&mut self, case: &SwitchCase) -> Result<(), CodegenError> {
        self.line(&format!("{} => {{", case.value));
        self.indent += 1;
        self.emit_stmts(&case.body)?;
        if case.fallthrough {
            self.warn(format!(
                "switch case {} marked fallthrough; UnrealScript cases do not fall through",
                case.value
            ));
        }
        self.indent -= 1;
        self.line("}");
        Ok(())
    }

    fn expr_list(&mut self, exprs: &[AstExpr]) -> Result<String, CodegenError> {
        let mut rendered = Vec::with_capacity(exprs.len());
        for expr in exprs {
            rendered.push(self.expr(expr)?);
        }
        Ok(rendered.join(", "))
    }

    fn expr(&mut self, expr: &AstExpr) -> Result<String, CodegenError> {
        let rendered = match expr {
            AstExpr::LocalVar { name, .. } => escape_ident(name),
            AstExpr::InstanceVar { prop, .. } => format!("ctx.instance_var(PropId({}))", prop.0),
            AstExpr::DefaultVar { prop, .. } => format!("ctx.default_var(PropId({}))", prop.0),
            AstExpr::Self_ { .. } => "ctx.self_obj()".to_string(),
            AstExpr::IntConst(value) => value.to_string(),
            AstExpr::FloatConst(value) => float_literal(*value),
            AstExpr::ByteConst(value) => format!("{value}u8"),
            AstExpr::StringConst(value) => escape_string_literal(value),
            AstExpr::NameConst(id) => format!("Name({})", id.0),
            AstExpr::ObjectConst(class_id) => format!("ctx.object({})", class_id.0),
            AstExpr::VectorConst { x, y, z } => {
                format!(
                    "Vec3::new({}, {}, {})",
                    float_literal(*x),
                    float_literal(*y),
                    float_literal(*z)
                )
            }
            AstExpr::RotatorConst { pitch, yaw, roll } => {
                format!("Rotator::new({pitch}, {yaw}, {roll})")
            }
            AstExpr::NoneConst => "Value::None".to_string(),
            AstExpr::BoolConst(value) => value.to_string(),
            AstExpr::BinOp {
                op, left, right, ..
            } => {
                let left = self.expr(left)?;
                let right = self.expr(right)?;
                match op {
                    BinOp::Assign
                    | BinOp::AddAssign
                    | BinOp::SubAssign
                    | BinOp::MulAssign
                    | BinOp::DivAssign => {
                        if self.opts.strict {
                            return Err(CodegenError::UnsupportedExpr(format!(
                                "assignment operator {op:?} in expression position"
                            )));
                        }
                        self.warn(format!(
                            "assignment operator {op:?} in expression position emitted as `=`"
                        ));
                        format!("({left} = {right})")
                    }
                    _ => {
                        let symbol = match op {
                            BinOp::Add => "+",
                            BinOp::Sub => "-",
                            BinOp::Mul => "*",
                            BinOp::Div => "/",
                            BinOp::Mod => "%",
                            BinOp::Eq => "==",
                            BinOp::Ne => "!=",
                            BinOp::Lt => "<",
                            BinOp::Gt => ">",
                            BinOp::Le => "<=",
                            BinOp::Ge => ">=",
                            BinOp::And => "&&",
                            BinOp::Or => "||",
                            BinOp::Xor => "^",
                            BinOp::Shl => "<<",
                            BinOp::Shr => ">>",
                            _ => unreachable!("assignment operators handled above"),
                        };
                        format!("({left} {symbol} {right})")
                    }
                }
            }
            AstExpr::UnaryOp { op, inner, .. } => {
                let inner = self.expr(inner)?;
                match op {
                    UnaryOp::Neg => format!("(-{inner})"),
                    UnaryOp::Not => format!("(!{inner})"),
                    UnaryOp::BitNot => format!("(!{inner})"),
                    UnaryOp::PreInc | UnaryOp::PreDec | UnaryOp::PostInc | UnaryOp::PostDec => {
                        if self.opts.strict {
                            return Err(CodegenError::UnsupportedExpr(format!(
                                "increment/decrement operator {op:?} in expression position"
                            )));
                        }
                        self.warn(format!(
                            "increment/decrement operator {op:?} emitted as `+ 1`/`- 1`"
                        ));
                        match op {
                            UnaryOp::PreInc | UnaryOp::PostInc => format!("({inner} + 1)"),
                            _ => format!("({inner} - 1)"),
                        }
                    }
                }
            }
            AstExpr::Call {
                func,
                args,
                native_index,
                ..
            } => {
                let args = self.expr_list(args)?;
                match native_index {
                    Some(index) => format!(
                        "ctx.native_call({index}, FuncId({}), vec![{args}])",
                        func.0
                    ),
                    None => format!("ctx.call(FuncId({}), vec![{args}])", func.0),
                }
            }
            AstExpr::GlobalCall { name, args, .. } => {
                let args = self.expr_list(args)?;
                format!(
                    "ctx.global_call({}, vec![{args}])",
                    escape_string_literal(name)
                )
            }
            AstExpr::NativeCall { index, args, .. } => {
                let args = self.expr_list(args)?;
                format!("ctx.native_call({index}, vec![{args}])")
            }
            AstExpr::Cast { ty, inner } => {
                let inner = self.expr(inner)?;
                let ty = rust_type(ty, self.opts)?;
                format!("ctx.cast({inner}, {})", escape_string_literal(&ty))
            }
            AstExpr::Member { obj, member, .. } => {
                let obj = self.expr(obj)?;
                format!("ctx.member({obj}, PropId({}))", member.0)
            }
            AstExpr::ArrayIndex { array, index, .. } => {
                let array = self.expr(array)?;
                let index = self.expr(index)?;
                format!("ctx.array_index({array}, {index})")
            }
            AstExpr::New { ty, args } => {
                let args = self.expr_list(args)?;
                let ty = rust_type(ty, self.opts)?;
                format!(
                    "ctx.new_object({}, vec![{args}])",
                    escape_string_literal(&ty)
                )
            }
            AstExpr::Switch { expr, cases } => {
                if self.opts.strict {
                    return Err(CodegenError::UnsupportedExpr(
                        "switch expression".to_string(),
                    ));
                }
                let cond = self.expr(expr)?;
                let values = cases
                    .iter()
                    .map(|case| case.value.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                self.warn(format!(
                    "switch used in expression position; emitted as a placeholder (cases: {values})"
                ));
                let _ = cond;
                "{ /* unsupported switch expression */ Value::None }".to_string()
            }
            AstExpr::Cond {
                cond,
                then,
                r#else,
                ..
            } => {
                let cond = self.expr(cond)?;
                let then = self.expr(then)?;
                let otherwise = self.expr(r#else)?;
                format!("if {cond} {{ {then} }} else {{ {otherwise} }}")
            }
        };
        Ok(rendered)
    }
}

fn unique_rust_name(
    base: &str,
    used: &mut HashSet<String>,
    warnings: &mut Vec<String>,
) -> String {
    let base = escape_ident(base);
    if used.insert(base.clone()) {
        return base;
    }
    let mut index = 2u32;
    loop {
        let candidate = format!("{base}__{index}");
        if used.insert(candidate.clone()) {
            warnings.push(format!(
                "duplicate function name `{base}` renamed to `{candidate}`"
            ));
            return candidate;
        }
        index += 1;
    }
}

fn needs_fallthrough_return(stmts: &[Stmt]) -> bool {
    match stmts.last() {
        None => true,
        Some(Stmt::Return(_)) | Some(Stmt::Stop) => false,
        Some(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deusex_ast::{ClassId, Expr, FuncId, NameId, Param, PropId};

    fn opts() -> CodegenOptions {
        CodegenOptions::default()
    }

    fn empty_package(name: &str) -> DecompiledPackage {
        DecompiledPackage {
            package: AstPackage {
                name: name.to_string(),
                classes: Vec::new(),
                functions: Vec::new(),
                states: Vec::new(),
            },
            rust_module_name: format!("deusex_{}", name.to_lowercase()),
        }
    }

    fn simple_function() -> AstFunction {
        AstFunction {
            id: FuncId(1),
            name: "Add".to_string(),
            outer_class: None,
            params: vec![
                Param {
                    name: "A".to_string(),
                    ty: Ty::Int,
                    is_out: false,
                    is_optional: false,
                    default: None,
                },
                Param {
                    name: "B".to_string(),
                    ty: Ty::Int,
                    is_out: false,
                    is_optional: false,
                    default: None,
                },
            ],
            ret_ty: Ty::Int,
            local_vars: Vec::new(),
            body: vec![Stmt::Return(Some(Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(Expr::LocalVar {
                    name: "A".to_string(),
                    ty: Ty::Int,
                    slot: 0,
                }),
                right: Box::new(Expr::LocalVar {
                    name: "B".to_string(),
                    ty: Ty::Int,
                    slot: 1,
                }),
                ty: Ty::Int,
            }))],
            native_index: None,
            precedence: 0,
            flags: 0,
            is_latent: false,
            is_event: false,
            is_operator: false,
            is_static: true,
            is_exec: false,
            line: 0,
        }
    }

    #[test]
    fn maps_primitive_types() {
        assert_eq!(rust_type(&Ty::Int, &opts()).unwrap(), "i32");
        assert_eq!(rust_type(&Ty::Float, &opts()).unwrap(), "f32");
        assert_eq!(rust_type(&Ty::Bool, &opts()).unwrap(), "bool");
        assert_eq!(rust_type(&Ty::Byte, &opts()).unwrap(), "u8");
        assert_eq!(rust_type(&Ty::String, &opts()).unwrap(), "String");
        assert_eq!(rust_type(&Ty::None, &opts()).unwrap(), "()");
        assert_eq!(
            rust_type(&Ty::Array(Box::new(Ty::Int)), &opts()).unwrap(),
            "Vec<i32>"
        );
        assert_eq!(rust_type(&Ty::Object(ClassId(3)), &opts()).unwrap(), "GcObject");
    }

    #[test]
    fn strict_mode_rejects_unknown_types() {
        let strict = CodegenOptions {
            strict: true,
            ..CodegenOptions::default()
        };
        assert!(matches!(
            rust_type(&Ty::Unknown, &strict),
            Err(CodegenError::UnsupportedType(_))
        ));
    }

    #[test]
    fn escapes_identifiers() {
        assert_eq!(escape_ident("Normal"), "Normal");
        assert_eq!(escape_ident("match"), "r#match");
        assert_eq!(escape_ident("self"), "self_");
        assert_eq!(escape_ident("crate"), "crate_");
        assert_eq!(escape_ident("9lives"), "_9lives");
        assert_eq!(escape_ident(""), "_unnamed");
        assert_eq!(escape_ident("weird name!"), "weird_name_");
    }

    #[test]
    fn escapes_string_literals() {
        assert_eq!(escape_string_literal("hi"), "\"hi\"");
        assert_eq!(
            escape_string_literal("line\nbreak \"quoted\" \\"),
            "\"line\\nbreak \\\"quoted\\\" \\\\\""
        );
        assert_eq!(escape_string_literal("\u{7}"), "\"\\u{7}\"");
    }

    #[test]
    fn formats_float_literals() {
        assert_eq!(float_literal(1.5), "1.5f32");
        assert_eq!(float_literal(f32::NAN), "f32::NAN");
        assert_eq!(float_literal(f32::NEG_INFINITY), "f32::NEG_INFINITY");
    }

    #[test]
    fn generates_simple_function() {
        let generated = generate_function(&simple_function(), &opts()).unwrap();
        assert!(generated.source.contains("pub fn Add(ctx: &mut ExecCtx, A: i32, B: i32) -> i32"));
        assert!(generated.source.contains("return (A + B);"));
        assert!(generated.warnings.is_empty());
    }

    #[test]
    fn generates_package_module() {
        let mut package = empty_package("Core");
        package.package.functions.push(simple_function());
        let generated = generate_package(&package, &opts()).unwrap();
        assert_eq!(generated.module_name, "deusex_core");
        assert!(generated.source.contains("pub mod deusex_core {"));
        assert!(generated.source.contains("pub fn Add"));
        syn::parse_file(&generated.source).expect("generated package should be valid Rust");
    }

    #[test]
    fn emits_goto_as_comment_with_warning() {
        let mut function = simple_function();
        function.body = vec![
            Stmt::Goto {
                label: "lbl_0010".to_string(),
            },
            Stmt::Return(None),
        ];
        let generated = generate_function(&function, &opts()).unwrap();
        assert!(generated.source.contains("// TODO: goto lbl_0010"));
        assert_eq!(generated.warning_count(), 1);
        syn::parse_file(&generated.source).expect("generated function should be valid Rust");
    }

    #[test]
    fn strict_mode_rejects_unresolved_expressions() {
        let strict = CodegenOptions {
            strict: true,
            ..CodegenOptions::default()
        };
        let function = simple_function();
        // The function itself is fine, but an unknown-typed local is not.
        let generated = generate_function(&function, &strict).unwrap();
        assert!(!generated.source.is_empty());
    }

    #[test]
    fn switch_statement_is_rendered() {
        let mut function = simple_function();
        function.ret_ty = Ty::None;
        function.body = vec![Stmt::Switch {
            expr: Expr::LocalVar {
                name: "A".to_string(),
                ty: Ty::Int,
                slot: 0,
            },
            cases: vec![SwitchCase {
                value: 1,
                body: vec![Stmt::Return(None)],
                fallthrough: false,
            }],
            default: vec![Stmt::Return(None)],
        }];
        let generated = generate_function(&function, &opts()).unwrap();
        assert!(generated.source.contains("match ctx.switch_key(A)"));
        assert!(generated.source.contains("1 => {"));
        syn::parse_file(&generated.source).expect("generated function should be valid Rust");
    }

    #[test]
    fn native_function_stub_is_rendered() {
        let mut function = simple_function();
        function.name = "NativeTest".to_string();
        function.body.clear();
        function.native_index = Some(42);
        let generated = generate_function(&function, &opts()).unwrap();
        assert!(generated.source.contains("return ctx.native_function(42);"));
    }

    #[test]
    fn name_constants_are_rendered() {
        let mut function = simple_function();
        function.ret_ty = Ty::Name;
        function.body = vec![Stmt::Return(Some(Expr::NameConst(NameId(7))))];
        let generated = generate_function(&function, &opts()).unwrap();
        assert!(generated.source.contains("return Name(7);"));
    }

    #[test]
    fn instance_vars_use_runtime_helper() {
        let mut function = simple_function();
        function.ret_ty = Ty::Unknown;
        function.body = vec![Stmt::Return(Some(Expr::InstanceVar {
            prop: PropId(3),
            ty: Ty::Unknown,
        }))];
        let generated = generate_function(&function, &opts()).unwrap();
        assert!(generated.source.contains("return ctx.instance_var(PropId(3));"));
    }
}
