//! Find functions containing switch statements.
//!
//! Usage:
//!   cargo run -p deusex-tools --example find_switches -- [Package] [limit]

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

fn count_switches(expr: &Expr) -> usize {
    let own = if matches!(expr.kind, ExprKind::Switch(..)) {
        1
    } else {
        0
    };
    own + expr.children().into_iter().map(count_switches).sum::<usize>()
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
        .unwrap_or(5);

    let package = UnrealPackage::open(system.join(format!("{package_name}.u")))?;
    let functions = parse_package_functions(&package)?;

    let mut matches: Vec<(&Function, usize)> = functions
        .iter()
        .map(|function| {
            let switches = function.body.iter().map(count_switches).sum::<usize>();
            (function, switches)
        })
        .filter(|(_, switches)| *switches > 0)
        .collect();
    matches.sort_by_key(|(function, _)| function.body.len());

    println!(
        "{package_name}: {} / {} functions contain switches",
        matches.len(),
        functions.len()
    );

    for (function, switches) in matches.iter().take(limit) {
        println!(
            "\n== {} ({} switches, {} statements, script={} normalized)",
            function.display_name(),
            switches,
            function.body.len(),
            function.script_size
        );
        for expression in &function.body {
            if matches!(expression.kind, ExprKind::Switch(..)) {
                println!(
                    "   switch @0x{:04X}..0x{:04X}",
                    expression.offset,
                    expression.offset + expression.size
                );
            }
            if let ExprKind::Case { offset, value } = &expression.kind {
                println!(
                    "   case @0x{:04X} -> 0x{offset:04X} value={}",
                    expression.offset,
                    value
                        .as_ref()
                        .map(|value| format!("{:?}", value.kind))
                        .unwrap_or_else(|| "default".to_string())
                );
            }
        }
    }

    Ok(())
}
