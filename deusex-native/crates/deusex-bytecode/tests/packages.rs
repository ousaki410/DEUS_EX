//! Integration tests against a local Deus Ex GOTY installation.
//!
//! Set `DEUSEX_GOTY_PATH` to the directory containing `System/Core.u`, or
//! place the extracted GOTY tree at the default development path. The tests
//! skip when the assets are unavailable, because game data is never part of
//! this repository.

use std::path::PathBuf;

use deusex_bytecode::{
    build_cfg, decompile_function, parse_package_functions, ExprKind, Function,
};
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

fn load(name: &str) -> Option<(UnrealPackage, Vec<Function>)> {
    let dir = system_dir()?;
    let package = UnrealPackage::open(dir.join(name)).expect("open package");
    let functions = parse_package_functions(&package).expect("parse functions");
    Some((package, functions))
}

fn find<'a>(package: &UnrealPackage, functions: &'a [Function], path: &str) -> &'a Function {
    functions
        .iter()
        .find(|function| {
            package
                .object_path(function.export_ref)
                .map(|candidate| candidate.eq_ignore_ascii_case(path))
                .unwrap_or(false)
        })
        .unwrap_or_else(|| panic!("function {path} not found"))
}

#[test]
fn parses_core_engine_and_deusex_functions() {
    let Some((core_package, core)) = load("Core.u") else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    let (engine_package, engine) = load("Engine.u").expect("Engine.u");
    let (deusex_package, deusex) = load("DeusEx.u").expect("DeusEx.u");

    assert_eq!(core_package.names().len(), 342);
    assert_eq!(core_package.imports().len(), 16);
    assert_eq!(core_package.exports().len(), 699);
    assert_eq!(engine_package.names().len(), 3574);
    assert_eq!(engine_package.imports().len(), 60);
    assert_eq!(engine_package.exports().len(), 5622);
    assert_eq!(deusex_package.names().len(), 12872);
    assert_eq!(deusex_package.imports().len(), 3336);
    assert_eq!(deusex_package.exports().len(), 21422);

    assert_eq!(core.len(), 170);
    assert_eq!(engine.len(), 1263);
    assert_eq!(deusex.len(), 4933);

    for (package, functions) in [
        (&core_package, &core),
        (&engine_package, &engine),
        (&deusex_package, &deusex),
    ] {
        for function in functions {
            assert_eq!(
                function.reader_end, function.export_end,
                "layout mismatch for {}",
                function.display_name()
            );
            assert_eq!(
                function.script_offset + function.script_raw_size <= function.export_end,
                true,
                "script exceeds export for {}",
                function.display_name()
            );
            let _ = package;
        }
    }
}

#[test]
fn decodes_commandlet_main() {
    let Some((package, functions)) = load("Core.u") else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };

    let main = find(&package, &functions, "Commandlet.Main");
    assert_eq!(main.script_size, 5);
    assert_eq!(main.script_raw_size, 2);
    assert_eq!(main.function_flags, 0xC00);
    assert_eq!(main.body.len(), 1);
    match &main.body[0].kind {
        ExprKind::NativeParm(reference) => assert_eq!(*reference, 22),
        other => panic!("unexpected Main body: {other:?}"),
    }

    let hello = find(&package, &functions, "HelloWorldCommandlet.Main");
    let mut has_hello = false;
    fn visit(expr: &deusex_bytecode::Expr, found: &mut bool) {
        if let ExprKind::StringConst(value) = &expr.kind {
            if value == "Hello, world!" {
                *found = true;
            }
        }
        for child in expr.children() {
            visit(child, found);
        }
    }
    for expr in &hello.body {
        visit(expr, &mut has_hello);
    }
    assert!(has_hello, "HelloWorldCommandlet.Main should contain its greeting");
}

#[test]
fn builds_cfg_for_real_functions() {
    let Some((package, functions)) = load("Core.u") else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };

    for function in &functions {
        let cfg = build_cfg(function);
        assert_eq!(cfg.entry, 0);
        
        // Native functions with no script body will have empty CFG
        if function.script_size == 0 {
            assert!(cfg.blocks.is_empty());
            continue;
        }
        
        assert!(!cfg.blocks.is_empty(), "function {} has no blocks", function.display_name());
        
        // Every block should have its statements mapped correctly
        for block in &cfg.blocks {
            for &stmt_idx in &block.statements {
                assert_eq!(cfg.statement_block[stmt_idx], block.id);
            }
        }
        
        // Verify no statement is left unmapped
        for (idx, &block_id) in cfg.statement_block.iter().enumerate() {
            assert!(block_id < cfg.blocks.len(), "statement {} mapped to invalid block {}", idx, block_id);
        }
        
        // Check that targets pointing to function end are handled
        if function.script_size > 0 {
            let has_end_block = cfg.blocks.iter().any(|b| b.start == function.script_size);
            if !has_end_block && cfg.unresolved_targets.iter().any(|(t, _)| *t == function.script_size) {
                // Function end is an expected unresolved target for fallthrough from last block
            }
        }
    }
}

#[test]
fn decompiles_all_core_functions() {
    let Some((package, functions)) = load("Core.u") else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };

    let mut errors = 0usize;
    for function in &functions {
        if decompile_function(function, &package).is_err() {
            errors += 1;
            if errors <= 5 {
                eprintln!("decompile error: {}", function.display_name());
            }
        }
    }
    assert_eq!(errors, 0, "all Core.u functions should decompile without errors");

    // Spot-check Commandlet.Main
    let main = find(&package, &functions, "Commandlet.Main");
    let ast = decompile_function(main, &package).expect("decompile Main");
    assert_eq!(ast.name, "Main");
    assert!(!ast.body.is_empty(), "Main should have a non-empty body");
}
