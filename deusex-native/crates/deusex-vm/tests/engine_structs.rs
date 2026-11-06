//! Struct/vector member access, using Engine.u's `TestInfo.TestQ`.

use std::path::PathBuf;

use deusex_vm::{PackageUnit, Value, Vm};

fn system_dir() -> Option<PathBuf> {
    let candidates = [
        std::env::var_os("DEUSEX_GOTY_PATH").map(PathBuf::from),
        Some(PathBuf::from(
            "/home/ou/Projects/deus_ex_goty/goty/app/system",
        )),
    ];
    for candidate in candidates.into_iter().flatten() {
        let system = if candidate.ends_with("system") {
            candidate
        } else {
            candidate.join("system")
        };
        if system.join("Engine.u").is_file() {
            return Some(system);
        }
    }
    None
}

fn engine_vm() -> Option<Vm> {
    let system = system_dir()?;
    let core = PackageUnit::load(system.join("Core.u")).expect("load Core.u");
    let engine = PackageUnit::load(system.join("Engine.u")).expect("load Engine.u");
    let mut vm = Vm::new();
    vm.add_unit(core);
    vm.add_unit(engine);
    Some(vm)
}

#[test]
fn executes_vector_struct_assignment() {
    let Some(mut vm) = engine_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };

    let (unit, function) = vm
        .find_function("TestInfo.TestQ")
        .expect("TestInfo.TestQ should exist");
    let result = vm
        .exec_function(unit, function, None, Vec::new())
        .expect("execute TestInfo.TestQ")
        .return_value;

    assert_eq!(result, Value::None);
    assert_eq!(
        vm.assertion_failures(),
        0,
        "vector member reads/writes should satisfy all asserts"
    );
}
