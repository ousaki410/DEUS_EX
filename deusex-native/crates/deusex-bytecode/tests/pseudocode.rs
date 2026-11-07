//! Integration tests for the symbol-preserving pseudocode renderer.
//!
//! These tests use the local Deus Ex GOTY installation when available and
//! skip otherwise; game data is never committed to the repository.

use std::path::PathBuf;
use std::sync::OnceLock;

use deusex_bytecode::{
    parse_package_functions, render_pseudocode, Function, NativeTable,
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

/// Native indices are global to the game package set; operator natives such
/// as `EqualEqual_NameName` live in `Core.u`, so tests need a global table.
fn native_table() -> NativeTable {
    static NATIVES: OnceLock<NativeTable> = OnceLock::new();
    NATIVES
        .get_or_init(|| {
            let mut natives = NativeTable::new();
            for name in ["Core.u", "Engine.u", "DeusEx.u", "Extension.u"] {
                let Some((package, functions)) = load(name) else {
                    continue;
                };
                for function in &functions {
                    if function.native_index > 0 {
                        if let Some(path) = package.object_path(function.export_ref) {
                            natives.insert(function.native_index as u32, path);
                        }
                    }
                }
            }
            natives
        })
        .clone()
}

#[test]
fn renders_hello_world_commandlet() {
    let Some((package, functions)) = load("Core.u") else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    let natives = native_table();
    let main = find(&package, &functions, "HelloWorldCommandlet.Main");
    let code = render_pseudocode(&package, main, &natives);

    assert!(code.contains("Log(\"Hello, world!\")"), "{code}");
    assert!(code.contains("if (Parms == \"\") goto"), "{code}");
    assert!(
        code.contains("Log(\"Command line parameters=\" $ Parms)"),
        "{code}"
    );
}

#[test]
fn renders_array_elements_with_index_operand_first() {
    let Some((package, functions)) = load("Core.u") else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    let natives = native_table();
    let sprintf = find(&package, &functions, "Object.Sprintf");
    let code = render_pseudocode(&package, sprintf, &natives);

    assert!(code.contains("S[0] = s0;"), "{code}");
    assert!(code.contains("S[3] = s3;"), "{code}");
    // The old parser order produced `expr[<array>]` shapes such as `s0[S]`.
    assert!(!code.contains("s0[S]"), "{code}");
}

#[test]
fn renders_native_stubs_as_declarations() {
    let Some((package, functions)) = load("Core.u") else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    let natives = native_table();
    let add = find(&package, &functions, "Object.Add_IntInt");
    let code = render_pseudocode(&package, add, &natives);

    assert!(
        code.contains("static native function int Add_IntInt(A: int, B: int);"),
        "{code}"
    );
    assert!(code.contains("// native index 146"), "{code}");
    assert!(!code.contains('{'), "stub body leaked: {code}");
}

#[test]
fn treats_bool_variable_as_a_read_not_a_negation() {
    let Some((package, functions)) = load("DeusEx.u") else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    let natives = native_table();
    let take_damage = find(
        &package,
        &functions,
        "AlarmUnit.Active.TakeDamage",
    );
    let code = render_pseudocode(&package, take_damage, &natives);

    // Source: `if (!bConfused) { ... }`; goto form inverts the branch.
    assert!(code.contains("if (bConfused) goto"), "{code}");
    assert!(!code.contains("!!"), "{code}");
    assert!(!code.contains("!bConfused"), "{code}");
}

#[test]
fn renders_assert_line_numbers_without_goto_targets() {
    let Some((package, functions)) = load("Engine.u") else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    let natives = native_table();
    let test = find(&package, &functions, "TestInfo.TestQ");
    let code = render_pseudocode(&package, test, &natives);

    assert!(
        code.contains("assert(V == vect(2.0, 3.0, 4.0)); // line 38"),
        "{code}"
    );
    assert!(!code.contains("else goto"), "{code}");
    assert!(!code.contains("L0026"), "{code}");
}

#[test]
fn folds_negated_comparisons_in_jump_conditions() {
    let Some((package, functions)) = load("Core.u") else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    let natives = native_table();
    let main = find(&package, &functions, "HelloWorldCommandlet.Main");
    let code = render_pseudocode(&package, main, &natives);

    assert!(code.contains("if (Parms == \"\") goto"), "{code}");
    assert!(code.contains("if (intparm == 0) goto"), "{code}");
    assert!(!code.contains("!((Parms != \"\"))"), "{code}");
}

#[test]
fn renders_default_case_sentinel_without_a_jump() {
    let Some((package, functions)) = load("DeusEx.u") else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    let natives = native_table();
    let function = find(&package, &functions, "ToolWindow.ButtonActivated");
    let code = render_pseudocode(&package, function, &natives);

    assert!(code.contains("default:"), "{code}");
    assert!(!code.contains("LFFFF"), "{code}");
    assert!(code.contains("return bHandled;"), "{code}");
}
