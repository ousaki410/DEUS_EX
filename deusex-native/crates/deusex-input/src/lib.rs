//! deusex-input - SDL3 窗口与输入后端骨架.
//!
//! 当系统安装了 SDL3 开发库后，启用 `sdl3` feature 即可编译真实后端：
//!   cargo build -p deusex-input --features sdl3
//! 未启用时保持纯 Rust placeholder，不依赖任何系统库。

use tracing::info;

/// 输入设备状态快照（每帧刷新）。
#[derive(Debug, Default, Clone)]
pub struct InputState {
    /// 按下的键盘键码（SDL3 scancode）。
    pub keys_down: Vec<u16>,
    /// 鼠标相对位移（本帧）。
    pub mouse_delta: (f32, f32),
    /// 鼠标绝对位置（像素）。
    pub mouse_pos: (f32, f32),
    /// 退出请求（窗口关闭 / Alt+F4）。
    pub quit_requested: bool,
}

pub fn init() {
    info!("Initializing deusex-input");
}

/// SDL3 驱动的窗口与输入上下文。
///
/// 仅在 `sdl3` feature 启用时可用。
#[cfg(feature = "sdl3")]
pub mod sdl {
    use super::InputState;
    use sdl3::{event::Event, EventPump, Sdl, VideoSubsystem};

    /// SDL3 核心上下文（已初始化事件与视频子系统）。
    pub struct SdlContext {
        pub sdl: Sdl,
        pub video: VideoSubsystem,
        pub event_pump: EventPump,
    }

    impl SdlContext {
        /// 初始化 SDL3（事件 + 视频子系统）。
        pub fn init() -> Result<Self, Box<dyn std::error::Error>> {
            let sdl = sdl3::init()?;
            let video = sdl.video()?;
            let event_pump = sdl.event_pump()?;
            super::init();
            Ok(SdlContext { sdl, video, event_pump })
        }

        /// 创建一个窗口并返回窗口句柄与原生窗口句柄（供 wgpu 创建 surface）。
        pub fn create_window(
            &self,
            title: &str,
            width: u32,
            height: u32,
        ) -> Result<SdlWindow, Box<dyn std::error::Error>> {
            let window = self.video.window(title, width, height).build()?;

            Ok(SdlWindow { window })
        }

        /// 轮询一帧的输入事件，填充 `InputState`。
        pub fn pump(&mut self, state: &mut InputState) {
            state.keys_down.clear();
            state.mouse_delta = (0.0, 0.0);
            state.quit_requested = false;

            for event in self.event_pump.poll_iter() {
                match event {
                    Event::Quit { .. } => state.quit_requested = true,
                    Event::KeyDown { keycode, .. } => {
                        if let Some(kc) = keycode {
                            state.keys_down.push(kc as u16);
                        }
                    }
                    Event::MouseMotion { xrel, yrel, x, y, .. } => {
                        state.mouse_delta = (xrel as f32, yrel as f32);
                        state.mouse_pos = (x as f32, y as f32);
                    }
                    _ => {}
                }
            }
        }
    }

    /// SDL3 窗口封装。持有窗口生命周期；窗口句柄可通过
    /// `sdl3::video::Window`（实现 wgpu 的 `WindowHandle` trait）直接交给
    /// `wgpu::Instance::create_surface`。
    pub struct SdlWindow {
        pub window: sdl3::video::Window,
    }
}

/// 当 `sdl3` feature 未启用时的占位：无法创建真实窗口。
#[cfg(not(feature = "sdl3"))]
pub mod sdl {
    pub struct SdlContext;
    pub struct SdlWindow;

    impl SdlContext {
        pub fn init() -> Result<Self, Box<dyn std::error::Error>> {
            Err("deusex-input: `sdl3` feature is disabled".into())
        }
    }

    impl SdlWindow {}
}
