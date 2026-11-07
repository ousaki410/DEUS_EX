//! deusex-reverse - Zero-execution reverse engineering tool for Deus Ex UE1 packages.
//!
//! Scans `.u` package files without running the game, extracts class/function/property
//! metadata, decompiles UnrealScript bytecode to expression trees, and generates
//! markdown documentation with dependency graphs.
//!
//! Usage:
//!   cargo run -p deusex-reverse -- <system_dir> [output_dir]
//!
//! Example:
//!   cargo run -p deusex-reverse -- goty/app/system ./reverse_report --all
//!
//! Output:
//!   - SUMMARY.md          : Overall summary across all packages
//!   - <package>.md        : Per-package detailed report
//!   - manifest.json       : JSON manifest (with --json)
//!   - report.csv          : CSV table of all functions across packages (with --csv)
//!   - function_graph.dot  : Function call graph (with --dot)
//!   - class_graph.dot     : Class inheritance graph (with --dot)
//!   - opcodes.csv         : Bytecode opcode frequency statistics (with --opcodes)
//!   - --all               : Emit every optional format above.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::Parser;
use deusex_bytecode::{parse_package_functions, Expr, ExprKind, Function, Token};
use deusex_package::{
    function_parameters, read_property_info, UPropertyInfo, UnrealPackage,
};
use serde::Serialize;

// ---------------------------------------------------------------------------
// CLI arguments
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Deus Ex Native - reverse engineering tool for UE1 packages"
)]
struct Args {
    /// Directory containing `.u` package files (e.g. goty/app/system).
    #[arg(value_name = "SYSTEM_DIR")]
    system_dir: PathBuf,

    /// Output directory for reports (default: ./reverse_report).
    #[arg(value_name = "OUTPUT_DIR", default_value = "./reverse_report")]
    output_dir: PathBuf,

    /// Write a JSON machine-readable manifest.
    #[arg(long)]
    json: bool,

    /// Write CSV tables.
    #[arg(long)]
    csv: bool,

    /// Write Graphviz DOT files (function call graph + class graph).
    #[arg(long)]
    dot: bool,

    /// Write opcode frequency statistics.
    #[arg(long)]
    opcodes: bool,

    /// Emit every optional output format (JSON, CSV, DOT, opcodes).
    #[arg(long)]
    all: bool,

    /// Only process packages matching this substring (e.g. "Core", "DeusEx").
    #[arg(long)]
    filter: Option<String>,
}

// ---------------------------------------------------------------------------
// Data models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
struct Report {
    package_name: String,
    file_path: String,
    file_version: u16,
    package_flags: u32,
    stats: PackageStats,
    classes: Vec<ClassInfo>,
    functions: Vec<FunctionInfo>,
    /// Opcode byte -> number of decoded expressions with that token.
    opcodes: BTreeMap<u8, u64>,
}

#[derive(Debug, Clone, Serialize)]
struct PackageStats {
    name_count: usize,
    import_count: usize,
    export_count: usize,
    function_count: usize,
    class_count: usize,
    property_count: usize,
    native_function_count: usize,
    total_script_bytes: usize,
    total_instruction_count: usize,
    total_branch_count: usize,
    total_opcode_count: u64,
}

#[derive(Debug, Clone, Serialize)]
struct ClassInfo {
    name: String,
    full_name: String,
    outer: Option<String>,
    super_class: Option<String>,
    class_flags: u32,
    is_native: bool,
    export_ref: i32,
    properties: Vec<PropertyInfo>,
}

#[derive(Debug, Clone, Serialize)]
struct PropertyInfo {
    name: String,
    type_name: String,
    resolved_type: Option<String>,
    array_dim: i32,
    flags: u32,
    replication_offset: Option<i16>,
}

#[derive(Debug, Clone, Serialize)]
struct ParameterInfo {
    name: String,
    type_name: String,
    resolved_type: Option<String>,
    flags: u32,
    array_dim: i32,
    is_out: bool,
    is_optional: bool,
    is_skip: bool,
    is_return: bool,
    is_coerce: bool,
}

#[derive(Debug, Clone, Serialize)]
struct FunctionInfo {
    name: String,
    full_name: String,
    class: Option<String>,
    state: Option<String>,
    package: String,
    native_index: i16,
    is_native: bool,
    is_latent: bool,
    is_event: bool,
    is_static: bool,
    precedence: u8,
    flags: u32,
    script_size: u32,
    script_raw_size: usize,
    signature: String,
    parameters: Vec<ParameterInfo>,
    instruction_count: usize,
    branch_count: usize,
    opcode_count: u64,
    native_calls: Vec<String>,
    external_calls: Vec<String>,
    referenced_names: Vec<String>,
    referenced_objects: Vec<String>,
}

// ---------------------------------------------------------------------------
// Analysis
// ---------------------------------------------------------------------------

fn analyze_package(path: &Path) -> Result<Report> {
    let package = UnrealPackage::open(path)?;
    let package_name = package_display_name(path);

    let mut functions = Vec::new();
    let mut opcodes: BTreeMap<u8, u64> = BTreeMap::new();

    for function in parse_package_functions(&package)? {
        let (info, function_opcodes) = analyze_function(&package, &function, &package_name);
        for (opcode, count) in function_opcodes {
            *opcodes.entry(opcode).or_insert(0) += count;
        }
        functions.push(info);
    }

    functions.sort_by(|a, b| {
        a.full_name
            .cmp(&b.full_name)
            .then_with(|| a.name.cmp(&b.name))
    });

    let classes = collect_classes(&package, &package_name);
    let total_opcode_count = opcodes.values().sum();

    let stats = PackageStats {
        name_count: package.names().len(),
        import_count: package.imports().len(),
        export_count: package.exports().len(),
        function_count: functions.len(),
        class_count: classes.len(),
        property_count: classes.iter().map(|class| class.properties.len()).sum(),
        native_function_count: functions.iter().filter(|f| f.is_native).count(),
        total_script_bytes: functions.iter().map(|f| f.script_raw_size).sum(),
        total_instruction_count: functions.iter().map(|f| f.instruction_count).sum(),
        total_branch_count: functions.iter().map(|f| f.branch_count).sum(),
        total_opcode_count,
    };

    Ok(Report {
        package_name,
        file_path: package
            .file_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| path.display().to_string()),
        file_version: package.summary().file_version,
        package_flags: package.summary().package_flags,
        stats,
        classes,
        functions,
        opcodes,
    })
}

