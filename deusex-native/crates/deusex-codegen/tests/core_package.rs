//! End-to-end test: parse `Core.u`, decompile to AST, generate Rust, and
//! validate the generated source with `syn`.
//!
//! Skipped when a local Deus Ex GOTY installation is unavailable. Game assets
//! are never redistributed with this repository.

use std::path::PathBuf;

use deusex_ast::DecompiledPackage;
use deusex_bytecode::decompile_package;
use deusex_codegen::{generate_package, CodegenOptions};
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

#[test]
fn generates_valid_rust_for_core_package() {
    let Some(system) = system_dir() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };

    let package = UnrealPackage::open(system.join("Core.u")).expect("open Core.u");
    let ast = decompile_package(&package).expect("decompile Core.u");
    assert!(
        !ast.functions.is_empty(),
        "Core.u should contain decompiled functions"
    );

    let decompiled = DecompiledPackage {
        package: ast,
        rust_module_name: "deusex_core".to_string(),
    };

    let generated =
        generate_package(&decompiled, &CodegenOptions::default()).expect("generate Rust source");

    assert_eq!(generated.module_name, "deusex_core");
    assert!(generated.source.contains("pub mod deusex_core {"));
    assert!(generated.source.len() > 1024, "output should not be trivial");

    syn::parse_file(&generated.source).expect("generated Core.u module should be valid Rust");

    eprintln!(
        "generated {} bytes for Core.u with {} warnings",
        generated.source.len(),
        generated.warning_count()
    );
}

#[test]
fn generated_core_functions_keep_their_names() {
    let Some(system) = system_dir() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };

    let package = UnrealPackage::open(system.join("Core.u")).expect("open Core.u");
    let ast = decompile_package(&package).expect("decompile Core.u");
    let decompiled = DecompiledPackage {
        package: ast,
        rust_module_name: "deusex_core".to_string(),
    };
    let generated =
        generate_package(&decompiled, &CodegenOptions::default()).expect("generate Rust source");

    // `Commandlet.Main` is decompiled at package scope in the current pipeline.
    assert!(
        generated.source.contains("pub fn Main("),
        "Commandlet.Main should be present in generated output"
    );
}
