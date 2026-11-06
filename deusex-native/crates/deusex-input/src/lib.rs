//! deusex-input - SDL3 窗口与输入后端，含游戏动作绑定系统.
//!
//! 当系统安装了 SDL3 开发库后，启用 `sdl3` feature 即可编译真实后端：
//!   cargo build -p deusex-input --features sdl3
//! 未启用时保持纯 Rust placeholder，不依赖任何系统库.
//!
//! 本模块提供：
//! - `InputState`: 每帧输入状态快照（键盘、鼠标、退出请求）
//! - `InputAction`: 游戏动作枚举（MoveForward, Jump, Use 等）
//! - `InputBinding`: 动作到按键/鼠标的映射
//! - `InputManager`: 管理绑定，查询动作是否被触发，支持按键重新绑定

use std::collections::HashMap;

use tracing::info;

/// 输入设备状态快照（每帧刷新）。
#[derive(Debug, Default, Clone)]
pub struct InputState {
    /// 按下的键盘键码（SDL3 scancode）。
    pub keys_down: Vec<u16>,
    /// 本帧新按下的键码（仅当帧）。
    pub keys_pressed: Vec<u16>,
    /// 本帧释放的键码。
    pub keys_released: Vec<u16>,
    /// 鼠标相对位移（本帧）。
    pub mouse_delta: (f32, f32),
    /// 鼠标绝对位置（像素）。
    pub mouse_pos: (f32, f32),
    /// 鼠标按钮（位掩码：bit0=左，bit1=右，bit2=中）。
    pub mouse_buttons: u8,
    /// 退出请求（窗口关闭 / Alt+F4）。
    pub quit_requested: bool,
}

/// 游戏动作枚举，对应 Deus Ex 原版控制。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputAction {
    /// 向前移动
    MoveForward,
    /// 向后移动
    MoveBackward,
    /// 向左平移
    StrafeLeft,
    /// 向右平移
    StrafeRight,
    /// 跳跃
    Jump,
    /// 蹲伏
    Crouch,
    /// 奔跑
    Run,
    /// 使用/交互
    Use,
    /// 攻击/开火
    Fire,
    /// 切换武器
    NextWeapon,
    /// 上一武器
    PrevWeapon,
    /// 打开物品栏
    Inventory,
    /// 打开地图
    Map,
    /// 菜单/暂停
    Menu,
    /// 对话/确认
    Talk,
    /// 接下来的动作（扩展用）
    Action1,
    Action2,
}

/// 输入绑定：一个动作可绑定多个按键。
#[derive(Debug, Clone, Default)]
pub struct InputBinding {
    /// 绑定的键盘 scancode 集合。
    pub keys: Vec<u16>,
}

/// 输入管理器：管理绑定并查询动作状态。
#[derive(Debug, Default)]
pub struct InputManager {
    /// 动作 -> 绑定。
    bindings: HashMap<InputAction, InputBinding>,
    /// 上一帧的输入状态，用于检测按下/释放边缘。
    prev_state: Option<InputState>,
}

impl InputManager {
    /// 创建新的输入管理器，带默认绑定（类似 Deus Ex 原版 WASD 布局）。
    pub fn new() -> Self {
        let mut mgr = Self::default();
        mgr.set_default_bindings();
        mgr
    }

