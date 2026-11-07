//! deusex-launcher - entry point for the Deus Ex native engine.
//!
//! Build modes:
//!   * default (no features): resource-directory detector only. Safe to build
//!     on any machine, no native game library required.
//!   * `--features gui`: real engine entry point. Requires system
//!     libsdl3-dev (and wgpu with raw-window-handle support).
//!   * `--features gui,audio`: additionally requires libopenal-dev for
//!     OpenAL audio.
//!
//!   cargo run -p deusex-launcher --features gui
//!   cargo run -p deusex-launcher --features gui,audio

use std::path::{Path, PathBuf};

use clap::Parser;

/// Discovered game installation (GOTY or Revision).
#[derive(Debug)]
struct Installation {
    name: String,
    root: PathBuf,
    system: PathBuf,
}

#[derive(Parser, Debug)]
#[command(author, version, about = "Deus Ex Native launcher")]
struct Args {
    /// Force a specific game installation: "goty" or "revision".
    #[arg(long)]
    install: Option<String>,

    /// Explicit asset root directory (contains `app/system/` with the `.u` packages).
    #[arg(long)]
    asset_dir: Option<PathBuf>,

    /// Print the detected system directory and exit.
    #[arg(long)]
    print_system: bool,
}

fn locate_installation(args: &Args) -> Option<Installation> {
    if let Some(dir) = &args.asset_dir {
        let root = dir.canonicalize().ok()?;
        let system = if root.join("system").join("Core.u").is_file() {
            root.join("system")
        } else if root.join("Core.u").is_file() {
            root.clone()
        } else {
            return None;
        };
        return Some(Installation {
            name: "user-specified".to_string(),
            root,
            system,
        });
    }

    if let Some(name) = args.install.as_deref() {
        let candidates: Vec<PathBuf> = match name {
            "goty" => vec![
                PathBuf::from("./goty"),
                PathBuf::from("/home/ou/Projects/deus_ex_goty/goty"),
            ],
            "revision" => vec![
                PathBuf::from("./revision"),
                PathBuf::from("/tmp/opencode/revision_extract"),
            ],
            _ => return None,
        };
        for root in candidates {
            if let Some(system) = system_dir(&root) {
                return Some(Installation {
                    name: name.to_string(),
                    root,
                    system,
                });
            }
        }
        return None;
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("goty"));
        candidates.push(cwd.join("revision"));
    }
    if let Ok(env) = std::env::var("DEUSEX_GOTY_PATH") {
        candidates.push(PathBuf::from(env));
    }
    if let Ok(env) = std::env::var("DEUSEX_REVISION_PATH") {
        candidates.push(PathBuf::from(env));
    }
    candidates.push(PathBuf::from("/home/ou/Projects/deus_ex_goty/goty"));
    candidates.push(PathBuf::from("/tmp/opencode/revision_extract"));

    for root in candidates {
        if let Some(system) = system_dir(&root) {
            let name = if root.to_string_lossy().contains("revision") {
                "revision"
            } else {
                "goty"
            };
            return Some(Installation {
                name: name.to_string(),
                root,
                system,
            });
        }
    }

    None
}

fn system_dir(root: &Path) -> Option<PathBuf> {
    let direct = root.join("system");
    if direct.join("Core.u").is_file() {
        return Some(direct);
    }
    let app_system = root.join("app").join("system");
    if app_system.join("Core.u").is_file() {
        return Some(app_system);
    }
    None
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Mode A: no `gui` feature — resource-directory detector only.
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[cfg(not(feature = "gui"))]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let installation =
        locate_installation(&args).ok_or_else(|| "no Deus Ex installation found".to_string())?;

    println!("deusex-launcher: detected {} installation", installation.name);
    println!("  root:     {}", installation.root.display());
    println!("  system:   {}", installation.system.display());

    let u_count = installation
        .system
        .read_dir()
        .ok()
        .and_then(|entries| {
            Some(
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        e.path()
                            .extension()
                            .and_then(|s| s.to_str())
                            == Some("u")
                    })
                    .count(),
            )
        })
        .unwrap_or(0);
    println!("  packages: {u_count} .u files");

    if args.print_system {
        println!("{}", installation.system.display());
    } else {
        println!("launcher ready; enable `--features gui` for the engine entry point.");
    }
    Ok(())
}

