//! Parse and disassemble UnrealScript functions from a Deus Ex package.
//!
//! Usage:
//!   cargo run -p deusex-tools --example parse_functions -- <package.u> [function-name]
//!
//! The optional function name can be a short name such as `Main` or a dotted
//! path such as `Commandlet.Main`.

use std::env;
use std::path::Path;

use deusex_bytecode::{parse_package_functions, Expr, Function, Token};
use deusex_package::UnrealPackage;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: {} <package.u> [function-name]", args[0]);
        std::process::exit(1);
    }

    let package = UnrealPackage::open(Path::new(&args[1]))?;
    println!(
        "{}: version={} names={} imports={} exports={}",
        args[1],
        package.summary().file_version,
        package.names().len(),
        package.imports().len(),
        package.exports().len()
    );

    let functions = parse_package_functions(&package)?;
    let mut mismatches = 0usize;
    for function in &functions {
        if function.reader_end != function.export_end {
            mismatches += 1;
            if mismatches <= 10 {
                eprintln!(
                    "layout mismatch: {} reader_end=0x{:X} export_end=0x{:X}",
                    function.display_name(),
                    function.reader_end,
                    function.export_end
                );
            }
        }
    }
    println!(
        "parsed {} functions ({} layout mismatches)",
        functions.len(),
        mismatches
    );

    let selector = args.get(2).map(|s| s.as_str());
    let mut printed = 0usize;
    for function in &functions {
        let path = package
            .object_path(function.export_ref)
            .unwrap_or_else(|| function.display_name());
        let selected = selector
            .map(|needle| {
                path.eq_ignore_ascii_case(needle)
                    || function.name.eq_ignore_ascii_case(needle)
                    || function.display_name().eq_ignore_ascii_case(needle)
                    || path.ends_with(needle)
            })
            .unwrap_or(printed < 10);

        if !selected {
            continue;
        }

        print_function(&package, function, &path);
        printed += 1;
        if selector.is_none() && printed >= 10 {
            break;
        }
    }

    if printed == 0 {
        eprintln!("no matching function found");
    }

    Ok(())
}

fn print_function(package: &UnrealPackage, function: &Function, path: &str) {
    println!(
        "\n== {path}  script={} raw bytes / {} normalized, native={} precedence={} flags=0x{:X}",
        function.script_raw_size,
        function.script_size,
        function.native_index,
        function.precedence,
        function.function_flags
    );
    for (index, expr) in function.body.iter().enumerate() {
        println!("  [{index}]");
        dump_expr(package, expr, 4);
    }
}

fn dump_expr(package: &UnrealPackage, expr: &Expr, indent: usize) {
    let pad = " ".repeat(indent);
    let token = expr
        .token()
        .map(Token::name)
        .unwrap_or("<unknown>");
    println!(
        "{pad}@{:04X}..{:04X} {token}: {}",
        expr.offset,
        expr.offset + expr.size,
        describe_kind(package, &expr.kind)
    );
    for child in expr.children() {
        dump_expr(package, child, indent + 2);
    }
}

