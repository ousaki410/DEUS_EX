//! UnrealScript-like pseudocode rendering.
//!
//! Unlike [`crate::decompile`], this pass renders the decoded expression tree
//! directly and keeps resolved symbol names (locals, properties, object paths,
//! native functions and operators). The output is a linear, goto-based
//! pseudocode listing: every jump target gets a label, so the semantics of the
//! original bytecode stay visible without reconstructing structured control
//! flow.

use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;

use deusex_package::{
    function_parameters, property_flags, read_property_info, UPropertyInfo, UnrealPackage,
};

use crate::function::{function_flags, Expr, ExprKind, Function};

/// Native index -> full script path (`254 -> Core.Object.EqualEqual_NameName`).
#[derive(Debug, Clone, Default)]
pub struct NativeTable {
    names: BTreeMap<u32, String>,
}

impl NativeTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a native function. The first declaration wins; later packages
    /// frequently export duplicate indices for inherited natives.
    pub fn insert(&mut self, index: u32, path: impl Into<String>) {
        self.names.entry(index).or_insert_with(|| path.into());
    }

    pub fn get(&self, index: u32) -> Option<&str> {
        self.names.get(&index).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

/// Render one function as pseudocode.
pub fn render_function(
    package: &UnrealPackage,
    function: &Function,
    natives: &NativeTable,
) -> String {
    let mut renderer = Renderer::new(package, natives);
    renderer.render(function)
}

struct Renderer<'a> {
    package: &'a UnrealPackage,
    natives: &'a NativeTable,
    named_labels: BTreeMap<u32, String>,
    targets: BTreeMap<u32, String>,
    emitted: HashSet<u32>,
    top_offsets: HashSet<u32>,
}

impl<'a> Renderer<'a> {
    fn new(package: &'a UnrealPackage, natives: &'a NativeTable) -> Self {
        Self {
            package,
            natives,
            named_labels: BTreeMap::new(),
            targets: BTreeMap::new(),
            emitted: HashSet::new(),
            top_offsets: HashSet::new(),
        }
    }

    fn render(&mut self, function: &Function) -> String {
        self.top_offsets = function.body.iter().map(|expr| expr.offset).collect();

        for expr in &function.body {
            self.collect_named_labels(expr);
        }
        for expr in &function.body {
            self.collect_targets(expr);
        }

        let signature = self.render_signature(function);

        let mut out = String::new();

        if function.is_native()
            && (function.body.is_empty() || is_stub_body(&function.body))
        {
            writeln!(out, "{signature}; // native index {}", function.native_index).unwrap();
            return out;
        }

        writeln!(out, "{signature}").unwrap();

        if function.body.is_empty() {
            out.push_str("{\n}\n");
            return out;
        }

        out.push_str("{\n");
        for expr in &function.body {
            let label = self.targets.get(&expr.offset).cloned();
            if let Some(label) = label {
                writeln!(out, "    {label}:").unwrap();
                self.emitted.insert(expr.offset);
            }

            let statement = self.render_stmt(expr);
            if statement.is_empty() {
                continue;
            }
            for line in statement.lines() {
                writeln!(out, "    {line}").unwrap();
            }
        }

        // Labels that point inside an expression (short-circuit helpers) are
        // still emitted so every `goto` has a destination.
        for (offset, label) in &self.targets {
            if !self.emitted.contains(offset) {
                writeln!(out, "    {label}: // nested expression target").unwrap();
            }
        }

        out.push_str("}\n");
        out
    }

    fn collect_named_labels(&mut self, expr: &Expr) {
        if let ExprKind::LabelTable(labels) = &expr.kind {
            for (name_index, offset) in labels {
                if *name_index == 0 {
                    continue;
                }
                if let Some(name) = self.package.name(*name_index) {
                    self.named_labels.insert(*offset, name.to_string());
                }
            }
        }
        for child in expr.children() {
            self.collect_named_labels(child);
        }
    }