fn analyze_function(
    package: &UnrealPackage,
    function: &Function,
    package_name: &str,
) -> (FunctionInfo, BTreeMap<u8, u64>) {
    let mut scan = ExprScan::new(package, package_name);
    scan.visit_all(&function.body);

    let (class, state) = resolve_function_owner(package, function);
    let full_name = match (&class, &state) {
        (Some(class), Some(state)) => format!("{package_name}.{class}.{state}.{}", function.name),
        (Some(class), None) => format!("{package_name}.{class}.{}", function.name),
        _ => format!("{package_name}.{}", function.name),
    };
    let is_static = (function.function_flags & deusex_bytecode::function_flags::STATIC) != 0;

    let parameters: Vec<ParameterInfo> =
        function_parameters(package, function.children_ref)
            .map(|parameters| {
                parameters
                    .into_iter()
                    .map(|info| ParameterInfo::from_property(package, info))
                    .collect()
            })
            .unwrap_or_default();
    let signature = render_signature(&function.name, is_static, &parameters);

    let info = FunctionInfo {
        name: function.name.clone(),
        full_name,
        class,
        state,
        package: package_name.to_string(),
        native_index: function.native_index,
        is_native: function.is_native(),
        is_latent: (function.function_flags & deusex_bytecode::function_flags::LATENT) != 0,
        is_event: (function.function_flags & deusex_bytecode::function_flags::EVENT) != 0,
        is_static,
        precedence: function.precedence,
        flags: function.function_flags,
        script_size: function.script_size,
        script_raw_size: function.script_raw_size,
        signature,
        parameters,
        instruction_count: scan.node_count,
        branch_count: scan.branch_count,
        opcode_count: scan.opcodes.values().sum(),
        native_calls: scan.native_calls.into_iter().collect(),
        external_calls: scan.external_calls.into_iter().collect(),
        referenced_names: scan.referenced_names.into_iter().collect(),
        referenced_objects: scan.referenced_objects.into_iter().collect(),
    };

    (info, scan.opcodes)
}

fn collect_classes(package: &UnrealPackage, package_name: &str) -> Vec<ClassInfo> {
    // Group UProperty exports by their owning UClass. Function parameters also
    // export as UProperty objects, but their outer is a UFunction, so we only
    // accept owners whose own class is `Class`.
    let mut properties_by_class: BTreeMap<i32, Vec<PropertyInfo>> = BTreeMap::new();
    for (reference, export) in package.exports().iter() {
        let class_name = package.class_name(reference).unwrap_or("");
        if !class_name.ends_with("Property") || export.outer <= 0 {
            continue;
        }
        let owner_is_class = package
            .class_name(export.outer)
            .map(|owner_class| owner_class.eq_ignore_ascii_case("class"))
            .unwrap_or(false);
        if !owner_is_class {
            continue;
        }
        let Ok(info) = read_property_info(package, reference) else {
            continue;
        };
        properties_by_class
            .entry(export.outer)
            .or_default()
            .push(PropertyInfo::from_property(package, info));
    }
    for properties in properties_by_class.values_mut() {
        properties.sort_by(|a, b| a.name.cmp(&b.name));
    }

    let mut classes = Vec::new();
    for (export_ref, export) in package.exports().iter() {
        // Only include actual UClass exports, not every object in the package.
        let declared_class = package.class_name(export_ref).unwrap_or("");
        if !declared_class.eq_ignore_ascii_case("class") {
            continue;
        }

        let name = package.object_name(export_ref).unwrap_or("").to_string();
        let full_name = format!(
            "{package_name}.{}",
            package
                .object_path(export_ref)
                .unwrap_or_else(|| name.clone())
        );
        let outer = if export.outer != 0 {
            reference_path(package, export.outer, package_name)
        } else {
            None
        };
        // UE1 stores the superclass of a UClass in FObjectExport::super_ref.
        let super_class = if export.super_ref != 0 {
            reference_path(package, export.super_ref, package_name)
        } else {
            None
        };
        let properties = properties_by_class.remove(&export_ref).unwrap_or_default();

        classes.push(ClassInfo {
            name,
            full_name,
            outer,
            super_class,
            class_flags: export.object_flags,
            is_native: (export.object_flags & deusex_package::RF_NATIVE) != 0,
            export_ref,
            properties,
        });
    }

    classes.sort_by(|a, b| a.full_name.cmp(&b.full_name));
    classes
}

/// Resolve a function's owning class and, when it is declared in a state, the
/// state name. `Function::outer_name` alone is ambiguous because functions
/// nested in a state have the state as their immediate outer object.
fn resolve_function_owner(
    package: &UnrealPackage,
    function: &Function,
) -> (Option<String>, Option<String>) {
    if function.outer_ref == 0 {
        return (None, None);
    }

    let outer_class = package.class_name(function.outer_ref).unwrap_or("");
    let outer_name = package
        .object_name(function.outer_ref)
        .unwrap_or(&function.outer_name)
        .to_string();

    if outer_class.eq_ignore_ascii_case("class") {
        return (Some(outer_name), None);
    }

    if outer_class.eq_ignore_ascii_case("state") {
        let class_name = package
            .outer_ref(function.outer_ref)
            .and_then(|owner_ref| package.object_name(owner_ref))
            .map(|name| name.to_string());
        return (class_name.or_else(|| Some(function.outer_name.clone())), Some(outer_name));
    }

    // Unknown outer kind: keep the outer name as the class name.
    (Some(outer_name), None)
}

