//! deusex-game - 游戏主逻辑与主循环.
//!
//! 提供游戏世界的 tick 更新、场景管理、以及与 renderer / input / audio
//! 子系统的集成入口。当前处于 Phase 2 实现阶段，优先构建可运行的
//! 最小游戏循环（可显示画面、响应退出）。

use std::time::{Duration, Instant};

use deusex_audio::AudioContext;
use deusex_input::InputState;
use deusex_renderer::{Renderer, SpriteDraw, SpriteRect, TextureId};
use tracing::{debug, info, warn};

/// 传递给场景的上下文快照，避免 `&mut Game` 的双重借用问题。
#[derive(Debug, Clone, Copy)]
pub struct GameCtx {
    /// 累计运行时间（秒）。
    pub elapsed: f64,
    /// 固定 tick 间隔（秒）。
    pub tick_dt: f64,
}

/// 游戏主状态，持有每帧需要的子系统引用与游戏时间。
pub struct Game {
    /// 累计运行时间（秒）。
    pub elapsed: f64,
    /// 上一帧的时间点，用于计算 delta time。
    last_frame: Instant,
    /// 固定 tick 间隔（60 Hz）。
    tick_interval: Duration,
    /// 未消耗的累积时间，用于固定步长 tick。
    accumulator: Duration,
    /// 是否已请求退出。
    quit_requested: bool,
    /// 场景栈（顶部为当前激活场景）。
    scenes: Vec<Box<dyn Scene>>,
    /// OpenAL 音频上下文（可选）。
    pub audio: Option<AudioContext>,
}

/// 场景接口：每个游戏状态（菜单、关卡、加载等）实现此 trait。
///
/// 所有方法接收 `GameCtx`（只读快照）而非 `&mut Game`，以避免
/// 借用检查器对 `self.scenes` 和 `self` 的双重可变借用。
pub trait Scene: std::fmt::Debug {
    /// 进入场景时调用。
    fn enter(&mut self, _ctx: GameCtx) {}
    /// 退出场景时调用。
    fn exit(&mut self, _ctx: GameCtx) {}
    /// 固定步长更新，dt 为秒。
    fn tick(&mut self, _dt: f64, _ctx: GameCtx) {}
    /// 变步长渲染，alpha 为上一帧在 tick 之间的插值因子 [0,1]。
    fn render(&mut self, _alpha: f32, _renderer: &mut Renderer) {}
    /// 处理输入，返回 true 表示事件已被消费。
    fn input(&mut self, _input: &InputState) -> bool {
        false
    }
}

impl Game {
    /// 创建新游戏实例。
    pub fn new() -> Self {
        info!("deusex-game: creating game instance");
        Self {
            elapsed: 0.0,
            last_frame: Instant::now(),
            tick_interval: Duration::from_secs_f64(1.0 / 60.0),
            accumulator: Duration::ZERO,
            quit_requested: false,
            scenes: Vec::new(),
            audio: None,
        }
    }

    /// 使用 OpenAL 音频上下文创建游戏实例。
    pub fn with_audio(audio: AudioContext) -> Self {
        let mut g = Self::new();
        g.audio = Some(audio);
        g
    }

    /// 推入一个新场景并激活。
    pub fn push_scene(&mut self, scene: Box<dyn Scene>) {
        // 退出当前场景（先 pop，避免双重可变借用）。
        if let Some(mut current) = self.scenes.pop() {
            current.exit(GameCtx {
                elapsed: self.elapsed,
                tick_dt: self.tick_interval.as_secs_f64(),
            });
            self.scenes.push(current);
        }
        self.scenes.push(scene);
        // 激活新场景。
        if let Some(mut current) = self.scenes.last_mut() {
            current.enter(GameCtx {
                elapsed: self.elapsed,
                tick_dt: self.tick_interval.as_secs_f64(),
            });
        }
        info!("deusex-game: scene pushed, {} active", self.scenes.len());
    }

    /// 弹出当前场景并返回。
    pub fn pop_scene(&mut self) -> Option<Box<dyn Scene>> {
        if let Some(mut current) = self.scenes.pop() {
            current.exit(GameCtx {
                elapsed: self.elapsed,
                tick_dt: self.tick_interval.as_secs_f64(),
            });
            if let Some(mut next) = self.scenes.last_mut() {
                next.enter(GameCtx {
                    elapsed: self.elapsed,
                    tick_dt: self.tick_interval.as_secs_f64(),
                });
            }
            info!("deusex-game: scene popped, {} active", self.scenes.len());
            return Some(current);
        }
        None
    }