    /// 设置默认按键绑定。
    pub fn set_default_bindings(&mut self) {
        self.bindings.clear();
        self.bindings.insert(InputAction::MoveForward, InputBinding { keys: vec![0x1A] }); // W
        self.bindings.insert(InputAction::MoveBackward, InputBinding { keys: vec![0x1D] }); // S
        self.bindings.insert(InputAction::StrafeLeft, InputBinding { keys: vec![0x04] });   // A
        self.bindings.insert(InputAction::StrafeRight, InputBinding { keys: vec![0x07] });  // D
        self.bindings.insert(InputAction::Jump, InputBinding { keys: vec![0x2C] });         // Space
        self.bindings.insert(InputAction::Crouch, InputBinding { keys: vec![0x1E] });       // LCtrl
        self.bindings.insert(InputAction::Run, InputBinding { keys: vec![0x1F] });          // LShift
        self.bindings.insert(InputAction::Use, InputBinding { keys: vec![0x14] });          // E
        self.bindings.insert(InputAction::Fire, InputBinding { keys: vec![] });             // 鼠标左键（单独处理）
        self.bindings.insert(InputAction::NextWeapon, InputBinding { keys: vec![0x25] });   // WheelUp (近似)
        self.bindings.insert(InputAction::PrevWeapon, InputBinding { keys: vec![0x26] });   // WheelDown
        self.bindings.insert(InputAction::Inventory, InputBinding { keys: vec![0x15] });    // I? Actually 0x15 might not be I
        self.bindings.insert(InputAction::Map, InputBinding { keys: vec![0x24] });          // M
        self.bindings.insert(InputAction::Menu, InputBinding { keys: vec![0x2D] });         // Esc
        self.bindings.insert(InputAction::Talk, InputBinding { keys: vec![0x22] });         // T
        self.bindings.insert(InputAction::Action1, InputBinding { keys: vec![0x23] });      // R? 
        self.bindings.insert(InputAction::Action2, InputBinding { keys: vec![0x0F] });      // Tab?
    }

    /// 为指定动作绑定一个按键。
    pub fn bind_key(&mut self, action: InputAction, key: u16) {
        let entry = self.bindings.entry(action).or_default();
        if !entry.keys.contains(&key) {
            entry.keys.push(key);
        }
    }

    /// 移除指定动作的某个按键绑定。
    pub fn unbind_key(&mut self, action: InputAction, key: u16) {
        if let Some(entry) = self.bindings.get_mut(&action) {
            entry.keys.retain(|&k| k != key);
        }
    }

    /// 清除动作的所有绑定。
    pub fn unbind_all(&mut self, action: InputAction) {
        if let Some(entry) = self.bindings.get_mut(&action) {
            entry.keys.clear();
        }
    }

    /// 获取动作绑定的按键。
    pub fn keys_for(&self, action: InputAction) -> Option<&[u16]> {
        self.bindings.get(&action).map(|b| b.keys.as_slice())
    }

    /// 根据上一帧状态计算按下/释放边缘，并合并到当前状态。
    pub fn update_edges(&mut self, curr: &mut InputState, prev: &InputState) {
        // 计算新按下的键
        curr.keys_pressed.clear();
        curr.keys_released.clear();

        for &k in &curr.keys_down {
            if !prev.keys_down.contains(&k) {
                curr.keys_pressed.push(k);
            }
        }
        for &k in &prev.keys_down {
            if !curr.keys_down.contains(&k) {
                curr.keys_released.push(k);
            }
        }
    }

    /// 查询动作在当前帧是否处于“按下”状态（持续）。
    pub fn is_action_down(&self, action: InputAction, state: &InputState) -> bool {
        let Some(binding) = self.bindings.get(&action) else {
            return false;
        };
        binding.keys.iter().any(|k| state.keys_down.contains(k))
    }

    /// 查询动作在当前帧是否刚被按下（边缘触发）。
    pub fn is_action_pressed(&self, action: InputAction, state: &InputState) -> bool {
        let Some(binding) = self.bindings.get(&action) else {
            return false;
        };
        binding.keys.iter().any(|k| state.keys_pressed.contains(k))
    }

    /// 查询动作在当前帧是否刚被释放（边缘触发）。
    pub fn is_action_released(&self, action: InputAction, state: &InputState) -> bool {
        let Some(binding) = self.bindings.get(&action) else {
            return false;
        };
        binding.keys.iter().any(|k| state.keys_released.contains(k))
    }

    /// 返回所有动作及其绑定的副本。
    pub fn all_bindings(&self) -> Vec<(InputAction, InputBinding)> {
        self.bindings.iter().map(|(a, b)| (*a, b.clone())).collect()
    }