impl PropertyInfo {
    fn from_property(package: &UnrealPackage, info: UPropertyInfo) -> Self {
        let resolved_type = if info.type_ref != 0 {
            package.object_path(info.type_ref)
        } else {
            None
        };
        Self {
            name: info.name,
            type_name: info.class_name,
            resolved_type,
            array_dim: info.array_dim,
            flags: info.flags,
            replication_offset: info.replication_offset,
        }
    }

    fn display_type(&self) -> String {
        type_label(
            &self.type_name,
            self.resolved_type.as_deref().map(short_object_name),
        )
    }
}

impl ParameterInfo {
    fn from_property(package: &UnrealPackage, info: UPropertyInfo) -> Self {
        let resolved_type = if info.type_ref != 0 {
            package.object_path(info.type_ref)
        } else {
            None
        };
        let is_out = info.is_out_parm();
        let is_optional = (info.flags & 0x0000_0010) != 0; // CPF_OptionalParm
        let is_skip = (info.flags & deusex_package::property_flags::SKIP_PARM) != 0;
        let is_return = info.is_return_parm();
        let is_coerce = info.is_coerce_parm();
        Self {
            name: info.name,
            type_name: info.class_name,
            resolved_type,
            flags: info.flags,
            array_dim: info.array_dim,
            is_out,
            is_optional,
            is_skip,
            is_return,
            is_coerce,
        }
    }

    fn display_type(&self) -> String {
        type_label(
            &self.type_name,
            self.resolved_type.as_deref().map(short_object_name),
        )
    }
}

fn type_label(type_name: &str, resolved_type: Option<&str>) -> String {
    let resolved = resolved_type.unwrap_or("");
    match type_name {
        "ByteProperty" if !resolved.is_empty() => resolved.to_string(),
        "ByteProperty" => "byte".to_string(),
        "IntProperty" => "int".to_string(),
        "BoolProperty" => "bool".to_string(),
        "FloatProperty" => "float".to_string(),
        "StrProperty" => "string".to_string(),
        "NameProperty" => "name".to_string(),
        "ObjectProperty" if !resolved.is_empty() => resolved.to_string(),
        "ObjectProperty" => "object".to_string(),
        "ClassProperty" if !resolved.is_empty() => format!("class<{resolved}>"),
        "ClassProperty" => "class".to_string(),
        "StructProperty" if !resolved.is_empty() => resolved.to_string(),
        "StructProperty" => "struct".to_string(),
        "ArrayProperty" if resolved.is_empty() => "array<?>".to_string(),
        "ArrayProperty" => format!("array<{resolved}>"),
        "FixedArrayProperty" if resolved.is_empty() => "fixed array".to_string(),
        "FixedArrayProperty" => format!("array<{resolved}>"),
        "PointerProperty" => "pointer".to_string(),
        other => other.trim_end_matches("Property").to_string(),
    }
}

fn short_object_name(path: &str) -> &str {
    path.rsplit('.').next().unwrap_or(path)
}

fn render_signature(name: &str, is_static: bool, parameters: &[ParameterInfo]) -> String {
    let return_type = parameters
        .iter()
        .find(|parameter| parameter.is_return)
        .map(ParameterInfo::display_type)
        .unwrap_or_else(|| "void".to_string());

    let inputs = parameters
        .iter()
        .filter(|parameter| !parameter.is_return && !parameter.is_skip)
        .map(|parameter| {
            let mut text = String::new();
            if parameter.is_out {
                text.push_str("out ");
            }
            text.push_str(&parameter.name);
            text.push_str(": ");
            text.push_str(&parameter.display_type());
            if parameter.is_optional {
                text.push('?');
            }
            text
        })
        .collect::<Vec<_>>()
        .join(", ");

    let static_keyword = if is_static { "static " } else { "" };
    format!("{static_keyword}function {return_type} {name}({inputs})")
}

/// Resolve an object reference to a package-qualified path.
///
/// Export references belong to the current package; import references walk
/// their outer chain to the top-level package import.
fn reference_path(
    package: &UnrealPackage,
    reference: i32,
    current_package: &str,
) -> Option<String> {
    if reference == 0 {
        return None;
    }

    let path = package.object_path(reference)?;
    let package_name = if reference > 0 {
        Some(current_package.to_string())
    } else {
        import_package_name(package, reference)
    };

    match package_name {
        Some(package_name)
            if !path.starts_with(&format!("{package_name}.")) =>
        {
            Some(format!("{package_name}.{path}"))
        }
        _ => Some(path),
    }
}

fn import_package_name(package: &UnrealPackage, reference: i32) -> Option<String> {
    let mut current = reference;
    let mut top_name = None;

    for _ in 0..64 {
        if current >= 0 {
            break;
        }
        let import = package.imports().get(current)?;
        top_name = package
            .name(import.object_name)
            .map(|name| name.to_string());
        if import.outer == 0 {
            break;
        }
        current = import.outer;
    }

    top_name
}

/// Recursive expression-tree scanner collecting metadata for one function.
struct ExprScan<'a> {
    package: &'a UnrealPackage,
    package_name: &'a str,
    native_calls: BTreeSet<String>,
    external_calls: BTreeSet<String>,
    referenced_names: BTreeSet<String>,
    referenced_objects: BTreeSet<String>,
    opcodes: BTreeMap<u8, u64>,
    node_count: usize,
    branch_count: usize,
}

impl<'a> ExprScan<'a> {
    fn new(package: &'a UnrealPackage, package_name: &'a str) -> Self {
        Self {
            package,
            package_name,
            native_calls: BTreeSet::new(),
            external_calls: BTreeSet::new(),
            referenced_names: BTreeSet::new(),
            referenced_objects: BTreeSet::new(),
            opcodes: BTreeMap::new(),
            node_count: 0,
            branch_count: 0,
        }
    }

    fn visit_all(&mut self, body: &[Expr]) {
        for expr in body {
            self.visit(expr);
        }
    }