    /// 替换当前场景。
    pub fn replace_scene(&mut self, scene: Box<dyn Scene>) {
        if let Some(mut current) = self.scenes.pop() {
            current.exit(GameCtx {
                elapsed: self.elapsed,
                tick_dt: self.tick_interval.as_secs_f64(),
            });
        }
        self.scenes.push(scene);
        if let Some(mut current) = self.scenes.last_mut() {
            current.enter(GameCtx {
                elapsed: self.elapsed,
                tick_dt: self.tick_interval.as_secs_f64(),
            });
        }
    }

    /// 请求退出游戏主循环。
    pub fn request_quit(&mut self) {
        self.quit_requested = true;
    }

    /// 是否已请求退出。
    pub fn is_quit_requested(&self) -> bool {
        self.quit_requested
    }

    /// 返回当前累计运行时间（秒）。
    pub fn elapsed(&self) -> f64 {
        self.elapsed
    }

    /// 单帧更新：计算 delta time、运行固定步长 tick、处理输入、渲染。
    ///
    /// 返回 `false` 表示主循环应结束。
    pub fn update(&mut self, input: &InputState, renderer: &mut Renderer) -> bool {
        let now = Instant::now();
        let frame_time = now.duration_since(self.last_frame);
        self.last_frame = now;

        // 限制最大 delta time，防止窗口拖拽后爆炸。
        let frame_time = frame_time.min(Duration::from_secs_f64(0.25));
        self.elapsed += frame_time.as_secs_f64();

        // 固定步长 tick（60 Hz）。
        let tick_dt = self.tick_interval.as_secs_f64();
        self.accumulator += frame_time;
        while self.accumulator >= self.tick_interval {
            if let Some(mut scene) = self.scenes.pop() {
                scene.tick(tick_dt, GameCtx {
                    elapsed: self.elapsed,
                    tick_dt,
                });
                self.scenes.push(scene);
            }
            self.accumulator -= self.tick_interval;
        }

        // 输入分发给当前场景。
        if input.quit_requested {
            self.request_quit();
        } else if let Some(mut scene) = self.scenes.pop() {
            let _consumed = scene.input(input);
            self.scenes.push(scene);
        }

        // 渲染（变步长，带插值 alpha）。
        let alpha = (self.accumulator.as_secs_f64() / self.tick_interval.as_secs_f64()) as f32;
        renderer.begin_frame();
        if let Some(mut scene) = self.scenes.pop() {
            scene.render(alpha, renderer);
            self.scenes.push(scene);
        } else {
            // 无场景时绘制清屏色（由 renderer::render 内部 clear 完成）。
        }
        if let Err(e) = renderer.render() {
            warn!("deusex-game: render error: {e}");
            return false;
        }

        !self.quit_requested
    }

    /// 运行游戏主循环，直到 `request_quit` 被调用或 `update` 返回 `false`。
    ///
    /// 这是一个阻塞调用，适合在 `tokio::main` 或 SDL3 事件循环中使用。
    pub fn run_loop<F>(&mut self, mut input_pump: F, renderer: &mut Renderer)
    where
        F: FnMut(&mut InputState),
    {
        info!("deusex-game: entering main loop");
        while !self.is_quit_requested() {
            let mut input = InputState::default();
            input_pump(&mut input);
            if !self.update(&input, renderer) {
                break;
            }
        }
        info!("deusex-game: main loop exited");
    }
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct CounterScene {
        ticks: u64,
    }

    impl std::fmt::Debug for CounterScene {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("CounterScene").field("ticks", &self.ticks).finish()
        }
    }

    impl Scene for CounterScene {
        fn tick(&mut self, _dt: f64, _ctx: GameCtx) {
            self.ticks += 1;
        }
    }

    #[test]
    fn game_quit_requested() {
        let mut game = Game::new();
        assert!(!game.is_quit_requested());
        game.request_quit();
        assert!(game.is_quit_requested());
    }

    #[test]
    fn game_push_pop_scene() {
        let mut game = Game::new();
        assert_eq!(game.scenes.len(), 0);

        game.push_scene(Box::new(CounterScene { ticks: 0 }));
        assert_eq!(game.scenes.len(), 1);

        game.pop_scene();
        assert_eq!(game.scenes.len(), 0);
    }

    #[test]
    fn game_replace_scene() {
        let mut game = Game::new();
        game.push_scene(Box::new(CounterScene { ticks: 0 }));
        assert_eq!(game.scenes.len(), 1);

        game.replace_scene(Box::new(CounterScene { ticks: 0 }));
        assert_eq!(game.scenes.len(), 1);
    }
}
