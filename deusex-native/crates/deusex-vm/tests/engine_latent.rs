//! Latent function support (e.g. `Sleep`, `FinishAnim`).
//!
//! Latent calls suspend the current frame and queue it for later resumption
//! via [`Vm::resume_latent`]. Since no Deus Ex script bytecode in the shipped
//! packages invokes a latent native, these tests exercise the resume machinery
//! directly and verify that the `LATENT` function flag is recognised.

use std::collections::HashMap;
use std::path::PathBuf;

use deusex_bytecode::function_flags::LATENT;
use deusex_vm::{LatentQueueEntry, PackageUnit, Vm};

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

fn latent_test_vm() -> Option<Vm> {
    let system = system_dir()?;
    let core = PackageUnit::load(system.join("Core.u")).expect("load Core.u");
    let engine = PackageUnit::load(system.join("Engine.u")).expect("load Engine.u");
    let mut vm = Vm::new();
    vm.add_unit(core);
    vm.add_unit(engine);
    Some(vm)
}

#[test]
fn latent_flag_is_recognised() {
    let vm = latent_test_vm().expect("vm should load");

    let (unit_index, function_index) = vm
        .find_function("Actor.Sleep")
        .expect("Actor.Sleep should exist");
    let unit = vm.unit(unit_index).expect("unit should exist");
    let flags = unit.functions[function_index].function_flags;
    assert!(
        flags & LATENT != 0,
        "Actor.Sleep should be flagged LATENT, got flags=0x{:X}",
        flags
    );
}

#[test]
fn resume_latent_executes_suspended_frame() {
    let mut vm = latent_test_vm().expect("vm should load");

    // `Commandlet.Main` is a trivial script function: it reads its Parms
    // native parameter and returns. It makes a good resume target.
    let (unit_index, function_index) = vm
        .find_function("Commandlet.Main")
        .expect("Commandlet.Main should exist");

    // Queue a latent resume entry pointing at Commandlet.Main from the start.
    vm.queue_latent(LatentQueueEntry {
        unit_index,
        function_index,
        self_object: None,
        resume_pc: 0,
        vars: HashMap::new(),
        return_ref: None,
        switch_value: None,
    });

    assert_eq!(vm.latent_queue_len(), 1);

    let outcome = vm.resume_latent().expect("resume latent should succeed");
    let Some(outcome) = outcome else {
        panic!("expected a resumed outcome, got None");
    };

    // Commandlet.Main completes without suspending again.
    assert!(!outcome.suspended);
    assert!(outcome.resume.is_none());
    assert_eq!(vm.latent_queue_len(), 0);
}

#[test]
fn resume_latent_empty_queue_returns_none() {
    let mut vm = latent_test_vm().expect("vm should load");

    assert_eq!(vm.latent_queue_len(), 0);
    let outcome = vm.resume_latent().expect("resume latent should succeed");
    assert!(outcome.is_none());
}