    fn visit(&mut self, expr: &Expr) {
        self.node_count += 1;
        if let Some(token) = expr.token() {
            *self.opcodes.entry(token.value()).or_insert(0) += 1;
        }

        match &expr.kind {
            ExprKind::Native { index, .. } => {
                self.native_calls.insert(format!("native#{index}"));
            }
            ExprKind::ExtendedNative { index, .. } => {
                self.native_calls.insert(format!("native#{index}"));
            }
            ExprKind::FinalFunction { function, .. } => {
                if let Some(path) =
                    reference_path(self.package, *function, self.package_name)
                {
                    self.external_calls.insert(path);
                }
            }
            ExprKind::VirtualFunction { name, .. } => {
                if let Some(name) = self.package.name(*name) {
                    let name = name.to_string();
                    self.external_calls.insert(name.clone());
                    self.referenced_names.insert(name);
                }
            }
            ExprKind::GlobalFunction { name, .. } => {
                if let Some(name) = self.package.name(*name) {
                    let name = name.to_string();
                    self.external_calls.insert(format!("global:{name}"));
                    self.referenced_names.insert(name);
                }
            }
            ExprKind::NameConst(index) => {
                if let Some(name) = self.package.name(*index) {
                    self.referenced_names.insert(name.to_string());
                }
            }
            ExprKind::ObjectConst(reference) => {
                if let Some(path) =
                    reference_path(self.package, *reference, self.package_name)
                {
                    self.referenced_objects.insert(path);
                }
            }
            ExprKind::MetaCast { class, .. } | ExprKind::DynamicCast { class, .. } => {
                if let Some(path) =
                    reference_path(self.package, *class, self.package_name)
                {
                    self.referenced_objects.insert(path);
                }
            }
            ExprKind::Jump(_)
            | ExprKind::JumpIfNot { .. }
            | ExprKind::Case { .. }
            | ExprKind::Assert { .. } => {
                self.branch_count += 1;
            }
            _ => {}
        }

        for child in expr.children() {
            self.visit(child);
        }
    }
}

fn package_display_name(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| "package".to_string())
}

// ---------------------------------------------------------------------------
// Markdown renderers
// ---------------------------------------------------------------------------

fn render_report(report: &Report) -> String {
    let mut out = String::new();

    writeln!(out, "# {}\n", report.package_name).unwrap();
    writeln!(out, "Source: `{}`\n", report.file_path).unwrap();

    out.push_str("## Summary\n\n");
    out.push_str("| Metric | Value |\n");
    out.push_str("|--------|-------|\n");
    writeln!(out, "| File version | {} |", report.file_version).unwrap();
    writeln!(out, "| Package flags | 0x{:08X} |", report.package_flags).unwrap();
    writeln!(out, "| Names | {} |", report.stats.name_count).unwrap();
    writeln!(out, "| Imports | {} |", report.stats.import_count).unwrap();
    writeln!(out, "| Exports | {} |", report.stats.export_count).unwrap();
    writeln!(out, "| Classes | {} |", report.stats.class_count).unwrap();
    writeln!(out, "| Properties | {} |", report.stats.property_count).unwrap();
    writeln!(out, "| Functions | {} |", report.stats.function_count).unwrap();
    writeln!(
        out,
        "| Native functions | {} |",
        report.stats.native_function_count
    )
    .unwrap();
    writeln!(
        out,
        "| Script bytes | {} |",
        report.stats.total_script_bytes
    )
    .unwrap();
    writeln!(
        out,
        "| Expression nodes | {} |",
        report.stats.total_instruction_count
    )
    .unwrap();
    writeln!(
        out,
        "| Branch instructions | {} |",
        report.stats.total_branch_count
    )
    .unwrap();
    writeln!(
        out,
        "| Opcode tokens | {} |",
        report.stats.total_opcode_count
    )
    .unwrap();
    out.push('\n');

    if !report.classes.is_empty() {
        writeln!(out, "## Classes ({})\n", report.classes.len()).unwrap();
        out.push_str("| Class | Super | Outer | Native | Properties | Flags |\n");
        out.push_str("|-------|-------|-------|--------|-----------:|-------|\n");
        for class in &report.classes {
            writeln!(
                out,
                "| {} | {} | {} | {} | {} | 0x{:08X} |",
                class.name,
                class.super_class.as_deref().unwrap_or("-"),
                class.outer.as_deref().unwrap_or("-"),
                yes_no(class.is_native),
                class.properties.len(),
                class.class_flags
            )
            .unwrap();
        }
        out.push('\n');

        if report.classes.iter().any(|class| !class.properties.is_empty()) {
            out.push_str("### Class Properties\n\n");
            for class in &report.classes {
                if class.properties.is_empty() {
                    continue;
                }
                writeln!(out, "#### {}\n", class.full_name).unwrap();
                out.push_str("| Property | Type | Array | Flags | Replication |\n");
                out.push_str("|----------|------|------:|-------|------------:|\n");
                for property in &class.properties {
                    writeln!(
                        out,
                        "| {} | {} | {} | 0x{:08X} | {} |",
                        property.name,
                        property.display_type(),
                        property.array_dim,
                        property.flags,
                        property
                            .replication_offset
                            .map(|offset| offset.to_string())
                            .unwrap_or_else(|| "-".to_string())
                    )
                    .unwrap();
                }
                out.push('\n');
            }
        }
    }

    if !report.functions.is_empty() {
        writeln!(out, "## Functions ({})\n", report.functions.len()).unwrap();
        out.push_str(
            "| Function | Class | State | Signature | Native | Latent | Event | Script | Nodes | Branches |\n",
        );
        out.push_str(
            "|----------|-------|-------|-----------|--------|--------|-------|--------|-------|----------|\n",
        );
        for function in &report.functions {
            writeln!(
                out,
                "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                function.name,
                function.class.as_deref().unwrap_or("?"),
                function.state.as_deref().unwrap_or("-"),
                function.signature,
                yes_no(function.is_native),
                yes_no(function.is_latent),
                yes_no(function.is_event),
                function.script_size,
                function.instruction_count,
                function.branch_count
            )
            .unwrap();
        }
        out.push('\n');

        if let Some(top) = render_top_opcodes(&report.opcodes, 15) {
            out.push_str("## Opcode Usage (top 15)\n\n");
            out.push_str(&top);
            out.push('\n');
        }

        out.push_str("## Function Details\n\n");
        for function in &report.functions {
            writeln!(out, "### {}\n", function.full_name).unwrap();
            out.push_str("| Property | Value |\n");
            out.push_str("|----------|-------|\n");
            writeln!(
                out,
                "| Class | {} |",
                function.class.as_deref().unwrap_or("?")
            )
            .unwrap();
            writeln!(out, "| Package | {} |", function.package).unwrap();
            if let Some(state) = &function.state {
                writeln!(out, "| State | {state} |").unwrap();
            }
            writeln!(out, "| Signature | `{}` |", function.signature).unwrap();
            writeln!(out, "| Native index | {} |", function.native_index).unwrap();
            writeln!(out, "| Native | {} |", yes_no(function.is_native)).unwrap();
            writeln!(out, "| Latent | {} |", yes_no(function.is_latent)).unwrap();
            writeln!(out, "| Event | {} |", yes_no(function.is_event)).unwrap();
            writeln!(out, "| Static | {} |", yes_no(function.is_static)).unwrap();
            writeln!(out, "| Precedence | {} |", function.precedence).unwrap();
            writeln!(out, "| Flags | 0x{:08X} |", function.flags).unwrap();
            writeln!(out, "| Script size | {} bytes |", function.script_size).unwrap();
            writeln!(
                out,
                "| Raw script size | {} bytes |",
                function.script_raw_size
            )
            .unwrap();
            writeln!(
                out,
                "| Expression nodes | {} |",
                function.instruction_count
            )
            .unwrap();
            writeln!(out, "| Branch count | {} |", function.branch_count).unwrap();
            writeln!(out, "| Opcode tokens | {} |", function.opcode_count).unwrap();

            if !function.native_calls.is_empty() {
                writeln!(
                    out,
                    "| Native calls | {} |",
                    join_limited(&function.native_calls, 40)
                )
                .unwrap();
            }
            if !function.external_calls.is_empty() {
                writeln!(
                    out,
                    "| Call targets | {} |",
                    join_limited(&function.external_calls, 40)
                )
                .unwrap();
            }
            if !function.referenced_names.is_empty() {
                writeln!(
                    out,
                    "| Referenced names | {} |",
                    join_limited(&function.referenced_names, 40)
                )
                .unwrap();
            }
            if !function.referenced_objects.is_empty() {
                writeln!(
                    out,
                    "| Referenced objects | {} |",
                    join_limited(&function.referenced_objects, 40)
                )
                .unwrap();
            }
            out.push('\n');

            let visible_parameters: Vec<&ParameterInfo> = function
                .parameters
                .iter()
                .filter(|parameter| !parameter.is_skip)
                .collect();
            if !visible_parameters.is_empty() {
                out.push_str("| Parameter | Type | Out | Optional | Return | Coerce | Flags |\n");
                out.push_str("|-----------|------|-----|----------|--------|--------|-------|\n");
                for parameter in visible_parameters {
                    writeln!(
                        out,
                        "| {} | {} | {} | {} | {} | {} | 0x{:08X} |",
                        parameter.name,
                        parameter.display_type(),
                        yes_no(parameter.is_out),
                        yes_no(parameter.is_optional),
                        yes_no(parameter.is_return),
                        yes_no(parameter.is_coerce),
                        parameter.flags
                    )
                    .unwrap();
                }
                out.push('\n');
            }
        }
    }

    out
}

