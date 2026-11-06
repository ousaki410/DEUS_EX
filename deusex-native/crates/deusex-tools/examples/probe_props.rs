//! Probe UProperty metadata for a function's parameter chain.
//!
//! Usage:
//!   cargo run -p deusex-tools --example probe_props -- [Package] [FunctionPath]
//!
//! Example:
//!   cargo run -p deusex-tools --example probe_props -- Core HelloWorldCommandlet.Main

use std::path::PathBuf;

use deusex_bytecode::parse_package_functions;
use deusex_package::{function_parameters, property_chain, UnrealPackage};

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
    let package_name = std::env::args().nth(1).unwrap_or_else(|| "Core".to_string());
    let function_path = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "HelloWorldCommandlet.Main".to_string());

    let package = UnrealPackage::open(system.join(format!("{package_name}.u")))?;
    let functions = parse_package_functions(&package)?;
    let function = functions
        .iter()
        .find(|function| {
            package
                .object_path(function.export_ref)
                .map(|path| path.eq_ignore_ascii_case(&function_path))
                .unwrap_or(false)
        })
        .unwrap_or_else(|| panic!("function {function_path} not found"));

    println!(
        "function {} children_ref={} -> {:?}",
        function_path,
        function.children_ref,
        package.object_path(function.children_ref)
    );

    let params = function_parameters(&package, function.children_ref)?;
    println!("  {} parameters:", params.len());
    for param in &params {
        let type_info = if param.type_ref != 0 {
            format!(
                " type_ref={} -> {:?}",
                param.type_ref,
                package.object_path(param.type_ref)
            )
        } else {
            String::new()
        };
        println!(
            "    {} : {} flags=0x{:08X} parm={} out={} return={} coerce={}{}",
            param.name,
            param.class_name,
            param.flags,
            param.is_parm(),
            param.is_out_parm(),
            param.is_return_parm(),
            param.is_coerce_parm(),
            type_info
        );
    }

    println!("  full chain:");
    for (index, prop) in property_chain(&package, function.children_ref)?
        .iter()
        .enumerate()
    {
        println!(
            "    [{index}] ref={} {} : {} next={} -> {:?}",
            prop.reference,
            prop.name,
            prop.class_name,
            prop.next,
            package.object_path(prop.next)
        );
    }

    Ok(())
}