fn describe_kind(package: &UnrealPackage, kind: &deusex_bytecode::ExprKind) -> String {
    use deusex_bytecode::ExprKind;

    let resolve = |reference: i32| {
        package
            .object_path(reference)
            .unwrap_or_else(|| format!("<ref {reference}>"))
    };
    let name = |index: i32| {
        package
            .name(index)
            .map(str::to_string)
            .unwrap_or_else(|| format!("<name {index}>"))
    };

    match kind {
        ExprKind::LocalVariable(reference) => {
            format!("LocalVariable({}) // {}", reference, resolve(*reference))
        }
        ExprKind::InstanceVariable(reference) => {
            format!("InstanceVariable({}) // {}", reference, resolve(*reference))
        }
        ExprKind::DefaultVariable(reference) => {
            format!("DefaultVariable({}) // {}", reference, resolve(*reference))
        }
        ExprKind::NativeParm(reference) => {
            format!("NativeParm({}) // {}", reference, resolve(*reference))
        }
        ExprKind::ObjectConst(reference) => {
            format!("ObjectConst({}) // {}", reference, resolve(*reference))
        }
        ExprKind::NameConst(reference) => {
            format!("NameConst({}) // {}", reference, name(*reference))
        }
        ExprKind::VirtualFunction { name: index, params } => format!(
            "VirtualFunction({}) // {} [{} args]",
            index,
            name(*index),
            params.len()
        ),
        ExprKind::GlobalFunction { name: index, params } => format!(
            "GlobalFunction({}) // {} [{} args]",
            index,
            name(*index),
            params.len()
        ),
        ExprKind::FinalFunction {
            function,
            params,
        } => format!(
            "FinalFunction({}) // {} [{} args]",
            function,
            resolve(*function),
            params.len()
        ),
        ExprKind::Native { index, params } => format!("Native({index}) [{} args]", params.len()),
        ExprKind::ExtendedNative { index, params } => {
            format!("ExtendedNative({index}) [{} args]", params.len())
        }
        ExprKind::Conversion { opcode, .. } => format!("Conversion(0x{opcode:02X})"),
        ExprKind::Jump(offset) => format!("Jump(0x{offset:04X})"),
        ExprKind::JumpIfNot { offset, .. } => format!("JumpIfNot(0x{offset:04X})"),
        ExprKind::Assert { offset, .. } => format!("Assert(0x{offset:04X})"),
        ExprKind::Case { offset, value } => format!(
            "Case(0x{offset:04X}) {}",
            if value.is_some() { "value" } else { "default" }
        ),
        ExprKind::Switch(byte, _) => format!("Switch(byte={byte})"),
        ExprKind::GotoLabel(_) => "GotoLabel".to_string(),
        ExprKind::EatString(_) => "EatString".to_string(),
        ExprKind::Let { .. } => "Let".to_string(),
        ExprKind::LetBool { .. } => "LetBool".to_string(),
        ExprKind::New(params) => format!("New [{} args]", params.len()),
        ExprKind::ClassContext { offset, byte, .. } => {
            format!("ClassContext(0x{offset:04X}, byte={byte})")
        }
        ExprKind::Context { offset, byte, .. } => {
            format!("Context(0x{offset:04X}, byte={byte})")
        }
        ExprKind::MetaCast { class, .. } => format!("MetaCast({}) // {}", class, resolve(*class)),
        ExprKind::DynamicCast { class, .. } => {
            format!("DynamicCast({}) // {}", class, resolve(*class))
        }
        ExprKind::Skip { offset, .. } => format!("Skip(0x{offset:04X})"),
        ExprKind::Iterator { offset, .. } => format!("Iterator(0x{offset:04X})"),
        ExprKind::StructMember { struct_ref, .. } => {
            format!("StructMember({}) // {}", struct_ref, resolve(*struct_ref))
        }
        ExprKind::StructCmpEq { struct_ref, .. } => {
            format!("StructCmpEq({}) // {}", struct_ref, resolve(*struct_ref))
        }
        ExprKind::StructCmpNe { struct_ref, .. } => {
            format!("StructCmpNe({}) // {}", struct_ref, resolve(*struct_ref))
        }
        ExprKind::ArrayElement { .. } | ExprKind::DynArrayElement { .. } => {
            "ArrayElement".to_string()
        }
        ExprKind::LabelTable(labels) => format!("LabelTable({} labels)", labels.len()),
        ExprKind::BoolVariable(_) => "BoolVariable".to_string(),
        ExprKind::Unknown2B { byte, .. } => format!("Unknown2B(byte={byte})"),
        ExprKind::IntConst(value) => format!("IntConst({value})"),
        ExprKind::IntConstByte(value) => format!("IntConstByte({value})"),
        ExprKind::ByteConst(value) => format!("ByteConst({value})"),
        ExprKind::FloatConst(value) => format!("FloatConst({value})"),
        ExprKind::StringConst(value) => format!("StringConst({value:?})"),
        ExprKind::UnicodeStringConst(value) => format!("UnicodeStringConst({value:?})"),
        ExprKind::VectorConst(value) => format!("VectorConst({value:?})"),
        ExprKind::RotationConst(value) => format!("RotationConst({value:?})"),
        ExprKind::IntZero => "IntZero".to_string(),
        ExprKind::IntOne => "IntOne".to_string(),
        ExprKind::True => "True".to_string(),
        ExprKind::False => "False".to_string(),
        ExprKind::Nothing => "Nothing".to_string(),
        ExprKind::NoObject => "NoObject".to_string(),
        ExprKind::SelfObject => "Self".to_string(),
        ExprKind::EndFunctionParms => "EndFunctionParms".to_string(),
        ExprKind::Unknown15 => "Unknown15".to_string(),
        ExprKind::IteratorPop => "IteratorPop".to_string(),
        ExprKind::IteratorNext => "IteratorNext".to_string(),
        ExprKind::Return(_) => "Return".to_string(),
        ExprKind::Stop => "Stop".to_string(),
    }
}
