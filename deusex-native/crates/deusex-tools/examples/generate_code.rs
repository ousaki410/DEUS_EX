//! Decompile a package and emit scaffold Rust source.
//!
//! Usage:
//!   cargo run -p deusex-tools --example generate_code -- [Package] [out.rs]
//!
//! `Package` defaults to `Core`. If no output path is given the source is
//! written to `/tmp/opencode/deusex_<package>.rs`. The GOTY installation is
//! located through `DEUSEX_GOTY_PATH` or the repository's default path.

use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
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

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let package_name = args.next().unwrap_or_else(|| "Core".to_string());
    let file_name = if package_name.ends_with(".u") {
        package_name.clone()
    } else {
        format!("{package_name}.u")
    };

    let system = system_dir().context(
        "Deus Ex GOTY assets not found; set DEUSEX_GOTY_PATH to the game directory",
    )?;
    let package_path = system.join(&file_name);
    if !package_path.is_file() {
        bail!("package not found: {}", package_path.display());
    }

    let package = UnrealPackage::open(&package_path)
        .with_context(|| format!("open {}", package_path.display()))?;
    let ast = decompile_package(&package).context("decompile package")?;
    let function_count = ast.functions.len();

    let stem = package_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("package")
        .to_lowercase();
    let module_name = format!("deusex_{stem}");
    let decompiled = DecompiledPackage {
        package: ast,
        rust_module_name: module_name.clone(),
    };

    let generated = generate_package(&decompiled, &CodegenOptions::default())
        .context("generate Rust source")?;

    let out_path = match args.next() {
        Some(path) => PathBuf::from(path),
        None => {
            let dir = PathBuf::from("/tmp/opencode");
            fs::create_dir_all(&dir).ok();
            dir.join(format!("{module_name}.rs"))
        }
    };
    fs::write(&out_path, &generated.source)
        .with_context(|| format!("write {}", out_path.display()))?;

    println!(
        "{}: {} functions -> {} ({} bytes, {} warnings)",
        file_name,
        function_count,
        out_path.display(),
        generated.source.len(),
        generated.warning_count()
    );
    for warning in generated.warnings.iter().take(20) {
        println!("  warning: {warning}");
    }
    if generated.warning_count() > 20 {
        println!(
            "  ... and {} more warnings",
            generated.warning_count() - 20
        );
    }

    Ok(())
}