    fn collect_targets(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Jump(offset) => self.add_target(*offset as u32),
            ExprKind::JumpIfNot { offset, condition } => {
                self.add_target(*offset as u32);
                self.collect_targets(condition);
            }
            // `Assert` stores a source line number, not a jump target.
            ExprKind::Assert { condition, .. } => self.collect_targets(condition),
            ExprKind::Case { offset, value } => {
                // `0xFFFF` marks the default case; it has no jump target.
                if !(*offset == 0xFFFF && value.is_none()) {
                    self.add_target(*offset as u32);
                }
                if let Some(value) = value {
                    self.collect_targets(value);
                }
            }
            ExprKind::Iterator { value, offset } => {
                self.add_target(*offset as u32);
                self.collect_targets(value);
            }
            ExprKind::LabelTable(labels) => {
                for (name_index, offset) in labels {
                    if *name_index != 0 {
                        self.add_target(*offset);
                    }
                }
            }
            _ => {}
        }

        for child in expr.children() {
            self.collect_targets(child);
        }
    }

    fn add_target(&mut self, offset: u32) {
        let label = self
            .named_labels
            .get(&offset)
            .cloned()
            .unwrap_or_else(|| format!("L{offset:04X}"));
        self.targets.entry(offset).or_insert(label);
    }

    fn label(&self, offset: u32) -> String {
        self.targets
            .get(&offset)
            .cloned()
            .unwrap_or_else(|| format!("L{offset:04X}"))
    }

    fn render_signature(&self, function: &Function) -> String {
        let parameters =
            function_parameters(self.package, function.children_ref).unwrap_or_default();
        let return_type = parameters
            .iter()
            .find(|parameter| parameter.is_return_parm())
            .map(|parameter| self.property_type(parameter, 0))
            .unwrap_or_else(|| "void".to_string());

        let inputs = parameters
            .iter()
            .filter(|parameter| {
                !parameter.is_return_parm()
                    && (parameter.flags & property_flags::SKIP_PARM) == 0
            })
            .map(|parameter| {
                let mut text = String::new();
                if parameter.is_out_parm() {
                    text.push_str("out ");
                }
                text.push_str(&parameter.name);
                text.push_str(": ");
                text.push_str(&self.property_type(parameter, 0));
                if parameter.flags & 0x0000_0010 != 0 {
                    // CPF_OptionalParm is not exported as a named constant yet.
                    text.push('?');
                }
                text
            })
            .collect::<Vec<_>>()
            .join(", ");

        let mut prefix = String::new();
        if (function.function_flags & function_flags::STATIC) != 0 {
            prefix.push_str("static ");
        }
        if function.is_native() {
            prefix.push_str("native ");
        }

        format!(
            "{prefix}function {return_type} {}({inputs})",
            function.name
        )
    }

    fn property_type(&self, property: &UPropertyInfo, depth: usize) -> String {
        if depth > 8 {
            return "?".to_string();
        }

        let resolved = |reference: i32, fallback: &str| {
            self.package
                .object_name(reference)
                .map(str::to_string)
                .unwrap_or_else(|| fallback.to_string())
        };

        match property.class_name.as_str() {
            "ByteProperty" if property.type_ref != 0 => resolved(property.type_ref, "byte"),
            "ByteProperty" => "byte".to_string(),
            "IntProperty" => "int".to_string(),
            "BoolProperty" => "bool".to_string(),
            "FloatProperty" => "float".to_string(),
            "StrProperty" => "string".to_string(),
            "NameProperty" => "name".to_string(),
            "ObjectProperty" => resolved(property.type_ref, "object"),
            "ClassProperty" => format!("class<{}>", resolved(property.type_ref, "object")),
            "StructProperty" => resolved(property.type_ref, "struct"),
            "ArrayProperty" => {
                if property.type_ref == 0 {
                    "array<?>".to_string()
                } else {
                    match read_property_info(self.package, property.type_ref) {
                        Ok(inner) => {
                            format!("array<{}>", self.property_type(&inner, depth + 1))
                        }
                        Err(_) => "array<?>".to_string(),
                    }
                }
            }
            "FixedArrayProperty" => {
                if property.type_ref == 0 {
                    "array<?>".to_string()
                } else {
                    match read_property_info(self.package, property.type_ref) {
                        Ok(inner) => {
                            format!("array<{}>", self.property_type(&inner, depth + 1))
                        }
                        Err(_) => "array<?>".to_string(),
                    }
                }
            }
            "PointerProperty" => "pointer".to_string(),
            other => other.trim_end_matches("Property").to_string(),
        }
    }

    fn render_stmt(&mut self, expr: &Expr) -> String {
        match &expr.kind {
            ExprKind::Nothing
            | ExprKind::Unknown15
            | ExprKind::EndFunctionParms
            | ExprKind::IteratorPop => String::new(),
            ExprKind::Let { target, value } | ExprKind::LetBool { target, value } => {
                format!(
                    "{} = {};",
                    self.render_target(target),
                    self.render_expr(value, 0)
                )
            }
            ExprKind::Return(value) => {
                if matches!(value.kind, ExprKind::Nothing) {
                    "return;".to_string()
                } else {
                    format!("return {};", self.render_expr(value, 0))
                }
            }
            ExprKind::Jump(offset) => format!("goto {};", self.label(*offset as u32)),
            ExprKind::JumpIfNot { offset, condition } => format!(
                "if ({}) goto {};",
                self.render_negated_condition(condition),
                self.label(*offset as u32)
            ),
            ExprKind::Assert { offset, condition } => format!(
                "assert({}); // line {offset}",
                self.render_expr(condition, 0)
            ),
            ExprKind::Case { offset, value } => match value {
                Some(value) => format!(
                    "case {}: goto {};",
                    self.render_expr(value, 0),
                    self.label(*offset as u32)
                ),
                None if *offset == 0xFFFF => "default:".to_string(),
                None => format!("default: goto {};", self.label(*offset as u32)),
            },
            ExprKind::Switch(_, value) => format!("switch ({})", self.render_expr(value, 0)),
            ExprKind::Stop => "stop;".to_string(),
            ExprKind::Iterator { value, offset } => format!(
                "foreach ({})  // end {}",
                self.render_expr(value, 0),
                self.label(*offset as u32)
            ),
            ExprKind::IteratorNext => "continue;".to_string(),
            ExprKind::GotoLabel(value) => format!("goto *{};", self.render_expr(value, 0)),
            ExprKind::EatString(value) => format!("eatstring({});", self.render_expr(value, 0)),
            ExprKind::LabelTable(labels) => {
                let names = labels
                    .iter()
                    .filter(|(name, _)| *name != 0)
                    .filter_map(|(name, offset)| {
                        self.package
                            .name(*name)
                            .map(|name| format!("{name}=L{offset:04X}"))
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("// label table: {names}")
            }
            _ => format!("{};", self.render_expr(expr, 0)),
        }
    }

    /// Render a condition for `JumpIfNot`, folding the negation into boolean
    /// variables and comparison operators.
    fn render_negated_condition(&mut self, condition: &Expr) -> String {
        match &condition.kind {
            ExprKind::BoolVariable(inner) => self.render_expr(inner, 0),
            ExprKind::True => "false".to_string(),
            ExprKind::False => "true".to_string(),
            ExprKind::Native { index, params } => {
                self.render_negated_native(u32::from(*index), params)
            }
            ExprKind::ExtendedNative { index, params } => {
                self.render_negated_native(u32::from(*index), params)
            }
            _ => format!("!({})", self.render_expr(condition, 0)),
        }
    }

    fn render_negated_native(&mut self, index: u32, params: &[Expr]) -> String {
        let path = self.natives.get(index).unwrap_or("").to_string();
        let short = path.rsplit('.').next().unwrap_or("");

        // `JumpIfNot(Not_PreBool(x))`: the two negations cancel out.
        if short == "Not_PreBool" && params.len() == 1 {
            return self.render_expr(&params[0], 0);
        }

        let inverted = if short.starts_with("EqualEqual_") {
            Some("!=")
        } else if short.starts_with("NotEqual_") || short.starts_with("ComplementEqual_") {
            Some("==")
        } else if short.starts_with("LessEqual_") {
            Some(">")
        } else if short.starts_with("GreaterEqual_") {
            Some("<")
        } else if short.starts_with("Less_") {
            Some(">=")
        } else if short.starts_with("Greater_") {
            Some("<=")
        } else {
            None
        };

        if let (Some(op), 2) = (inverted, params.len()) {
            let precedence = 6;
            let rendered = format!(
                "{} {op} {}",
                self.render_expr(&params[0], precedence),
                self.render_expr(&params[1], precedence + 1)
            );
            return rendered;
        }

        format!("!({})", self.render_native(index, params, 0))
    }

    fn render_target(&mut self, target: &Expr) -> String {
        match &target.kind {
            ExprKind::BoolVariable(inner) => self.render_target(inner),
            _ => self.render_expr(target, 0),
        }
    }

    fn render_expr(&mut self, expr: &Expr, parent_precedence: u8) -> String {
        match &expr.kind {
            ExprKind::LocalVariable(reference) | ExprKind::NativeParm(reference) => {
                self.object_name(*reference, "local")
            }
            ExprKind::InstanceVariable(reference) => self.object_name(*reference, "property"),
            ExprKind::DefaultVariable(reference) => {
                format!("default.{}", self.object_name(*reference, "property"))
            }
            ExprKind::Return(value) => {
                if matches!(value.kind, ExprKind::Nothing) {
                    "return".to_string()
                } else {
                    format!("return {}", self.render_expr(value, 0))
                }
            }
            ExprKind::Switch(_, value) => format!("switch ({})", self.render_expr(value, 0)),
            ExprKind::Jump(offset) => format!("goto {}", self.label(*offset as u32)),
            ExprKind::JumpIfNot { offset, condition } => format!(
                "if (!({})) goto {}",
                self.render_expr(condition, 0),
                self.label(*offset as u32)
            ),
            ExprKind::Assert { offset, condition } => {
                format!("assert({}) /* line {offset} */", self.render_expr(condition, 0))
            }
            ExprKind::Case { offset, value } => match value {
                Some(value) => format!(
                    "case {}: goto {}",
                    self.render_expr(value, 0),
                    self.label(*offset as u32)
                ),
                None if *offset == 0xFFFF => "default:".to_string(),
                None => format!("default: goto {}", self.label(*offset as u32)),
            },
            ExprKind::Stop => "stop".to_string(),
            ExprKind::Nothing => "None".to_string(),
            ExprKind::LabelTable(_) => "/* label table */".to_string(),
            ExprKind::GotoLabel(value) => format!("goto *{}", self.render_expr(value, 0)),
            ExprKind::EatString(value) => format!("eatstring({})", self.render_expr(value, 0)),
            ExprKind::Let { target, value } | ExprKind::LetBool { target, value } => format!(
                "({} = {})",
                self.render_target(target),
                self.render_expr(value, 0)
            ),
            ExprKind::DynArrayElement { array, index }
            | ExprKind::ArrayElement { array, index } => format!(
                "{}[{}]",
                self.render_expr(array, 13),
                self.render_expr(index, 0)
            ),
            ExprKind::New(items) => format!("new({})", self.render_args(items)),
            ExprKind::ClassContext { context, value, .. }
            | ExprKind::Context { context, value, .. } => format!(
                "{}.{}",
                self.render_expr(context, 13),
                self.render_expr(value, 13)
            ),
            ExprKind::MetaCast { class, value } => format!(
                "class'{}'({})",
                self.object_path(*class, "class"),
                self.render_expr(value, 0)
            ),
            ExprKind::Unknown15 => "/* unknown15 */".to_string(),
            ExprKind::EndFunctionParms => String::new(),
            ExprKind::SelfObject => "self".to_string(),
            ExprKind::Skip { value, .. } | ExprKind::Unknown2B { value, .. } => {
                self.render_expr(value, parent_precedence)
            }
            ExprKind::VirtualFunction { name, params } => {
                let function_name = self
                    .package
                    .name(*name)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("virtual_{name}"));
                format!("{function_name}({})", self.render_args(params))
            }
            ExprKind::FinalFunction { function, params } => format!(
                "{}({})",
                self.object_path(*function, "function"),
                self.render_args(params)
            ),
            ExprKind::IntConst(value) => value.to_string(),
            ExprKind::FloatConst(value) => format_float(*value),
            ExprKind::StringConst(value) | ExprKind::UnicodeStringConst(value) => {
                format!("\"{}\"", escape_string(value))
            }
            ExprKind::ObjectConst(reference) => self.object_path(*reference, "object"),
            ExprKind::NameConst(index) => {
                let name = self
                    .package
                    .name(*index)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("name_{index}"));
                format!("'{name}'")
            }
            ExprKind::RotationConst(value) => format!(
                "rot({}, {}, {})",
                format_float(value[0]),
                format_float(value[1]),
                format_float(value[2])
            ),
            ExprKind::VectorConst(value) => format!(
                "vect({}, {}, {})",
                format_float(value[0]),
                format_float(value[1]),
                format_float(value[2])
            ),
            ExprKind::ByteConst(value) | ExprKind::IntConstByte(value) => value.to_string(),
            ExprKind::IntZero => "0".to_string(),
            ExprKind::IntOne => "1".to_string(),
            ExprKind::True => "true".to_string(),
            ExprKind::False => "false".to_string(),
            ExprKind::NoObject => "None".to_string(),
            // `EX_BoolVariable` wraps a bool variable read; it is not a
            // negation (negation is the `Not_PreBool` native).
            ExprKind::BoolVariable(inner) => self.render_expr(inner, parent_precedence),
            ExprKind::DynamicCast { class, value } => format!(
                "class'{}'({})",
                self.object_path(*class, "class"),
                self.render_expr(value, 0)
            ),
            ExprKind::Iterator { value, offset } => format!(
                "foreach ({})  // end {}",
                self.render_expr(value, 0),
                self.label(*offset as u32)
            ),
            ExprKind::IteratorPop => "}".to_string(),
            ExprKind::IteratorNext => "continue".to_string(),
            ExprKind::StructCmpEq {
                struct_ref: _,
                left,
                right,
            } => self.render_binary("==", 6, left, right, parent_precedence),
            ExprKind::StructCmpNe {
                struct_ref: _,
                left,
                right,
            } => self.render_binary("!=", 6, left, right, parent_precedence),
            ExprKind::StructMember { struct_ref, value } => format!(
                "{}.{}",
                self.render_expr(value, 13),
                self.object_name(*struct_ref, "member")
            ),
            ExprKind::Conversion { opcode: _, value } => self.render_expr(value, parent_precedence),
            ExprKind::GlobalFunction { name, params } => {
                let function_name = self
                    .package
                    .name(*name)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("global_{name}"));
                format!("{function_name}({})", self.render_args(params))
            }
            ExprKind::ExtendedNative { index, params } => {
                self.render_native(*index as u32, params, parent_precedence)
            }
            ExprKind::Native { index, params } => {
                self.render_native(u32::from(*index), params, parent_precedence)
            }
        }
    }

    fn render_binary(
        &mut self,
        op: &str,
        precedence: u8,
        left: &Expr,
        right: &Expr,
        parent_precedence: u8,
    ) -> String {
        let rendered = format!(
            "{} {op} {}",
            self.render_expr(left, precedence),
            self.render_expr(right, precedence + 1)
        );
        if precedence < parent_precedence {
            format!("({rendered})")
        } else {
            rendered
        }
    }

    fn render_args(&mut self, args: &[Expr]) -> String {
        args.iter()
            .map(|arg| self.render_expr(arg, 0))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn render_native(&mut self, index: u32, params: &[Expr], parent_precedence: u8) -> String {
        let path = self.natives.get(index).unwrap_or("").to_string();
        let short = path.rsplit('.').next().unwrap_or("");
        let function_name = if short.is_empty() {
            format!("native_{index}")
        } else {
            short.to_string()
        };

        if let Some(operator) = operator_for(&function_name) {
            match operator {
                Operator::Binary(op, precedence) if params.len() == 2 => {
                    return self.render_binary(
                        op,
                        precedence,
                        &params[0],
                        &params[1],
                        parent_precedence,
                    );
                }
                Operator::Assign(op) if params.len() == 2 && is_assignable(&params[0]) => {
                    return format!(
                        "{} {op} {}",
                        self.render_target(&params[0]),
                        self.render_expr(&params[1], 0)
                    );
                }
                Operator::Prefix(op) if params.len() == 1 => {
                    return format!("{op}{}", self.render_expr(&params[0], 12));
                }
                Operator::Postfix(op) if params.len() == 1 => {
                    return format!("{}{op}", self.render_expr(&params[0], 12));
                }
                _ => {}
            }
        }

        format!("{function_name}({})", self.render_args(params))
    }

    fn object_name(&self, reference: i32, fallback: &str) -> String {
        self.package
            .object_name(reference)
            .filter(|name| *name != "None")
            .map(str::to_string)
            .unwrap_or_else(|| format!("{fallback}#{reference}"))
    }

    fn object_path(&self, reference: i32, fallback: &str) -> String {
        self.package
            .object_path(reference)
            .filter(|path| !path.is_empty())
            .unwrap_or_else(|| format!("{fallback}#{reference}"))
    }
}

