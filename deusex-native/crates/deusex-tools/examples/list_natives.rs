//! List native function indices for one or more packages.
//!
//! Usage:
//!   cargo run -p deusex-tools --example list_natives -- Core Engine DeusEx
//!
//! Names are resolved from the package's UFunction exports. The output is
//! used to build the runtime's native dispatch table.

use std::path::PathBuf;

use deusex_bytecode::{function_flags, parse_package_functions};
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(system) = system_dir() else {
        eprintln!("Deus Ex GOTY assets not found; set DEUSEX_GOTY_PATH");
        std::process::exit(1);
    };

    let package_names: Vec<String> = std::env::args()
        .skip(1)
        .collect::<Vec<_>>();
    let package_names = if package_names.is_empty() {
        vec![
            "Core".to_string(),
            "Engine".to_string(),
            "DeusEx".to_string(),
        ]
    } else {
        package_names
    };

    for name in package_names {
        let path = system.join(format!("{name}.u"));
        let package = UnrealPackage::open(&path)?;
        let functions = parse_package_functions(&package)?;
        let mut natives: Vec<_> = functions
            .iter()
            .filter(|function| {
                function.native_index > 0
                    || function.function_flags & function_flags::INTRINSIC != 0
            })
            .map(|function| {
                let path = package
                    .object_path(function.export_ref)
                    .unwrap_or_else(|| function.display_name());
                (
                    function.native_index,
                    function.function_flags,
                    path,
                )
            })
            .collect();
        natives.sort_by_key(|(index, _, _)| *index);

        println!("== {name}: {} natives", natives.len());
        for (index, flags, path) in natives {
            let kind = if flags & function_flags::INTRINSIC != 0 {
                "intrinsic"
            } else {
                "native"
            };
            println!("  {index:3}  {kind:9}  {path}");
        }
    }

    Ok(())
}
