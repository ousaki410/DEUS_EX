//! Switch and assertion behaviour, using Engine.u's built-in test function.

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
fn executes_switch_statement() {
    let Some(mut vm) = engine_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };

    let (unit, function) = vm
        .find_function("TestInfo.TestSwitch")
        .expect("TestInfo.TestSwitch should exist");
    let result = vm
        .exec_function(unit, function, None, Vec::new())
        .expect("execute TestInfo.TestSwitch")
        .return_value;

    // `TestSwitch` asserts its intermediate state and logs success.
    assert_eq!(vm.logs(), ["testswitch succeeded"]);
    assert_eq!(result, Value::None);
    assert_eq!(vm.assertion_failures(), 0);
}

#[test]
fn dispatches_virtual_function_through_class_path() {
    let Some(mut vm) = engine_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };

    // TestStatic(i) asserts:
    //   i == 123, TestInfo.xnum == 777, OtherStatic(i * 2) == 555
    // The virtual call `OtherStatic(i * 2)` must resolve through TestInfo's
    // class path to `TestInfo.OtherStatic`.
    let object = vm.create_object_with_class("TestInfo instance", "TestInfo");
    vm.set_class_default("TestInfo", 491, Value::Int(777)); // TestInfo.xnum

    let (unit, function) = vm
        .find_function("TestInfo.TestStatic")
        .expect("TestInfo.TestStatic should exist");
    let result = vm
        .exec_function(unit, function, Some(object), vec![Value::Int(123)])
        .expect("execute TestInfo.TestStatic")
        .return_value;

    // `TestStatic` has no explicit return expression, so its `ReturnValue`
    // slot keeps the default (Int 0).
    assert_eq!(result, Value::Int(0));
    assert_eq!(vm.assertion_failures(), 0);
}