fn render_top_opcodes(opcodes: &BTreeMap<u8, u64>, limit: usize) -> Option<String> {
    if opcodes.is_empty() {
        return None;
    }

    let total: u64 = opcodes.values().sum();
    let mut entries: Vec<(&u8, &u64)> = opcodes.iter().collect();
    entries.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));

    let mut out = String::new();
    out.push_str("| Opcode | Name | Count | Share |\n");
    out.push_str("|--------|------|------:|------:|\n");
    for (opcode, count) in entries.into_iter().take(limit) {
        let share = if total > 0 {
            *count as f64 * 100.0 / total as f64
        } else {
            0.0
        };
        writeln!(
            out,
            "| 0x{:02X} | {} | {} | {:.2}% |",
            opcode,
            Token::new(*opcode).name(),
            count,
            share
        )
        .unwrap();
    }
    Some(out)
}

fn render_summary(reports: &[Report]) -> String {
    let mut out = String::new();

    out.push_str("# Deus Ex Native - Reverse Engineering Report\n\n");
    writeln!(
        out,
        "Generated from {} package(s).\n",
        reports.len()
    )
    .unwrap();

    let total_classes: usize = reports.iter().map(|r| r.classes.len()).sum();
    let total_properties: usize = reports.iter().map(|r| r.stats.property_count).sum();
    let total_functions: usize = reports.iter().map(|r| r.functions.len()).sum();
    let total_native: usize = reports
        .iter()
        .map(|r| r.stats.native_function_count)
        .sum();
    let total_script_bytes: usize = reports.iter().map(|r| r.stats.total_script_bytes).sum();
    let total_instructions: usize = reports
        .iter()
        .map(|r| r.stats.total_instruction_count)
        .sum();
    let total_branches: usize = reports.iter().map(|r| r.stats.total_branch_count).sum();
    let total_opcodes: u64 = reports.iter().map(|r| r.stats.total_opcode_count).sum();

    out.push_str("## Totals\n\n");
    out.push_str("| Metric | Value |\n");
    out.push_str("|--------|-------|\n");
    writeln!(out, "| Total packages | {} |", reports.len()).unwrap();
    writeln!(out, "| Total classes | {total_classes} |").unwrap();
    writeln!(out, "| Total properties | {total_properties} |").unwrap();
    writeln!(out, "| Total functions | {total_functions} |").unwrap();
    writeln!(out, "| Total native functions | {total_native} |").unwrap();
    writeln!(out, "| Total script bytes | {total_script_bytes} |").unwrap();
    writeln!(out, "| Total expression nodes | {total_instructions} |").unwrap();
    writeln!(out, "| Total branches | {total_branches} |").unwrap();
    writeln!(out, "| Total opcode tokens | {total_opcodes} |").unwrap();
    out.push('\n');

    out.push_str("## Per-Package Summary\n\n");
    out.push_str(
        "| Package | Version | Classes | Properties | Functions | Native | Script bytes | Nodes | Opcodes |\n",
    );
    out.push_str(
        "|---------|--------:|--------:|-----------:|----------:|-------:|-------------:|------:|--------:|\n",
    );
    for report in reports {
        writeln!(
            out,
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            report.package_name,
            report.file_version,
            report.classes.len(),
            report.stats.property_count,
            report.functions.len(),
            report.stats.native_function_count,
            report.stats.total_script_bytes,
            report.stats.total_instruction_count,
            report.stats.total_opcode_count
        )
        .unwrap();
    }
    out.push('\n');

    if !reports.is_empty() {
        out.push_str("## All Functions\n\n");
        out.push_str(
            "| Package | Function | Class | Native | Latent | Event | Script bytes | Nodes |\n",
        );
        out.push_str(
            "|---------|----------|-------|--------|--------|-------|-------------:|------:|\n",
        );
        for report in reports {
            for function in &report.functions {
                writeln!(
                    out,
                    "| {} | {} | {} | {} | {} | {} | {} | {} |",
                    report.package_name,
                    function.name,
                    function.class.as_deref().unwrap_or("?"),
                    yes_no(function.is_native),
                    yes_no(function.is_latent),
                    yes_no(function.is_event),
                    function.script_size,
                    function.instruction_count
                )
                .unwrap();
            }
        }
        out.push('\n');
    }

    out
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "Y"
    } else {
        "N"
    }
}

