//! Deus Ex / UE1 UnrealScript bytecode decoding.
//!
//! The package reader lives in `deusex-package`; this crate extracts
//! `UFunction` objects and decodes their scripts into an expression tree.
//! The expression tree preserves *normalized* byte offsets, which is the
//! coordinate system used by jump, case, and label offsets.

pub mod cfg;
pub mod decompile;
pub mod function;
pub mod instructions;
pub mod parser;
pub mod pseudocode;

pub use cfg::{build_cfg, BasicBlock, Cfg, Edge, EdgeKind, Terminator};
pub use decompile::{
    decompile_function, decompile_package, decompile_to_rust_module, DecompileError,
};
pub use function::{function_flags, Expr, ExprKind, Function};
pub use pseudocode::{render_function as render_pseudocode, NativeTable};
pub use instructions::Token;
pub use parser::{
    decode_script, parse_package_functions, read_function, BytecodeError,
};

/// Bytecode version matches the Deus Ex package file version.
pub const BYTECODE_VERSION: u32 = 68;
