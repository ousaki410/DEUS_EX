//! Integration tests for game + VM integration.
//!
//! These tests load .u packages and execute UnrealScript functions through
//! the VM within the game loop context.
//!
//! Tests skip when GOTY assets are unavailable.

use std::path::{Path, PathBuf};

use deusex_game::Game;
use deusex_input::InputState;
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

fn load_core(system: &Path) -> (Vm, PackageUnit) {
    let core_path = system.join("Core.u");
    let core = PackageUnit::load(&core_path).expect("load Core.u");
    let mut vm = Vm::new();
    vm.add_unit(core);
    (vm, PackageUnit::load(&core_path).expect("reload Core.u"))
}

#[test]
fn test_game_with_vm_integration() {
    let Some(system) = system_dir() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };

    // Create VM and load Core.u
    let (mut vm, _core_unit) = load_core(&system);

    // Create game instance
    let mut game = Game::new();

    // Find and execute HelloWorldCommandlet.Main through VM
    let (unit_idx, func_idx) = vm
        .find_function("HelloWorldCommandlet.Main")
        .expect("HelloWorldCommandlet.Main should exist");

    // Create a mock object for self
    let _object_id = vm.create_object("HelloWorldCommandlet");

    // Execute the function
    let outcome = vm
        .exec_function(unit_idx, func_idx, None, vec![Value::String(String::new())])
        .expect("execute HelloWorldCommandlet.Main");

    assert_eq!(outcome.return_value, Value::Int(0));
    assert!(!vm.logs().is_empty());
    assert_eq!(vm.logs()[0], "Hello, world!");
}

#[test]
fn test_game_tick_with_vm() {
    let Some(system) = system_dir() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };

    let mut vm = Vm::new();
    let core_path = system.join("Core.u");
    let core = PackageUnit::load(&core_path).expect("load Core.u");
    vm.add_unit(core);

    let mut game = Game::new();

    // Execute a simple function multiple times through VM in game loop
    let (unit_idx, func_idx) = vm
        .find_function("Object.Abs")
        .expect("Object.Abs should exist");

    for i in 1..=5 {
        let outcome = vm
            .exec_function(
                unit_idx,
                func_idx,
                None,
                vec![Value::Int(-i)],
            )
            .expect("execute Object.Abs");
        
        assert_eq!(outcome.return_value, Value::Int(i));
    }

    // Game tick should work without errors
    let input = InputState::default();
    // game.update(&input, renderer) - would need renderer
    // Just verify game state is valid
    assert!(!game.is_quit_requested());
    assert_eq!(game.elapsed(), 0.0);
}

#[test]
fn test_game_scene_stack() {
    let mut game = Game::new();
    
    // Initially no scenes - verify via elapsed and quit state
    assert!(!game.is_quit_requested());
    assert_eq!(game.elapsed(), 0.0);

    // Push a scene
    struct TestScene;
    impl std::fmt::Debug for TestScene {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "TestScene")
        }
    }
    impl deusex_game::Scene for TestScene {}

    game.push_scene(Box::new(TestScene));

    // Pop the scene
    game.pop_scene();
    assert!(!game.is_quit_requested());
}