fn join_limited(values: &[String], limit: usize) -> String {
    if values.is_empty() {
        return "-".to_string();
    }
    let mut joined = values
        .iter()
        .take(limit)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if values.len() > limit {
        write!(joined, " ... (+{} more)", values.len() - limit).unwrap();
    }
    joined
}

// ---------------------------------------------------------------------------
// CSV export
// ---------------------------------------------------------------------------

fn render_csv(reports: &[Report]) -> String {
    const HEADER: &str = "package,function,class,state,signature,native,latent,event,static,precedence,flags,script_size,raw_script_size,instruction_count,branch_count,opcode_count,native_calls,external_calls,referenced_names,referenced_objects";
    let mut out = String::new();
    out.push_str(HEADER);
    out.push('\n');

    for report in reports {
        for function in &report.functions {
            let row = [
                csv_escape(&report.package_name),
                csv_escape(&function.name),
                csv_escape(function.class.as_deref().unwrap_or("")),
                csv_escape(function.state.as_deref().unwrap_or("")),
                csv_escape(&function.signature),
                function.is_native.to_string(),
                function.is_latent.to_string(),
                function.is_event.to_string(),
                function.is_static.to_string(),
                function.precedence.to_string(),
                format!("0x{:08X}", function.flags),
                function.script_size.to_string(),
                function.script_raw_size.to_string(),
                function.instruction_count.to_string(),
                function.branch_count.to_string(),
                function.opcode_count.to_string(),
                function.native_calls.len().to_string(),
                function.external_calls.len().to_string(),
                function.referenced_names.len().to_string(),
                function.referenced_objects.len().to_string(),
            ];
            out.push_str(&row.join(","));
            out.push('\n');
        }
    }

    out
}

fn render_classes_csv(reports: &[Report]) -> String {
    let mut out = String::from("package,class,super,outer,native,flags,property_count\n");
    for report in reports {
        for class in &report.classes {
            let row = [
                csv_escape(&report.package_name),
                csv_escape(&class.full_name),
                csv_escape(class.super_class.as_deref().unwrap_or("")),
                csv_escape(class.outer.as_deref().unwrap_or("")),
                class.is_native.to_string(),
                format!("0x{:08X}", class.class_flags),
                class.properties.len().to_string(),
            ];
            out.push_str(&row.join(","));
            out.push('\n');
        }
    }
    out
}

fn render_properties_csv(reports: &[Report]) -> String {
    let mut out =
        String::from("package,class,property,type,array_dim,flags,replication_offset\n");
    for report in reports {
        for class in &report.classes {
            for property in &class.properties {
                let row = [
                    csv_escape(&report.package_name),
                    csv_escape(&class.full_name),
                    csv_escape(&property.name),
                    csv_escape(&property.display_type()),
                    property.array_dim.to_string(),
                    format!("0x{:08X}", property.flags),
                    property
                        .replication_offset
                        .map(|offset| offset.to_string())
                        .unwrap_or_default(),
                ];
                out.push_str(&row.join(","));
                out.push('\n');
            }
        }
    }
    out
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

// ---------------------------------------------------------------------------
// Graphviz DOT export
// ---------------------------------------------------------------------------

fn render_dot_function_graph(reports: &[Report]) -> String {
    let mut out = String::new();
    out.push_str("digraph FunctionCalls {\n");
    out.push_str("  rankdir=LR;\n");
    out.push_str("  node [shape=box, fontsize=10];\n");
    out.push_str("  edge [fontsize=9];\n");

    let mut edges: BTreeSet<(String, String)> = BTreeSet::new();

    for report in reports {
        let cluster_id = dot_id("cluster", &report.package_name);
        writeln!(out, "  subgraph \"{cluster_id}\" {{").unwrap();
        writeln!(
            out,
            "    label=\"{}\";",
            dot_escape(&report.package_name)
        )
        .unwrap();

        for function in &report.functions {
            let source_id = dot_id("fn", &function.full_name);
            writeln!(
                out,
                "    \"{source_id}\" [label=\"{}\"];",
                dot_escape(&function.display_label())
            )
            .unwrap();

            for target in &function.external_calls {
                let target_id = dot_id("fn", target);
                if edges.insert((source_id.clone(), target_id.clone())) {
                    writeln!(out, "    \"{source_id}\" -> \"{target_id}\";").unwrap();
                }
            }
        }

        out.push_str("  }\n");
    }

    out.push_str("}\n");
    out
}

fn render_dot_class_graph(reports: &[Report]) -> String {
    let mut out = String::new();
    out.push_str("digraph ClassHierarchy {\n");
    out.push_str("  rankdir=TB;\n");
    out.push_str("  node [shape=box, fontsize=10];\n");
    out.push_str("  edge [fontsize=9];\n");

    let mut edges: BTreeSet<(String, String)> = BTreeSet::new();

    for report in reports {
        let cluster_id = dot_id("cluster", &report.package_name);
        writeln!(out, "  subgraph \"{cluster_id}\" {{").unwrap();
        writeln!(
            out,
            "    label=\"{}\";",
            dot_escape(&report.package_name)
        )
        .unwrap();

        for class in &report.classes {
            let class_id = dot_id("cls", &class.full_name);
            writeln!(
                out,
                "    \"{class_id}\" [label=\"{}\", tooltip=\"{}\"];",
                dot_escape(&class.name),
                dot_escape(&class.full_name)
            )
            .unwrap();

            if let Some(super_class) = &class.super_class {
                let super_id = dot_id("cls", super_class);
                if edges.insert((class_id.clone(), super_id.clone())) {
                    writeln!(
                        out,
                        "    \"{class_id}\" -> \"{super_id}\" [label=\"extends\"];"
                    )
                    .unwrap();
                }
            }
        }

        out.push_str("  }\n");
    }

    out.push_str("}\n");
    out
}

fn dot_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "")
}

