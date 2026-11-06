//! Example: Inspect a Deus Ex package file
//!
//! Run with: cargo run --example inspect_package -- <path_to_package.u>

use deusex_package::inspect_package;
use std::env;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        eprintln!("Usage: {} <package.u>", args[0]);
        eprintln!("\nExample packages in the extracted game:");
        eprintln!("  Core.u");
        eprintln!("  Engine.u");
        eprintln!("  DeusEx.u");
        eprintln!("  DeusExCharacters.u");
        eprintln!("  DeusExItems.u");
        eprintln!("  DeusExSounds.u");
        eprintln!("  DeusExUI.u");
        std::process::exit(1);
    }
    
    let package_path = Path::new(&args[1]);
    
    if !package_path.exists() {
        eprintln!("File not found: {}", package_path.display());
        std::process::exit(1);
    }
    
    println!("Inspecting: {}", package_path.display());
    println!();
    
    // Use the convenience function
    inspect_package(package_path)?;
    
    // Or use the full API for more control
    // let mut package = UnrealPackage::open(package_path)?;
    // package.print_summary();
    
    Ok(())
}
