//! deusex-audio - OpenAL Soft + libopenmpt audio backend skeleton.
//!
//! When the system ships the OpenAL development library, enable the matching
//! feature:
//!   cargo build -p deusex-audio --features openal
//! Otherwise this crate stays a pure-Rust placeholder and compiles without any
//! native audio library.
//!
//! libopenmpt 的 Rust binding crate 名在 crates.io 上未确认（无 `libopenmpt` crate），
//! 流式 `.umx` 模块播放待后续接入。

use tracing::info;

/// Handle to an initialized OpenAL device and context.
///
/// The raw `ALCdevice`/`ALCcontext` pointers are owned and destroyed on drop.
#[cfg(feature = "openal")]
pub struct AudioContext {
    device: *mut openal_sys::ALCdevice,
    context: *mut openal_sys::ALCcontext,
}

#[cfg(feature = "openal")]
impl AudioContext {
    /// Open the default OpenAL device and create a context for it, making it
    /// current on this thread.
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        unsafe {
            let device = openal_sys::alcOpenDevice(std::ptr::null());
            if device.is_null() {
                return Err("openal: alcOpenDevice failed".into());
            }
            let context = openal_sys::alcCreateContext(device, std::ptr::null());
            if context.is_null() {
                openal_sys::alcCloseDevice(device);
                return Err("openal: alcCreateContext failed".into());
            }
            if openal_sys::alcMakeContextCurrent(context) == 0 {
                openal_sys::alcDestroyContext(context);
                openal_sys::alcCloseDevice(device);
                return Err("openal: alcMakeContextCurrent failed".into());
            }
            info!("audio: initialized OpenAL device/context");
            Ok(AudioContext { device, context })
        }
    }
}

#[cfg(feature = "openal")]
impl Drop for AudioContext {
    fn drop(&mut self) {
        unsafe {
            if !self.context.is_null() {
                openal_sys::alcMakeContextCurrent(std::ptr::null_mut());
                openal_sys::alcDestroyContext(self.context);
            }
            if !self.device.is_null() {
                openal_sys::alcCloseDevice(self.device);
            }
        }
    }
}

/// 当 `openal` feature 未启用时的占位。
#[cfg(not(feature = "openal"))]
pub struct AudioContext;

#[cfg(not(feature = "openal"))]
impl AudioContext {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Err("deusex-audio: `openal` feature is disabled".into())
    }
}

pub fn init() {
    info!("Initializing deusex-audio");
}
