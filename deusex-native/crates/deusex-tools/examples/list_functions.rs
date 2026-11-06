//! List functions in a package, optionally filtered by path substring.
//!
//! Usage:
//!   cargo run -p deusex-tools --example list_functions -- <package.u> [filter]

use std::path::Path;

use deusex_bytecode::parse_package_functions;
use deusex_package::UnrealPackage;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!(
            "usage: {} <package.u> [filter]",
            args.first().map(String::as_str).unwrap_or("list_functions")
        );
        std::process::exit(1);
    }

    let package = UnrealPackage::open(Path::new(&args[1]))?;
    let functions = parse_package_functions(&package)?;
    let filter = args.get(2).map(|value| value.to_ascii_lowercase());

    for function in &functions {
        let path = package
            .object_path(function.export_ref)
            .unwrap_or_else(|| function.display_name());
        if let Some(filter) = &filter {
            if !path.to_ascii_lowercase().contains(filter) {
                continue;
            }
        }
        println!(
            "{path}  flags=0x{:X} native={} body={} script={}",
            function.function_flags,
            function.native_index,
            function.body.len(),
            function.script_size
        );
    }

    Ok(())
}
