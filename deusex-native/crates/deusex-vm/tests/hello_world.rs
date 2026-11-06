//! Executes real UnrealScript bytecode from `Core.u`.
//!
//! Skipped when a local Deus Ex GOTY installation is unavailable. Game assets
//! are never redistributed with this repository.

use std::path::PathBuf;

use deusex_vm::{PackageUnit, Value, Vm};

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

fn vm_with_core() -> Option<Vm> {
    let system = system_dir()?;
    let unit = PackageUnit::load(system.join("Core.u")).expect("load Core.u");
    assert!(
        unit.warnings.is_empty(),
        "unexpected package warnings: {:?}",
        unit.warnings
    );
    let mut vm = Vm::new();
    vm.add_unit(unit);
    Some(vm)
}

/// Create a `HelloWorldCommandlet` instance with explicit field values.
fn hello_world_object(vm: &mut Vm, intparm: i32, strparm: &str) -> u32 {
    let object = vm.create_object("HelloWorldCommandlet");
    vm.set_object_property(object, "HelloWorldCommandlet.intparm", Value::Int(intparm));
    vm.set_object_property(
        object,
        "HelloWorldCommandlet.strparm",
        Value::String(strparm.to_string()),
    );
    object
}

#[test]
fn executes_hello_world_commandlet_main() {
    let Some(mut vm) = vm_with_core() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };

    let object = hello_world_object(&mut vm, 0, "");
    let (unit, function) = vm
        .find_function("HelloWorldCommandlet.Main")
        .expect("HelloWorldCommandlet.Main should exist");
    let result = vm
        .exec_function(unit, function, Some(object), vec![Value::String(String::new())])
        .expect("execute HelloWorldCommandlet.Main")
        .return_value;

    assert_eq!(vm.logs(), ["Hello, world!"]);
    assert_eq!(result, Value::Int(0));
}

#[test]
fn follows_conditional_branches_and_reads_instance_variables() {
    let Some(mut vm) = vm_with_core() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };

    let object = hello_world_object(&mut vm, 7, "abc");
    let (unit, function) = vm
        .find_function("HelloWorldCommandlet.Main")
        .expect("HelloWorldCommandlet.Main should exist");
    let result = vm
        .exec_function(
            unit,
            function,
            Some(object),
            vec![Value::String("intparm".to_string())],
        )
        .expect("execute HelloWorldCommandlet.Main")
        .return_value;

    assert_eq!(
        vm.logs(),
        [
            "Hello, world!",
            "Command line parameters=intparm",
            "You specified intparm=7",
            "You specified strparm=abc",
        ]
    );
    assert_eq!(result, Value::Int(0));
}

#[test]
fn dispatches_native_functions_with_empty_bodies() {
    let Some(mut vm) = vm_with_core() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };

    let result = vm
        .call_function(
            "Object.Log",
            None,
            vec![Value::String("from native".to_string())],
        )
        .expect("Object.Log should dispatch to the native registry")
        .return_value;
    assert_eq!(result, Value::None);
    assert_eq!(vm.logs(), ["from native"]);
}

#[test]
fn rejects_unknown_functions() {    let Some(mut vm) = vm_with_core() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    let error = vm
        .call_function("DoesNotExist.Main", None, Vec::new())
        .expect_err("missing function should fail");
    assert!(error.to_string().contains("not found"), "{error}");
}
