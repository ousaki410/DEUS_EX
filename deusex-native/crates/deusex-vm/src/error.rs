//! VM error types.

use deusex_bytecode::BytecodeError;
use deusex_package::PackageError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VmError {
    #[error("package error: {0}")]
    Package(#[from] PackageError),

    #[error("bytecode error: {0}")]
    Bytecode(#[from] BytecodeError),

    #[error("function `{0}` not found")]
    FunctionNotFound(String),

    #[error("unresolved native index {index} in `{function}`")]
    UnresolvedNative { function: String, index: u32 },

    #[error("native `{0}` is not registered")]
    UnknownNative(String),

    #[error("native `{name}` failed: {message}")]
    Native { name: String, message: String },

    #[error("unsupported expression `{kind}` in `{function}`")]
    UnsupportedExpression { kind: String, function: String },

    #[error("assertion failed in `{function}` at line {line}")]
    AssertionFailed { function: String, line: u16 },

    #[error("invalid jump target 0x{offset:04X} in `{function}`")]
    InvalidJump { offset: u32, function: String },

    #[error("step limit exceeded in `{0}`")]
    StepLimit(String),

    #[error("call depth limit exceeded in `{0}`")]
    CallDepth(String),
}

impl VmError {
    pub fn native(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Native {
            name: name.into(),
            message: message.into(),
        }
    }
}
