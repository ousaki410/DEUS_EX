//! `deusex-native` CLI engine: load all `.u` packages and interactively
//! execute UnrealScript functions through the bytecode VM.
//!
//! This is a Linux native command-line engine — no Wine, no GUI dev
//! libraries required. It loads every `.u` package in the system
//! directory and provides a REPL where you can type function paths to
//! execute them through the VM and see the results.
//!
//! Usage:
//!   cargo run --example run_cli [--system DIR]
//!
//! The `system` directory defaults to $DEUSEX_GOTY_PATH/system, then to
//! /home/ou/Projects/deus_ex_goty/goty/app/system.
//!
//! Example session:
//!   > Commandlet.Main Core.u
//!   Hello, world!
//!   return_value = Int(0)
//!   > Object.Abs Core.u -5
//!   5

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use clap::Parser;
use deusex_vm::{PackageUnit, Value, Vm};

#[derive(Parser, Debug)]
#[command(author, version, about = "Deus Ex Native CLI engine (Linux)")]
struct Args {
    /// Directory containing the `.u` packages.
    #[arg(long)]
    system: Option<PathBuf>,

    /// Comma-separated list of packages to load (default: all .u files).
    #[arg(long)]
    packages: Option<String>,

    /// Execute one function and exit (non-interactive).
    #[arg(long)]
    exec: Option<String>,

    /// Max call depth for the VM.
    #[arg(long, default_value_t = 1024)]
    call_depth: usize,
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

fn discover_packages(system: &Path, filter: Option<&str>) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(system) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("u") {
                let name = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
                if let Some(filter) = filter {
                    if filter.split(',').any(|f| name == f.trim()) {
                        names.push(name);
                    }
                } else {
                    names.push(name);
                }
            }
        }
    }
    names.sort_by(|a, b| {
        let a_core = a == "Core";
        let b_core = b == "Core";
        match (a_core, b_core) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.cmp(b),
        }
    });
    names
}

fn load_package(system: &Path, name: &str) -> Result<PackageUnit, Box<dyn std::error::Error>> {
    let path = system.join(format!("{name}.u"));
    Ok(PackageUnit::load(&path)?)
}

fn run_function(
    vm: &mut Vm,
    func_path: &str,
    args: Vec<Value>,
) -> Result<Value, Box<dyn std::error::Error>> {
    let (unit_index, function_index) = vm
        .find_function(func_path)
        .ok_or_else(|| format!("function not found: {func_path}"))?;

    let outcome = vm
        .exec_function(unit_index, function_index, None, args)
        .map_err(|err| format!("execution failed: {err:?}"))?;

    // Print logs produced by this call.
    for line in vm.logs() {
        println!("  log: {line}");
    }

    Ok(outcome.return_value)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let system = locate_system_dir(args.system.as_deref()).ok_or_else(|| {
        "Deus Ex GOTY assets not found; pass --system or set DEUSEX_GOTY_PATH"
    })?;

    let package_names = discover_packages(&system, args.packages.as_deref());
    eprintln!("discovered {} packages in {}", package_names.len(), system.display());

    let mut vm = Vm::new();
    vm.set_call_depth_limit(args.call_depth);

    for name in &package_names {
        match load_package(&system, name) {
            Ok(unit) => {
                eprintln!("loaded package: {name}");
                vm.add_unit(unit);
            }
            Err(e) => {
                eprintln!("WARNING: failed to load {name}: {e}");
            }
        }
    }

    eprintln!("\nDeus Ex Native CLI engine ready.");
    eprintln!("Type a function path (e.g. Object.Abs Core.u -5) or 'quit' to exit.\n");

    // Non-interactive mode: execute one function and exit.
    if let Some(exec_str) = &args.exec {
        let parts: Vec<&str> = exec_str.splitn(2, ' ').collect();
        let func_path = parts[0];
        let func_args: Vec<Value> = if parts.len() > 1 {
            parse_args(parts[1])?
        } else {
            Vec::new()
        };
        let result = run_function(&mut vm, func_path, func_args)?;
        println!("return_value = {result:?}");
        return Ok(());
    }

    // Interactive REPL.
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        let line = line.trim();

        if line.is_empty() || line == "quit" || line == "exit" {
            break;
        }

        if line.starts_with('#') || line.starts_with("//") {
            continue;
        }

        // Parse: "FunctionPath [arg1 arg2 ...]"
        let mut parts = line.split_whitespace();
        let func_path = match parts.next() {
            Some(p) => p,
            None => continue,
        };
        let func_args: Vec<Value> = parts.map(|a| parse_value_arg(a)).collect();

        print!("> ");
        stdout.flush()?;

        match run_function(&mut vm, func_path, func_args) {
            Ok(result) => {
                println!("return_value = {result:?}");
                println!("assertion_failures = {}", vm.assertion_failures());
            }
            Err(err) => {
                eprintln!("error: {err}");
            }
        }
    }

    eprintln!("\nDeus Ex Native CLI engine shutdown.");
    Ok(())
}

/// Parse a command-line argument into a Value. Supports int, float, bool,
/// string (quoted), name (quoted), None, and object references (None).
fn parse_value_arg(s: &str) -> Value {
    let s = s.trim();
    if s.eq_ignore_ascii_case("none") || s == "null" {
        return Value::None;
    }
    if s.eq_ignore_ascii_case("true") {
        return Value::Bool(true);
    }
    if s.eq_ignore_ascii_case("false") {
        return Value::Bool(false);
    }
    // Quoted string
    if let Some(s) = s.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return Value::String(s.to_string());
    }
    // Quoted name
    if let Some(s) = s.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
        return Value::Name(s.to_string());
    }
    // Integer
    if let Ok(n) = s.parse::<i32>() {
        return Value::Int(n);
    }
    // Float
    if let Ok(f) = s.parse::<f32>() {
        return Value::Float(f);
    }
    // Fallback: string
    Value::String(s.to_string())
}

/// Parse all arguments after the function path.
fn parse_args(s: &str) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    Ok(s.split_whitespace().map(parse_value_arg).collect())
}