#[cfg(not(feature = "gui"))]
fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        eprintln!(
            "  hint: pass --asset-dir <dir>, or --install goty|revision, \
             or set DEUSEX_GOTY_PATH / DEUSEX_REVISION_PATH"
        );
        std::process::exit(1);
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Mode B: `gui` feature — real engine entry point (winit + wgpu + OpenAL).
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[cfg(feature = "gui")]
use deusex_game::Game;
#[cfg(feature = "gui")]
use deusex_input::{InputManager, InputState};
#[cfg(feature = "gui")]
use deusex_renderer::{Gpu, Renderer};
#[cfg(feature = "gui")]
use tracing::info;
#[cfg(feature = "gui")]
use winit::application::ApplicationHandler;
#[cfg(feature = "gui")]
use winit::event::{KeyEvent, WindowEvent};
#[cfg(feature = "gui")]
use winit::event_loop::{ActiveEventLoop, EventLoop};
#[cfg(feature = "gui")]
use winit::keyboard::{Key, NamedKey};
#[cfg(feature = "gui")]
use winit::window::WindowAttributes;
#[cfg(feature = "audio")]
use deusex_audio::{AudioContext, AudioManager, Listener, Position};

#[cfg(feature = "gui")]
fn run() -> Result<(), Box<dyn std::error::Error>> {

    let args = Args::parse();
    let installation =
        locate_installation(&args).ok_or_else(|| "no Deus Ex installation found".to_string())?;

    println!(
        "deusex-launcher (gui): detected {} installation",
        installation.name
    );
    println!("  root:     {}", installation.root.display());
    println!("  system:   {}", installation.system.display());

    let event_loop = EventLoop::new()?;

    let mut app = App::new()?;

    event_loop.run_app(&mut app)?;
    Ok(())
}

/// GUI 应用程序状态，实现 winit 的 ApplicationHandler。
#[cfg(feature = "gui")]
struct App {
    window: Option<winit::window::Window>,
    renderer: Option<Renderer>,
    input_mgr: InputManager,
    game: Game,
    prev_state: InputState,
    #[cfg(feature = "audio")]
    audio_mgr: AudioManager,
}

#[cfg(feature = "gui")]
impl App {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            window: None,
            renderer: None,
            input_mgr: InputManager::new(),
            game: Game::new(),
            prev_state: InputState::default(),
            #[cfg(feature = "audio")]
            audio_mgr: AudioManager::new(AudioContext::new()?)?,
        })
    }
}

#[cfg(feature = "gui")]
impl ApplicationHandler<()> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attributes = WindowAttributes::default()
            .with_title("Deus Ex Native")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0));
        let window = event_loop.create_window(attributes).unwrap();

        let gpu = pollster::block_on(Gpu::new()).unwrap();
        let surface = gpu.instance.create_surface(&window).unwrap();
        // wgpu::Surface<'_> 借用 window，但 Renderer::new 要求 Surface<'static>。
        // 由于 window 和 renderer 都在 App 中，它们会同时销毁，使用 unsafe
        // 将 surface 生命周期提升为 'static 是安全的。
        let surface = unsafe { std::mem::transmute::<wgpu::Surface<'_>, wgpu::Surface<'static>>(surface) };
        let renderer = Renderer::new(
            gpu,
            surface,
            1280,
            720,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        )
        .unwrap();

        self.window = Some(window);
        self.renderer = Some(renderer);

        #[cfg(feature = "audio")]
        {
            info!("audio backend ready");
            self.audio_mgr
                .set_listener(Listener {
                    position: Position { x: 0.0, y: 0.0, z: 0.0 },
                    ..Default::default()
                });
        }
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => _event_loop.exit(),
            WindowEvent::KeyboardInput {
                event: KeyEvent { logical_key, .. },
                ..
            } => {
                if logical_key == Key::Named(NamedKey::Escape) {
                    _event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        let mut curr = InputState::default();

        self.input_mgr.update_edges(&mut curr, &self.prev_state);
        self.prev_state = curr.clone();

        if self
            .input_mgr
            .is_action_pressed(deusex_input::InputAction::Menu, &curr)
        {
            info!("input: Menu pressed, quitting");
            _event_loop.exit();
        }

        #[cfg(feature = "audio")]
        {
            self.audio_mgr.update();
        }

        self.renderer
            .as_mut()
            .unwrap()
            .begin_frame();
        if !self
            .game
            .update(&curr, self.renderer.as_mut().unwrap())
        {
            _event_loop.exit();
        }
    }
}

#[cfg(feature = "gui")]
fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        eprintln!(
            "  hint: the `gui` feature needs system SDL3/OpenAL dev libraries \
             (libsdl3-dev / libopenal-dev) and wgpu with raw-window-handle."
        );
        std::process::exit(1);
    }
}