fn dot_id(prefix: &str, value: &str) -> String {
    let mut id = String::with_capacity(prefix.len() + 1 + value.len());
    id.push_str(prefix);
    id.push('_');
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            id.push(ch);
        } else {
            id.push('_');
        }
    }
    id
}

impl FunctionInfo {
    fn display_label(&self) -> String {
        let mut parts = Vec::new();
        if let Some(class) = &self.class {
            parts.push(class.as_str());
        }
        if let Some(state) = &self.state {
            parts.push(state.as_str());
        }
        parts.push(self.name.as_str());
        parts.join(".")
    }
}

// ---------------------------------------------------------------------------
// Opcode statistics
// ---------------------------------------------------------------------------

fn render_opcode_csv(opcodes: &BTreeMap<u8, u64>) -> String {
    let total: u64 = opcodes.values().sum();
    let mut out = String::new();
    out.push_str("opcode,hex,name,count,percentage\n");

    for (opcode, count) in opcodes {
        let share = if total > 0 {
            *count as f64 * 100.0 / total as f64
        } else {
            0.0
        };
        writeln!(
            out,
            "{},0x{:02X},{},{},{:.2}",
            opcode,
            opcode,
            Token::new(*opcode).name(),
            count,
            share
        )
        .unwrap();
    }

    writeln!(out, "TOTAL,,,{},100.00", total).unwrap();
    out
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = Args::parse();

    if !args.system_dir.is_dir() {
        bail!(
            "system directory not found: {}",
            args.system_dir.display()
        );
    }

    fs::create_dir_all(&args.output_dir).with_context(|| {
        format!(
            "failed to create output directory: {}",
            args.output_dir.display()
        )
    })?;

    let emit_json = args.json || args.all;
    let emit_csv = args.csv || args.all;
    let emit_dot = args.dot || args.all;
    let emit_opcodes = args.opcodes || args.all;

    let mut package_paths = collect_package_paths(&args)?;
    package_paths.sort();
    println!(
        "Found {} package(s) in {}",
        package_paths.len(),
        args.system_dir.display()
    );

    let mut reports: Vec<Report> = Vec::with_capacity(package_paths.len());
    let mut failures = 0usize;

    for package_path in &package_paths {
        match analyze_package(package_path) {
            Ok(report) => {
                println!(
                    "  ok   {:<24} {:>4} classes {:>5} functions {:>7} opcodes",
                    report.package_name,
                    report.classes.len(),
                    report.functions.len(),
                    report.stats.total_opcode_count
                );
                reports.push(report);
            }
            Err(error) => {
                failures += 1;
                eprintln!(
                    "  FAIL {}: {error:#}",
                    package_path.display()
                );
            }
        }
    }

    write_reports(&args, &reports)?;

    let global_opcodes = aggregate_opcodes(&reports);
    if emit_json {
        write_json(&args, &reports)?;
    }
    if emit_csv {
        write_csv(&args, &reports)?;
    }
    if emit_dot {
        write_dot(&args, &reports)?;
    }
    if emit_opcodes {
        write_opcodes(&args, &global_opcodes)?;
    }

    println!(
        "Wrote {} package report(s) and SUMMARY.md to {}",
        reports.len(),
        args.output_dir.display()
    );
    if failures > 0 {
        eprintln!("{failures} package(s) failed to parse");
    }

    Ok(())
}

fn collect_package_paths(args: &Args) -> Result<Vec<PathBuf>> {
    let filter = args
        .filter
        .as_ref()
        .map(|filter| filter.to_ascii_lowercase());
    let mut package_paths = Vec::new();

    for entry in walkdir::WalkDir::new(&args.system_dir) {
        let entry = entry.with_context(|| {
            format!("failed to walk {}", args.system_dir.display())
        })?;
        if !entry.file_type().is_file() {
            continue;
        }
        let is_package = entry
            .path()
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.eq_ignore_ascii_case("u"))
            .unwrap_or(false);
        if !is_package {
            continue;
        }
        if let Some(filter) = &filter {
            if !entry
                .path()
                .to_string_lossy()
                .to_ascii_lowercase()
                .contains(filter)
            {
                continue;
            }
        }
        package_paths.push(entry.path().to_path_buf());
    }

    Ok(package_paths)
}