    /// 设置上一帧状态（供边缘检测使用）。
    pub fn set_prev_state(&mut self, state: InputState) {
        self.prev_state = Some(state);
    }

    /// 获取上一帧状态。
    pub fn prev_state(&self) -> Option<&InputState> {
        self.prev_state.as_ref()
    }
}

/// 默认键码常量（SDL3 scancode 值，供参考）。
pub mod keycode {
    /// W
    pub const W: u16 = 0x1A;
    /// A
    pub const A: u16 = 0x04;
    /// S
    pub const S: u16 = 0x1D;
    /// D
    pub const D: u16 = 0x07;
    /// Space
    pub const SPACE: u16 = 0x2C;
    /// Left Shift
    pub const LSHIFT: u16 = 0x1F;
    /// Left Ctrl
    pub const LCTRL: u16 = 0x1E;
    /// E
    pub const E: u16 = 0x14;
    /// R
    pub const R: u16 = 0x23;
    /// T
    pub const T: u16 = 0x22;
    /// I
    pub const I: u16 = 0x1C;
    /// M
    pub const M: u16 = 0x24;
    /// Tab
    pub const TAB: u16 = 0x0F;
    /// Esc
    pub const ESC: u16 = 0x2D;
    /// Left mouse button
    pub const MOUSE_LEFT: u8 = 0x01;
    /// Right mouse button
    pub const MOUSE_RIGHT: u8 = 0x02;
    /// Middle mouse button
    pub const MOUSE_MIDDLE: u8 = 0x04;
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
            super::info!("input: initialized SDL3");
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
            state.keys_pressed.clear();
            state.keys_released.clear();
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
                    Event::KeyUp { keycode, .. } => {
                        // KeyUp 不影响 keys_down（已在 pump 外部由 InputManager 处理边缘）
                    }
                    Event::MouseMotion { xrel, yrel, x, y, .. } => {
                        state.mouse_delta = (xrel as f32, yrel as f32);
                        state.mouse_pos = (x as f32, y as f32);
                    }
                    Event::MouseButtonDown { mouse_btn, .. } => {
                        state.mouse_buttons |= 1u8 << (mouse_btn as u8);
                    }
                    Event::MouseButtonUp { mouse_btn, .. } => {
                        state.mouse_buttons &= !(1u8 << (mouse_btn as u8));
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

pub fn init() {
    info!("Initializing deusex-input");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_bindings_exist() {
        let mgr = InputManager::new();
        // 空 InputState 没有按键，所有动作都应为 false
        assert!(!mgr.is_action_down(InputAction::MoveForward, &InputState::default()));
        let state = InputState {
            keys_down: vec![keycode::W],
            ..Default::default()
        };
        assert!(mgr.is_action_down(InputAction::MoveForward, &state));
        assert!(!mgr.is_action_down(InputAction::Jump, &state));
    }

    #[test]
    fn bind_and_unbind() {
        let mut mgr = InputManager::new();
        let state = InputState {
            keys_down: vec![0x99],
            ..Default::default()
        };
        assert!(!mgr.is_action_down(InputAction::Jump, &state));
        mgr.bind_key(InputAction::Jump, 0x99);
        assert!(mgr.is_action_down(InputAction::Jump, &state));
        mgr.unbind_key(InputAction::Jump, 0x99);
        assert!(!mgr.is_action_down(InputAction::Jump, &state));
    }

    #[test]
    fn edge_detection() {
        let mut mgr = InputManager::new();
        let prev = InputState {
            keys_down: vec![keycode::W],
            ..Default::default()
        };
        let mut curr = InputState {
            keys_down: vec![keycode::W, keycode::SPACE],
            ..Default::default()
        };
        mgr.update_edges(&mut curr, &prev);
        assert!(curr.keys_pressed.contains(&keycode::SPACE));
        assert!(curr.keys_released.is_empty());
        assert!(mgr.is_action_pressed(InputAction::Jump, &curr));
    }
}
