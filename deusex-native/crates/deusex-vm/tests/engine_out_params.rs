//! Out-parameter write-back, using Engine.u's built-in TestOptionalOut.

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
fn writes_back_out_parameters() {
    let Some(mut vm) = engine_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };

    // `TestOptionalOut` initialises A/B/C, then calls `SubTestOptionalOut`
    // several times with different argument subsets. `SubTestOptionalOut`
    // doubles each parameter:
    //   A *= 2.0, B = B * 2, C += C
    // The caller asserts the values after every call, so all writes must be
    // propagated back through both script out parameters and the
    // compound-assignment intrinsics.
    let object = vm.create_object_with_class("TestInfo instance", "TestInfo");
    let (unit, function) = vm
        .find_function("TestInfo.TestOptionalOut")
        .expect("TestInfo.TestOptionalOut should exist");
    let result = vm
        .exec_function(unit, function, Some(object), Vec::new())
        .expect("execute TestInfo.TestOptionalOut")
        .return_value;

    assert_eq!(vm.logs(), ["TestOptionalOut ok!"]);
    assert_eq!(vm.assertion_failures(), 0);
    assert_eq!(result, Value::None);
}
