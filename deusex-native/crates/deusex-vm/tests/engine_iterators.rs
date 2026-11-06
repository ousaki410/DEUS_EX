//! Iterator/foreach support, using DeusEx.u's WanderPoint.PreBeginPlay.

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
        if system.join("DeusEx.u").is_file() {
            return Some(system);
        }
    }
    None
}

fn deusex_vm() -> Option<Vm> {
    let system = system_dir()?;
    let core = PackageUnit::load(system.join("Core.u")).expect("load Core.u");
    let engine = PackageUnit::load(system.join("Engine.u")).expect("load Engine.u");
    let deusex = PackageUnit::load(system.join("DeusEx.u")).expect("load DeusEx.u");
    let mut vm = Vm::new();
    vm.add_unit(core);
    vm.add_unit(engine);
    vm.add_unit(deusex);
    Some(vm)
}

#[test]
fn iterates_all_actors_with_tag_filter() {
    let Some(mut vm) = deusex_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };

    let wander = vm.create_object_with_class("WanderPoint instance", "WanderPoint");
    vm.set_object_property(wander, "gazeTag", Value::Name("Foo".into()));
    // Keep Actor.PreBeginPlay from taking the `Destroy()` branch.
    vm.set_object_property(wander, "bGameRelevant", Value::Bool(true));

    let decoration = vm.create_object_with_class("Decoration instance", "Decoration");
    vm.set_object_property(decoration, "Tag", Value::Name("Foo".into()));

    let pawn = vm.create_object_with_class("Pawn instance", "Pawn");
    vm.set_object_property(pawn, "Tag", Value::Name("Bar".into()));

    let trigger = vm.create_object_with_class("Trigger instance", "Trigger");
    vm.set_object_property(trigger, "Tag", Value::Name("Foo".into()));

    // `PreBeginPlay` performs `foreach AllActors(class'Actor', gazeItem,
    // gazeTag) break;` so the first matching actor wins.
    let (unit, function) = vm
        .find_function("DeusEx.WanderPoint.PreBeginPlay")
        .expect("DeusEx.WanderPoint.PreBeginPlay should exist");
    vm.exec_function(unit, function, Some(wander), Vec::new())
        .expect("execute DeusEx.WanderPoint.PreBeginPlay");

    assert_eq!(
        vm.object_property(wander, "gazeItem"),
        Some(&Value::Object(decoration))
    );
    assert_eq!(vm.assertion_failures(), 0);
}