fn write_reports(args: &Args, reports: &[Report]) -> Result<()> {
    for report in reports {
        let file_name = format!("{}.md", safe_file_name(&report.package_name));
        let path = args.output_dir.join(file_name);
        fs::write(&path, render_report(report))
            .with_context(|| format!("failed to write {}", path.display()))?;
    }

    let summary_path = args.output_dir.join("SUMMARY.md");
    fs::write(&summary_path, render_summary(reports))
        .with_context(|| format!("failed to write {}", summary_path.display()))?;

    Ok(())
}

fn write_json(args: &Args, reports: &[Report]) -> Result<()> {
    let path = args.output_dir.join("manifest.json");
    let json = serde_json::to_string_pretty(reports)
        .context("failed to serialize package manifest")?;
    fs::write(&path, json)
        .with_context(|| format!("failed to write {}", path.display()))?;
    println!("Wrote {}", path.display());
    Ok(())
}

fn write_csv(args: &Args, reports: &[Report]) -> Result<()> {
    let functions_path = args.output_dir.join("report.csv");
    fs::write(&functions_path, render_csv(reports))
        .with_context(|| format!("failed to write {}", functions_path.display()))?;
    println!("Wrote {}", functions_path.display());

    let classes_path = args.output_dir.join("classes.csv");
    fs::write(&classes_path, render_classes_csv(reports))
        .with_context(|| format!("failed to write {}", classes_path.display()))?;
    println!("Wrote {}", classes_path.display());

    let properties_path = args.output_dir.join("properties.csv");
    fs::write(&properties_path, render_properties_csv(reports))
        .with_context(|| format!("failed to write {}", properties_path.display()))?;
    println!("Wrote {}", properties_path.display());

    Ok(())
}

fn write_dot(args: &Args, reports: &[Report]) -> Result<()> {
    let function_path = args.output_dir.join("function_graph.dot");
    fs::write(&function_path, render_dot_function_graph(reports))
        .with_context(|| format!("failed to write {}", function_path.display()))?;
    println!("Wrote {}", function_path.display());

    let class_path = args.output_dir.join("class_graph.dot");
    fs::write(&class_path, render_dot_class_graph(reports))
        .with_context(|| format!("failed to write {}", class_path.display()))?;
    println!("Wrote {}", class_path.display());
    Ok(())
}

fn write_opcodes(args: &Args, opcodes: &BTreeMap<u8, u64>) -> Result<()> {
    let path = args.output_dir.join("opcodes.csv");
    fs::write(&path, render_opcode_csv(opcodes))
        .with_context(|| format!("failed to write {}", path.display()))?;
    println!("Wrote {}", path.display());
    Ok(())
}

fn aggregate_opcodes(reports: &[Report]) -> BTreeMap<u8, u64> {
    let mut global = BTreeMap::new();
    for report in reports {
        for (opcode, count) in &report.opcodes {
            *global.entry(*opcode).or_insert(0) += count;
        }
    }
    global
}

fn safe_file_name(name: &str) -> String {
    let mut safe: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if safe.is_empty() {
        safe.push_str("package");
    }
    safe
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_escape_handles_quotes_and_commas() {
        assert_eq!(csv_escape("plain"), "plain");
        assert_eq!(csv_escape("a,b"), "\"a,b\"");
        assert_eq!(csv_escape("a\"b"), "\"a\"\"b\"");
        assert_eq!(csv_escape("a\nb"), "\"a\nb\"");
    }

    #[test]
    fn dot_escape_handles_special_characters() {
        assert_eq!(dot_escape("a\"b\\c\nd"), "a\\\"b\\\\c\\nd");
    }

    #[test]
    fn empty_summary_is_valid() {
        let summary = render_summary(&[]);
        assert!(summary.contains("Generated from 0 package(s)."));
        assert!(summary.contains("| Total packages | 0 |"));
        assert!(summary.contains("| Total functions | 0 |"));
    }

    #[test]
    fn opcode_csv_has_header_and_total() {
        let mut opcodes = BTreeMap::new();
        opcodes.insert(0x00, 3);
        opcodes.insert(0x70, 1);
        let csv = render_opcode_csv(&opcodes);
        assert!(csv.starts_with("opcode,hex,name,count,percentage\n"));
        assert!(csv.contains("0,0x00,LocalVariable,3,75.00"));
        assert!(csv.contains("112,0x70,Native,1,25.00"));
        assert!(csv.contains("TOTAL,,,4,100.00"));
    }

    #[test]
    fn safe_file_name_replaces_path_separators() {
        assert_eq!(safe_file_name("DeusEx/ConSys"), "DeusEx_ConSys");
        assert_eq!(safe_file_name("Core"), "Core");
        assert_eq!(safe_file_name(""), "package");
    }

    #[test]
    fn type_labels_match_unrealscript_syntax() {
        assert_eq!(type_label("IntProperty", None), "int");
        assert_eq!(type_label("StrProperty", None), "string");
        assert_eq!(type_label("BoolProperty", None), "bool");
        assert_eq!(type_label("ArrayProperty", Some("int")), "array<int>");
        assert_eq!(type_label("ClassProperty", Some("Actor")), "class<Actor>");
        assert_eq!(type_label("ByteProperty", Some("EInputKey")), "EInputKey");
    }

    #[test]
    fn signature_has_return_type_and_parameters() {
        let parameters = vec![
            ParameterInfo {
                name: "Value".to_string(),
                type_name: "IntProperty".to_string(),
                resolved_type: None,
                flags: 0x80,
                array_dim: 1,
                is_out: false,
                is_optional: false,
                is_skip: false,
                is_return: false,
                is_coerce: false,
            },
            ParameterInfo {
                name: "ReturnValue".to_string(),
                type_name: "BoolProperty".to_string(),
                resolved_type: None,
                flags: 0x480,
                array_dim: 1,
                is_out: false,
                is_optional: false,
                is_skip: false,
                is_return: true,
                is_coerce: false,
            },
        ];
        assert_eq!(
            render_signature("Test", false, &parameters),
            "function bool Test(Value: int)"
        );
    }
}
