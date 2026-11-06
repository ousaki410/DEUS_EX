//! `deusex-native` CLI example: load one or more `.u` packages and run an
//! UnrealScript function through the bytecode VM.
//!
//! Usage:
//!   cargo run --example run_script -- [--system DIR] <FunctionPath> [Package.u]...
//!
//! Examples:
//!   cargo run --example run_script -- Commandlet.Main Core.u
//!   cargo run --example run_script -- HelloWorldCommandlet.Main Core.u
//!
//! The `system` directory defaults to `$DEUSEX_GOTY_PATH/system`, then to
//! `/home/ou/Projects/deus_ex_goty/goty/app/system`. `Core.u` is always
//! loaded first (other packages typically depend on it).

use std::path::{Path, PathBuf};

use clap::Parser;
use deusex_vm::{PackageUnit, Value, Vm};

#[derive(Parser, Debug)]
#[command(author, version, about = "Run a UnrealScript function through the deusex VM")]
struct Args {
    /// Directory containing the `.u` packages (defaults to $DEUSEX_GOTY_PATH/system,
    /// then /home/ou/Projects/deus_ex_goty/goty/app/system).
    #[arg(long)]
    system: Option<PathBuf>,

    /// Script path of the function to execute, e.g. `Commandlet.Main`.
    function_path: String,

    /// Additional `.u` packages to load (Core.u is loaded first automatically).
    packages: Vec<String>,
}

fn locate_system_dir(explicit: Option<&Path>) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(dir) = explicit {
        candidates.push(if dir.ends_with("system") {
            dir.to_path_buf()
        } else {
            dir.join("system")
        });
    }
    if let Ok(env) = std::env::var("DEUSEX_GOTY_PATH") {
        let p = PathBuf::from(env);
        candidates.push(if p.ends_with("system") { p } else { p.join("system") });
    }
    candidates.push(PathBuf::from(
        "/home/ou/Projects/deus_ex_goty/goty/app/system",
    ));
    for system in candidates {
        if system.join("Core.u").is_file() {
            return Some(system);
        }
    }
    None
}

fn load_package(system: &Path, name: &str) -> Result<PackageUnit, Box<dyn std::error::Error>> {
    let path = if name.ends_with(".u") {
        system.join(name)
    } else {
        system.join(format!("{name}.u"))
    };
    Ok(PackageUnit::load(&path)?)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let system = locate_system_dir(args.system.as_deref()).ok_or_else(|| {
        "Deus Ex GOTY assets not found; pass --system or set DEUSEX_GOTY_PATH"
    })?;

    let mut vm = Vm::new();

    // Core.u is the foundation package; load it first when present.
    if system.join("Core.u").is_file() {
        let core = load_package(&system, "Core.u")?;
        vm.add_unit(core);
    }

    for name in &args.packages {
        let unit = load_package(&system, name)?;
        eprintln!("loaded package: {name}");
        vm.add_unit(unit);
    }

    let (unit_index, function_index) = vm
        .find_function(&args.function_path)
        .ok_or_else(|| format!("function not found: {}", args.function_path))?;

    eprintln!(
        "executing {} (unit {}, function {})",
        args.function_path, unit_index, function_index
    );

    let outcome = vm
        .exec_function(unit_index, function_index, None, Vec::new())
        .map_err(|err| format!("execution failed: {err:?}"))?;

    eprintln!("--- logs ---");
    for line in vm.logs() {
        eprintln!("{line}");
    }
    eprintln!("--- result ---");
    eprintln!("return_value = {:?}", outcome.return_value);
    eprintln!("out_values   = {:?}", outcome.out_values);
    eprintln!("assertion_failures = {}", vm.assertion_failures());

    // Print the return value in a machine-friendly way on stdout.
    match &outcome.return_value {
        Value::None => println!("None"),
        Value::Int(n) => println!("{n}"),
        Value::Float(f) => println!("{f}"),
        Value::Bool(b) => println!("{}", if *b { "True" } else { "False" }),
        Value::String(s) => println!("{s}"),
        Value::Name(n) => println!("{n}"),
        Value::Byte(b) => println!("{b}"),
        other => println!("{other:?}"),
    }

    Ok(())
}
