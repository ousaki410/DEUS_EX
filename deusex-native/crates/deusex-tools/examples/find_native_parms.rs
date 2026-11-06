//! Find functions that contain NativeParm expressions and show their context.
//!
//! Usage:
//!   cargo run -p deusex-tools --example find_native_parms -- [Package] [limit]

use std::path::PathBuf;

use deusex_bytecode::{parse_package_functions, Expr, ExprKind, Function};
use deusex_package::UnrealPackage;

fn system_dir() -> Option<PathBuf> {
    let candidates = [
        std::env::var_os("DEUSEX_GOTY_PATH").map(PathBuf::from),
        Some(PathBuf::from("/home/ou/Projects/deus_ex_goty/goty/app/system")),
    ];
    for candidate in candidates.into_iter().flatten() {
        let system = if candidate.ends_with("system") {
            candidate
        } else {
            candidate.join("system")
        };
        if system.join("Core.u").is_file() {
            return Some(system);
        }
    }
    None
}

fn contains_native_parm(expr: &Expr) -> bool {
    matches!(expr.kind, ExprKind::NativeParm(_))
        || expr.children().iter().any(|child| contains_native_parm(child))
}

fn dump(expr: &Expr, indent: usize) {
    let pad = " ".repeat(indent);
    let summary = match &expr.kind {
        ExprKind::NativeParm(reference) => format!("NativeParm({reference})"),
        ExprKind::Native { index, params } => format!("Native({index}, {} params)", params.len()),
        ExprKind::ExtendedNative { index, params } => {
            format!("ExtendedNative({index}, {} params)", params.len())
        }
        ExprKind::VirtualFunction { name, params } => {
            format!("VirtualFunction(name ref {name}, {} params)", params.len())
        }
        ExprKind::FinalFunction { function, params } => {
            format!("FinalFunction(ref {function}, {} params)", params.len())
        }
        ExprKind::Let { .. } => "Let".to_string(),
        other => format!("{other:?}"),
    };
    println!(
        "{pad}@{:04X}..{:04X} {summary}",
        expr.offset,
        expr.offset + expr.size
    );
    for child in expr.children() {
        dump(child, indent + 2);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(system) = system_dir() else {
        eprintln!("Deus Ex GOTY assets not found; set DEUSEX_GOTY_PATH");
        std::process::exit(1);
    };
    let package_name = std::env::args().nth(1).unwrap_or_else(|| "Engine".to_string());
    let limit: usize = std::env::args()
        .nth(2)
        .and_then(|value| value.parse().ok())
        .unwrap_or(3);

    let package = UnrealPackage::open(system.join(format!("{package_name}.u")))?;
    let functions = parse_package_functions(&package)?;
    println!("{package_name}: {} functions", functions.len());

    let mut shown = 0usize;
    for function in functions.iter().filter(|function: &&Function| {
        function
            .body
            .iter()
            .any(|expr| contains_native_parm(expr))
    }) {
        println!(
            "\n== {}",
            function.display_name()
        );
        for expr in &function.body {
            dump(expr, 2);
        }
        shown += 1;
        if shown >= limit {
            break;
        }
    }
    println!("\nfound at least {shown} functions with NativeParm (limit {limit})");
    Ok(())
}
