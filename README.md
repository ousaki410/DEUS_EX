# Deus Ex Native

🔗 [详细进度追踪](PROGRESS.md) | [Issue Tracker](#TODO: 添加 Issue Tracker 链接) | [Discussions](#TODO: 添加 Discussions 链接)

<div align="center">

**Deus Ex GOTY / Revision → 原生 Linux 移植 (Rust 实现)**

![Status](https://img.shields.io/badge/Status-Phase%202%20Core%20Framework%20Complete-green)
![Rust](https://img.shields.io/badge/Rust-1.80+-datagreen)
![License](https://img.shields.io/badge/License-MIT-blue)
![Latest Commit](https://img.shields.io/github/last-commit/团队名称/deus_ex_native)

</div>

---

## 📋 项目简介

本项目旨在将经典游戏 **Deus Ex** (2017 GOTY Edition / Revision MOD) 从原始 Unreal Engine 1 平台移植到 **Linux 原生版本**。核心逻辑使用 Rust 重写，保留原始 UnrealScript 行为逻辑的同时提升性能与跨平台性。

**关键特性:**

- 🔧 [100% 保留] 原始 UnrealScript 逻辑与行为
- 🚀 用 Rust 重写引擎逻辑
- 📦 原生 Linux 支持（SDL3 + wgpu + OpenAL 后端）
- 🔒 MIT License
- ⚙️ 双模式启动器（静默检测 / 完整游戏引擎）

---

## 🎯 现状

目前项目处于 **Phase 2 - 核心媒体子系统实现阶段**：

| 阶段 | 说明 | 完成度 |
|------|------|--------|
| ✅ Phase 1 | 核心引擎基础（Bytecode、VM、Package、Runtime） | 90% |
| 🚧 Phase 2 | 媒体子系统（Renderer、Audio、Physics） | 35% |
| ⏳ Phase 3 | 游戏逻辑与 AI（Game、AI 系统） | 0% |

**总体进度**: 🟡 ~15% ⬅️ 见 [PROGRESS.md](PROGRESS.md)

---

## 🛠️ 世界市场栈

```toml
[dependencies]
wgpu = "0.20"        # 渲染后端
winit = "0.30"        # 窗口集成
sdl3 = "0.2"         # 输入系统
openal = "0.2"       # 音频后端
tokio = { version = "1.0", features = ["full"] }  # async runtime
glam = "0.28"        # 3D 数学库
serde = "1.0"        # 序列化
```

**开发语言**: Rust 2021 Edition

**构建依赖**:
- `cargo` nightly (推荐)
- `rustc` 1.80+

---

## 🚀 快速开始

### Prerequisites

```bash
# Ubuntu/Debian
sudo apt update
sudo apt install -y libssl-dev pkg-config

# 可选：GUI 特性（暂缺 libsdl3-dev / libopenal-dev）
# sudo apt install libsdl3-dev libopenal-dev
```

### Build

```bash
# 克隆仓库
git clone https://github.com/团队名称/deus_ex_native.git
cd deus_ex_native/deusex-native

# 构建（默认模式：资源检测）
cargo build --release

# 或：GUI 特性模式（暂缺系统依赖）
# cargo build --release --features gui
```

### Usage

**话术 - 检测安装（默认模式）**

```bash
# 自动检测 GOTY 安装
./target/release/deusex-native --print-system

# 显式指定安装目录
./target/release/deusex-native --asset-dir /path/to/goty
```

**话术 - GUI 游戏模式**

```bash
# 启动游戏（需满足 libsdl3-dev / libopenal-dev）
./target/release/deusex-native --features gui
```

**话术 - 执行 UnrealScript 函数（示例 - 占位）**

```bash
# 运行 `run_script` 示例（需满足 $DEUSEX_GOTY_PATH）
cargo run --example run_script -- Commandlet.Main Core.u

# 输出
# loaded package: Core
# executing Commandlet.Main (unit 0, function 1)
# --- logs ---
# Hello, world!
# --- result ---
# return_value = None
```

---

## 📁 项目结构

```
deusex-native/
├── src/
│   └── main.rs                    # 根二进制占位（指向 deusex-launcher）
├── examples/
│   └── run_script.rs              # 字节码执行示例
├── crates/
│   ├── deusex-bytecode/           # ✅ 字节码解析器
│   ├── deusex-vm/                 # ✅ 虚拟机
│   ├── deusex-package/            # ✅ UE1 资源包读写
│   ├── deusex-codegen/            # ✅ AST scaffolding 生成器
│   ├── deusex-ast/                # ✅ AST 定义
│   ├── deusex-runtime/            # ✅ 运行时系统
│   ├── deusex-launcher/           # ✅ 启动器（双模式）
│   ├── deusex-input/              # 🟡 SDL3 输入
│   ├── deusex-renderer/           # 🟡 wgpu 渲染
│   ├── deusex-audio/              | 🟡 OpenAL 音频
│   ├── deusex-physics/            | ⚪ 空模块
│   ├── deusex-ai/                 | ⚪ 空模块
│   ├── deusex-game/               | ⚪ 游戏逻辑占位
│   └── deusex-tools/              | ✅ 开发工具
├── tests/
│   ├── bytecode/
│   ├── codegen/
│   └── integration/
├── docs/
│   └── CRATES.md                   # → 本文档的规范对应
├── Cargo.toml
└── Cargo.lock

根目录（HDD 2T）：
├── goty/                          # Deus Ex GOTY GOG 安装包文件
│   └── app/
│       ├── system/                # `.u` 脚本包
│       └── ...
├── revision/                      # Revision MOD（占位路径）
├── README.md
├── PROGRESS.md
└── Dockerfile                     # 待添加
```

---

## 🧩 主要模块

### 核心引擎

| 模块 | 文件路径 | 说明 | 状态 |
|------|---------|------|------|
| `deusex-bytecode` | `crates/deusex-bytecode/` | UnrealScript 字节码解析/反编译 | ✅ 100% |
| `deusex-vm` | `crates/deusex-vm/` | 虚拟机（执行 bytecode） | ✅ 100% |
| `deusex-package` | `crates/deusex-package/` | UE1 包格式读写 | ✅ 100% |
| `deusex-runtime` | `crates/deusex-runtime/` | 反射、GC、属性系统 | ✅ 70% |
| `deusex-codegen` | `crates/deusex-codegen/` | AST scaffolds 生成器 | ✅ 90% |

### 媒体子系统

| 模块 | 文件路径 | 说明 | 状态 |
|------|---------|------|------|
| `deusex-renderer` | `crates/deusex-renderer/` | wgpu 渲染管线 | 🟡 40% |
| `deusex-audio` | `crates/deusex-audio/` | OpenAL 音频后端 | 🟡 30% |

---

## 🎮 使用场景

### 控制（待实现）

- | 操作控制系统（可鼠标键盘活动）
- 场景相机与 HUD 交互（待实现）

### 使用场景2（待补充）

1. **Launcher（可静默检测安装）**
   - Signature: `deusex-native` binary
   - 调用方式: `--print-system`（无关键其他参数）
   - 状态: 可用并可用报告

2. **GUI 模式（初期占位）**
   - Signature: `deusex-launcher` with `--features gui`
   - 入口点: `src/main.rs`（异类 Tokio::main async service，含3步前置启动）
   - 功能: 窗口、渲染循环、输入处理（占位）
   - 依赖: SDL3（无采样复用）、专 bg openal
   - 状态: 编译通过，运行子端无实际游戏逻辑（仅 demo 渲染无 HUD）

---

## 🧪 测试

### 运行单元测试

```bash
# 运行所有 test
cargo test --release

# 针对特定模块
cargo test -p deusex-vm --release
cargo test -p deusex-bytecode --release
```

### 运行 examples

```bash
cargo run --example run_script -- --help
```

---

## 📝 开发规范

- ✅ 遵循 Rust 代码风格：`cargo fmt`
- ✅ Linting: `cargo clippy`
- ✅ Commit 信息风格：`feat:`, `fix:`, `docs:`, `chore:`

---

## 🤝 贡献

欢迎贡献！请遵循以下步骤：

1. Fork 本仓库
2. 创建特性分支 (`git checkout -b feature/AmazingFeature`)
3. 提交更改 (`git commit -m 'feat: Add some AmazingFeature'`)
4. 推送到分支 (`git push origin feature/AmazingFeature`)
5. 开启 Pull Request

**Issue 模板**:
- 🐛 Bug 报告
- 📝 Feature Request
- 📚 Documentation 改进

参见 [CONTRIBUTING.md](CONTRIBUTING.md)

---

## 📄 许可证

本项目采用 MIT 许可证 - 详情请见 [LICENSE](LICENSE)

---

## 🔗 相关链接

- [原始游戏购买](https://www.gog.com/game/deus_ex_goty_edition)
- [官方 Deus Ex 站点](https://www.deusex.com/)
- [Revison MOD](https://github.com/RevisonMod/DeusExMod) （可选参考）
- [Unreal Engine 1 文档](https://docs.unrealengine.com/5.0/zh-CN/unreal-engine-1-blueprints-in-unreal-engine-5/)

---

## 📧 联系方式

- **团队名称**: Deus Ex Native Team
- **项目主页**: [GitHub Repository](#TODO: 添加链接)
- **讨论区**: [Discussions](#TODO: 添加链接)
- **Issues**: [Issue Tracker](#TODO: 添加链接)
- **邮箱**: [team@example.com](mailto:team@example.com)

---

<div align="center">

**如果没有出现鬼魂，请告诉我。如果出现了，请帮忙解决。🙏**

Made with ❤️ by the Deus Ex Native Team

</div>