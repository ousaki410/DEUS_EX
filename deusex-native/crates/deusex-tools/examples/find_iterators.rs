//! Find functions containing iterator (foreach) expressions.
//!
//! Usage:
//!   cargo run -p deusex-tools --example find_iterators -- [Package] [limit]

use std::path::PathBuf;

use deusex_bytecode::{parse_package_functions, Expr, ExprKind};
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

fn count_iterators(expr: &Expr) -> usize {
    let own = if matches!(expr.kind, ExprKind::Iterator { .. }) {
        1
    } else {
        0
    };
    own + expr
        .children()
        .into_iter()
        .map(count_iterators)
        .sum::<usize>()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(system) = system_dir() else {
        eprintln!("Deus Ex GOTY assets not found; set DEUSEX_GOTY_PATH");
        std::process::exit(1);
    };
    let package_name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "Engine".to_string());
    let limit: usize = std::env::args()
        .nth(2)
        .and_then(|value| value.parse().ok())
        .unwrap_or(20);

    let path = if package_name.ends_with(".u") {
        PathBuf::from(&package_name)
    } else {
        system.join(format!("{package_name}.u"))
    };
    let package = UnrealPackage::open(&path)?;
    let functions = parse_package_functions(&package)?;

    let mut shown = 0usize;
    for function in &functions {
        let count = function
            .body
            .iter()
            .map(count_iterators)
            .sum::<usize>();
        if count == 0 {
            continue;
        }
        let path = package
            .object_path(function.export_ref)
            .unwrap_or_else(|| function.display_name());
        println!("{path}  iterators={count}");
        shown += 1;
        if shown >= limit {
            break;
        }
    }
    println!("{shown} function(s) shown");

    Ok(())
}
