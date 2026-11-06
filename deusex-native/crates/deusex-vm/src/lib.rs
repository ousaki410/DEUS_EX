//! Deus Ex Native - UnrealScript bytecode virtual machine.
//!
//! This crate executes the decoded bytecode produced by `deusex-bytecode`
//! directly, without decompiling to Rust first. That keeps execution faithful
//! to the original engine (including computed jumps and switch dispatch) and
//! avoids the need to recover structured control flow.
//!
//! ```no_run
//! use deusex_vm::{PackageUnit, Value, Vm};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let unit = PackageUnit::load("/path/to/Core.u")?;
//! let mut vm = Vm::new();
//! let index = vm.add_unit(unit);
//! let object = vm.create_object("HelloWorldCommandlet");
//! vm.set_object_property(object, "intparm", Value::Int(0));
//! vm.set_object_property(object, "strparm", Value::String(String::new()));
//!
//! let (unit, function) = vm.find_function("HelloWorldCommandlet.Main").unwrap();
//! vm.exec_function(unit, function, Some(object), vec![Value::String(String::new())])?;
//! assert_eq!(vm.logs(), ["Hello, world!"]);
//! # Ok(())
//! # }
//! ```

pub mod error;
pub mod native;
pub mod unit;
pub mod value;
pub mod vm;

pub use error::VmError;
pub use native::{NativeContext, NativeFn, NativeRegistry};
pub use unit::PackageUnit;
pub use value::Value;
pub use vm::{ExecOutcome, LatentQueueEntry, ScriptObject, Vm};