enum Operator {
    Binary(&'static str, u8),
    Assign(&'static str),
    Prefix(&'static str),
    Postfix(&'static str),
}

fn operator_for(name: &str) -> Option<Operator> {
    // Increment/decrement forms must be checked before the plain operators.
    if name.starts_with("AddAdd_Pre") {
        return Some(Operator::Prefix("++"));
    }
    if name.starts_with("AddAdd_") {
        return Some(Operator::Postfix("++"));
    }
    if name.starts_with("SubtractSubtract_Pre") {
        return Some(Operator::Prefix("--"));
    }
    if name.starts_with("SubtractSubtract_") {
        return Some(Operator::Postfix("--"));
    }
    if name == "Not_PreBool" {
        return Some(Operator::Prefix("!"));
    }
    if name.starts_with("Complement_Pre") {
        return Some(Operator::Prefix("~"));
    }
    if name.starts_with("Subtract_Pre") {
        return Some(Operator::Prefix("-"));
    }

    if name.starts_with("AddEqual_") {
        return Some(Operator::Assign("+="));
    }
    if name.starts_with("SubtractEqual_") {
        return Some(Operator::Assign("-="));
    }
    if name.starts_with("MultiplyEqual_") {
        return Some(Operator::Assign("*="));
    }
    if name.starts_with("DivideEqual_") {
        return Some(Operator::Assign("/="));
    }
    if name.starts_with("PercentEqual_") || name.starts_with("ModEqual_") {
        return Some(Operator::Assign("%="));
    }
    if name.starts_with("AndAnd_") {
        return Some(Operator::Binary("&&", 2));
    }
    if name.starts_with("OrOr_") {
        return Some(Operator::Binary("||", 1));
    }
    if name.starts_with("XorXor_") {
        return Some(Operator::Binary("^^", 2));
    }
    if name.starts_with("EqualEqual_") {
        return Some(Operator::Binary("==", 6));
    }
    if name.starts_with("ComplementEqual_") || name.starts_with("NotEqual_") {
        return Some(Operator::Binary("!=", 6));
    }
    if name.starts_with("LessLess_") {
        return Some(Operator::Binary("<<", 8));
    }
    if name.starts_with("GreaterGreaterGreater_") {
        return Some(Operator::Binary(">>>", 8));
    }
    if name.starts_with("GreaterGreater_") {
        return Some(Operator::Binary(">>", 8));
    }
    if name.starts_with("LessEqual_") {
        return Some(Operator::Binary("<=", 7));
    }
    if name.starts_with("GreaterEqual_") {
        return Some(Operator::Binary(">=", 7));
    }
    if name.starts_with("Less_") {
        return Some(Operator::Binary("<", 7));
    }
    if name.starts_with("Greater_") {
        return Some(Operator::Binary(">", 7));
    }
    if name.starts_with("Add_") {
        return Some(Operator::Binary("+", 9));
    }
    if name.starts_with("Subtract_") {
        return Some(Operator::Binary("-", 9));
    }
    if name.starts_with("MultiplyMultiply_") {
        return Some(Operator::Binary("**", 11));
    }
    if name.starts_with("Multiply_") {
        return Some(Operator::Binary("*", 10));
    }
    if name.starts_with("Divide_") {
        return Some(Operator::Binary("/", 10));
    }
    if name.starts_with("Percent_") || name.starts_with("Mod_") {
        return Some(Operator::Binary("%", 10));
    }
    if name.starts_with("And_") {
        return Some(Operator::Binary("&", 5));
    }
    if name.starts_with("Or_") {
        return Some(Operator::Binary("|", 3));
    }
    if name.starts_with("Xor_") {
        return Some(Operator::Binary("^", 4));
    }
    if name == "Concat_StrStr" {
        return Some(Operator::Binary("$", 9));
    }
    if name == "At_StrStr" {
        return Some(Operator::Binary("@", 9));
    }

    None
}

/// Native functions are serialized with a body that only reads each
/// parameter (`NativeParm`) and nothing else. Such bodies carry no logic, so
/// the renderer emits a native declaration instead of the stub statements.
fn is_stub_body(body: &[Expr]) -> bool {
    !body.is_empty()
        && body
            .iter()
            .all(|expr| matches!(expr.kind, ExprKind::NativeParm(_) | ExprKind::Nothing))
}

fn is_assignable(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::BoolVariable(inner) => is_assignable(inner),
        ExprKind::LocalVariable(_)
        | ExprKind::InstanceVariable(_)
        | ExprKind::DefaultVariable(_)
        | ExprKind::NativeParm(_)
        | ExprKind::ArrayElement { .. }
        | ExprKind::DynArrayElement { .. }
        | ExprKind::StructMember { .. } => true,
        _ => false,
    }
}

fn format_float(value: f32) -> String {
    if !value.is_finite() {
        return value.to_string();
    }
    if value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        value.to_string()
    }
}

fn escape_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_common_native_operators() {
        assert!(matches!(
            operator_for("Add_IntInt"),
            Some(Operator::Binary("+", 9))
        ));
        assert!(matches!(
            operator_for("AddEqual_FloatFloat"),
            Some(Operator::Assign("+="))
        ));
        assert!(matches!(
            operator_for("AddAdd_PreInt"),
            Some(Operator::Prefix("++"))
        ));
        assert!(matches!(
            operator_for("AddAdd_Int"),
            Some(Operator::Postfix("++"))
        ));
        assert!(matches!(
            operator_for("GreaterGreater_IntInt"),
            Some(Operator::Binary(">>", 8))
        ));
        assert!(matches!(
            operator_for("EqualEqual_NameName"),
            Some(Operator::Binary("==", 6))
        ));
        assert!(matches!(
            operator_for("Not_PreBool"),
            Some(Operator::Prefix("!"))
        ));
    }

    #[test]
    fn formats_float_literals() {
        assert_eq!(format_float(1.0), "1.0");
        assert_eq!(format_float(0.5), "0.5");
        assert_eq!(format_float(-2.0), "-2.0");
    }

    #[test]
    fn escapes_string_literals() {
        assert_eq!(escape_string("a\"b\\c\n"), "a\\\"b\\\\c\\n");
    }
}
