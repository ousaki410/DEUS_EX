//! deusex-launcher - entry point for the Deus Ex native engine.
//!
//! Build modes:
//!   * default (no features): resource-directory detector only. Safe to build
//!     on any machine, no native game library required.
//!   * `--features gui`: real engine entry point. Requires system
//!     `libsdl3-dev` / `libopenal-dev` (and wgpu built with
//!     `raw-window-handle`). See `deusex-input` / `deusex-audio` /
//!     `deusex-renderer` for the missing-library fallback behavior.
//!
//!   cargo run -p deusex-launcher --features gui

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
    // Explicit overrides take precedence.
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

    // Auto-detect: CWD-relative, env vars, then well-known absolute paths.
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

/// Return the `system` directory (containing the `.u` packages) inside a game
/// root, if present.
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

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Mode A: no `gui` feature — resource-directory detector only.
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

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

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Mode B: `gui` feature — real engine entry point (SDL3 + wgpu + OpenAL).
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[cfg(feature = "gui")]
async fn run_async() -> Result<(), Box<dyn std::error::Error>> {
    use deusex_audio::AudioContext;
    use deusex_game::Game;
    use deusex_input::sdl::{SdlContext, SdlWindow};
    use deusex_renderer::{Gpu, Renderer};
    use tracing::info;

    let args = Args::parse();
    let installation =
        locate_installation(&args).ok_or_else(|| "no Deus Ex installation found".to_string())?;

    println!(
        "deusex-launcher (gui): detected {} installation",
        installation.name
    );
    println!("  root:     {}", installation.root.display());
    println!("  system:   {}", installation.system.display());

    // ┌─────────────────────────────────────────────────────────────────┐
    // │ 1. SDL3 window + input                                          │
    // └─────────────────────────────────────────────────────────────────┘
    // `sdl3::video::Window` implements wgpu's `WindowHandle` trait (blanket
    // impl over `HasWindowHandle` + `HasDisplayHandle`), so it can be passed
    // straight to `Instance::create_surface`; wgpu boxes it and keeps the
    // native window alive for the surface's lifetime.
    //
    // 注意：sdl3 0.2 依赖 `raw-window-handle 0.5`，而 wgpu 0.20 依赖
    // `raw-window-handle 0.6`，两者的 `HasWindowHandle` trait 不兼容。
    // 系统装好库后，若这里编译失败，需统一版本（如升级 sdl3 或降级 wgpu）
    // 再把窗口句柄传给 wgpu。
    let mut sdl = SdlContext::init()?;
    let SdlWindow { window } = sdl.create_window("Deus Ex Native", 1280, 720)?;

    // ┌─────────────────────────────────────────────────────────────────┐
    // │ 2. OpenAL audio                                                 │
    // └─────────────────────────────────────────────────────────────────┘
    let audio = AudioContext::new()?;
    info!("audio backend ready");

    // ┌─────────────────────────────────────────────────────────────────┐
    // │ 3. wgpu device + swapchain surface                              │
    // └─────────────────────────────────────────────────────────────────┘
    let gpu = Gpu::new().await?;
    let surface: wgpu::Surface<'static> = gpu.instance.create_surface(window)?;
    let mut renderer = Renderer::new(
        gpu,
        surface,
        1280,
        720,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    )?;

    // ┌─────────────────────────────────────────────────────────────────┐
    // │ 4. Game + main loop                                             │
    // └─────────────────────────────────────────────────────────────────┘
    let mut game = Game::with_audio(audio);

    // TODO: 推入实际的游戏场景（例如主菜单、关卡加载等）。
    // 当前没有场景时，renderer 会每帧清屏为深蓝色。

    game.run_loop(|state| sdl.pump(state), &mut renderer);

    Ok(())
}

#[cfg(feature = "gui")]
#[tokio::main]
async fn main() {
    if let Err(err) = run_async().await {
        eprintln!("error: {err}");
        eprintln!(
            "  hint: the `gui` feature needs system SDL3/OpenAL dev libraries \
             (libsdl3-dev / libopenal-dev) and wgpu with raw-window-handle."
        );
        std::process::exit(1);
    }
}
