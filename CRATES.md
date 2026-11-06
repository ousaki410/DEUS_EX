# Crate 文档

本文档详细说明 `deusex-native` 工作区中的各个 crate。

---

## 📦 Crate 架构

```
deusex-bytecode
deusex-ast
deusex-vm
deusex-package
deusex-codegen
deusex-runtime
deusex-input
deusex-renderer
deusex-audio
deusex-physics
deusex-ai
deusex-game
deusex-tools
deusex-launcher
```

---

## 🔧 核心模块

### deusex-bytecode

**职责**: UnrealScript 字节码解码器

**主要功能**:
- 解析 `.u` 文件中的 UFunction 对象
- 将 bytecode 解码为 AST（表达式树）
- 构建控制流图 (CFG)
- 函数反编译

**模块**:
- `parser`: 解析字节码
- `function`: 函数表示
- `instructions`: 指令定义
- `cfg`: 控制流图构建
- `decompile`: 反编译器

**状态**: ✅ 100% 完成

**使用示例**:
```rust
use deusex_bytecode::{decode_script, BytecodeError};

fn main() -> Result<(), BytecodeError> {
    let script = decode_script(&file_content)?;
    Ok(())
}
```

---

### deusex-vm

**职责**: UnrealScript 虚拟机

**主要功能**:
- 直接执行解码后的 bytecode
- 支持跳转、native 调用、latent CALL 等控制流
- 管理变量作用域
- 处理对象生命周期

**模块**:
- `vm`: 虚拟机核心
- `value`: 值类型系统
- `unit`: Package 单位管理
- `native`: 原生函数注册
- `error`: 错误类型

**状态**: ✅ 100% 完成

**使用示例**:
```rust
use deusex_vm::{PackageUnit, Vm};

let unit = PackageUnit::load("/path/to/Core.u")?;
let mut vm = Vm::new();
vm.add_unit(unit);
vm.exec_function(unit_index, function_index, None, vec![])?;
```

---

### deusex-package

**职责**: UE1 包格式读写

**主要功能**:
- 解析 UE1 包文件格式
- 读取/写入名称表、导入/导出表
- 属性序列化/反序列化
- 资源处理

**模块**:
- `package`: 包加载器
- `archive`: 数据流
- `export_table`: 导出表
- `import_table`: 导入表
- `name_table`: 名称表
- `property`: 属性系统
- `serialization`: 序列化
- `summary`: 包摘要

**状态**: ✅ 100% 完成

**使用示例**:
```rust
use deusex_package::Package;

let package = Package::load("/path/to/game.u")?;
let name_table = package.name_table();
```

---

### deusex-runtime

**职责**: 运行时系统

**主要功能**:
- UObject 反射系统
- AActor 生命周期管理
- 属性系统（Getter/Setter）
- GC（垃圾回收）
- Latent Action 处理
- World Tick

**模块**:
- `uobject`: UObject 基类
- `class`: 类系统
- `actor`: Actor 基类
- `property`: 属性反射
- `gc`: 垃圾回收
- `latent`: Latent Action
- `world`: 世界管理
- `name`: 名称系统
- `array`: 数组类型
- `delegate`: 委托系统
- `replication`: 网络复制

**状态**: ✅ 70% 完成（缺少游戏逻辑）

**使用示例**:
```rust
use deusex_runtime::{Runtime, ClassBuilder};

let mut rt = Runtime::new();
rt.initialize();

let actor_class = ClassBuilder::new("Actor")
    .parent("Object")
    .flags(ClassFlags::NATIVE)
    .build();
rt.class_registry().register(actor_class);
```

---

### deusex-codegen

**职责**: AST scaffolding 生成器

**主要功能**:
- 将 Decompiled AST 生成 Rust scaffolds
- 处理原生函数和 latent CALL
- 生成临时类/函数的 Rust 代码

**状态**: ✅ 90% 完成（标记跳转和未实现项）

**使用示例**:
```rust
use deusex_codegen::{CodegenOptions, generate_function};

let code = generate_function(&ast_function, &CodegenOptions::default())?;
fs::write("output.rs", code.source)?;
```

---

### deusex-ast

**职责**: UnrealScript AST 定义

**主要功能**:
- 定义 UnrealScript AST 类型
- 表达式与语句定义
- 类型系统

**模块**:
- `lib.rs`: AST 类型定义

**状态**: ✅ 80% 完成（部分表达式/语句待补充）

---

## 🔨 媒体子系统

### deusex-renderer

**职责**: 渲染管线（基于 wgpu）

**主要功能**:
- GPU 设备/队列/适配器初始化
- Swapchain 表面配置与双缓冲呈现
- RGBA8 纹理上传（TextureId 句柄管理）
- Sprite 渲染管线（顶点着色器 + 片元着色器，Alpha 混合）
- 屏幕像素坐标 → NDC 变换
- `begin_frame()` / `draw_sprite()` / `render()` 渲染闭环
- 窗口大小调整（resize）

**模块**:
- `Gpu`: wgpu 设备与队列
- `Renderer`: 表面绑定的渲染上下文
- `TextureId`, `TextureData`: 纹理资源句柄与上传描述
- `SpriteDraw`, `SpriteRect`: 绘图指令

**状态**: 🟡 60% 完成（基础渲染管线已实现，等待游戏资源接入）

**使用示例**:
```rust
use deusex_renderer::{Gpu, Renderer, TextureId, SpriteDraw, SpriteRect, TextureData};

let gpu = Gpu::new().await?;
let surface = gpu.instance.create_surface(window)?;
let mut renderer = Renderer::new(gpu, surface, 1280, 720, wgpu::TextureFormat::Bgra8UnormSrgb)?;

// 上传纹理
let tex_id = renderer.upload_texture(TextureData {
    width: 64,
    height: 64,
    data: vec![255, 0, 0, 255; 64 * 64], // 纯红
})?;

// 渲染一帧
renderer.begin_frame();
renderer.draw_sprite(SpriteDraw {
    texture: tex_id,
    rect: SpriteRect { x: 100.0, y: 100.0, w: 64.0, h: 64.0 },
    color: wgpu::Color::WHITE,
})?;
renderer.render()?;
```

---

### deusex-audio

**职责**: 音频后端（基于 OpenAL Soft）

**主要功能**:
- OpenAL 设备/Context 初始化与销毁（RAII，`AudioContext`）
- `AudioBuffer`: WAV 文件解析（PCM 8/16-bit, mono/stereo）并上传到 OpenAL 缓冲
- `AudioSource`: 音源播放控制（play/stop/pause）、音量(gain)、音调(pitch)、3D 位置、循环(looping)、状态查询
- `AudioManager`: 统一管理缓冲(`load_buffer`)、音源(`create_source`)、绑定(`bind_buffer`)、播放控制、监听者(`set_listener`)
- `Listener`: 3D 音频监听者（位置、前向/上向量、速度），通过 `alListenerfv` 设置方向
- `UmxPlayer`: .umx 模块音乐播放器（需 `openmpt` feature），基于 libopenmpt 渲染 + OpenAL 双缓冲流式播放，支持 MOD/XM/S3M/IT/UMX 格式
- `probe_module()`: 模块文件格式探测（需 `openmpt` feature）

**状态**: 🟡 70% 完成（后端初始化 + 缓冲/音源/管理器 + .umx 模块音乐播放已实现）

**模块**:
- `AudioContext`: OpenAL 设备与上下文 RAII
- `AudioBuffer`, `BufferId`: 已加载的音频缓冲句柄
- `AudioSource`, `SourceId`: 音源实例与句柄
- `AudioManager`: 统一音频管理器
- `SourceConfig`, `Listener`, `Position`: 配置与 3D 位置类型
- `UmxPlayer`: .umx 模块播放器（`openmpt` feature 下可用）
- `probe_module`: 模块格式探测函数（`openmpt` feature 下可用）

**使用示例**:
```rust
use deusex_audio::{AudioContext, AudioManager, Listener, Position, SourceConfig};

let ctx = AudioContext::new()?;
let mut mgr = AudioManager::new(ctx)?;

// 加载 WAV 声音
let buffer_id = mgr.load_buffer("footstep", &wav_bytes)?;

// 创建音源并绑定缓冲
let source_id = mgr.create_source(SourceConfig {
    gain: 0.8,
    looping: false,
    ..Default::default()
})?;
mgr.bind_buffer(source_id, buffer_id)?;

// 播放
mgr.play(source_id)?;

// 设置 3D 监听者（玩家位置）
mgr.set_listener(Listener {
    position: Position { x: 10.0, y: 0.0, z: -5.0 },
    ..Default::default()
});
```

**.umx 模块音乐播放示例**:
```rust
use deusex_audio::umx::UmxPlayer;

let ctx = AudioContext::new()?;
let umx_data = std::fs::read("music.umx")?;

let mut player = UmxPlayer::from_data(
    &umx_data,
    ctx.al_device(),
    ctx.al_context(),
    44100,
)?;
player.set_gain(0.7);
player.play();

// 每帧调用 update
loop {
    if !player.update() {
        break; // 播放结束
    }
}
```

---

### deusex-input

**职责**: 输入系统（基于 SDL3）+ 游戏动作绑定

**主要功能**:
- `InputState`: 每帧输入快照（键盘按下/按下边缘/释放边缘、鼠标移动/按钮/位置、退出请求）
- `InputAction`: 游戏动作枚举（MoveForward/Backward/StrafeLeft/Right/Jump/Crouch/Run/Use/Fire/NextPrevWeapon/Inventory/Map/Menu/Talk/Action1/2，共 16 种）
- `InputBinding`: 动作到多个按键(SDL3 scancode)的映射
- `InputManager`: 动作状态查询(`is_action_down`/`pressed`/`released`)、按键边缘检测(`update_edges`)、默认 WASD 绑定、运行时重新绑定(`bind_key`/`unbind_key`)
- SDL3 后端：`SdlContext`(窗口+事件泵)、`SdlWindow`、键盘/鼠标/窗口关闭事件处理

**状态**: 🟡 85% 完成（输入状态 + 动作绑定系统已实现，等待游戏手柄支持）

**模块**:
- `InputState`: 每帧输入状态
- `InputAction`: 游戏动作枚举
- `InputBinding`: 按键绑定
- `InputManager`: 绑定管理与动作查询
- `keycode`: SDL3 scancode 常量
- `sdl::SdlContext`, `sdl::SdlWindow`: SDL3 后端

**使用示例**:
```rust
use deusex_input::{InputManager, InputAction, keycode};

let mut mgr = InputManager::new(); // 自带默认 WASD 绑定

// 重新绑定跳跃键
mgr.bind_key(InputAction::Jump, keycode::SPACE);

// 查询动作状态
if mgr.is_action_pressed(InputAction::Menu, &input_state) {
    // ESC 刚被按下
}
if mgr.is_action_down(InputAction::MoveForward, &input_state) {
    // W 键被按住
}
```

---

### deusex-physics

**职责**: 3D 刚体物理引擎

**主要功能**:
- `PhysicsWorld`: 物理世界，固定步长模拟（默认 60Hz，可配置子步数）
- `RigidBody`: 刚体（位置、速度、质量、力累加器、弹性/摩擦系数、静态/动态）
- `ColliderShape`: 球体（Sphere）、AABB、无限平面（Plane）三种碰撞体形状
- 广阶段：AABB 包围盒剔除
- 窄阶段：球体-球体、AABB-AABB、球体-AABB、球体-平面、AABB-平面碰撞检测
- 碰撞响应：基于冲量的法向+摩擦解算、位置校正（防止下陷/穿模）
- 欧拉积分速度/位置，重力、速度剪裁（防止隧道效应）
- `CharacterController`: 角色控制器（WASD 移动、跳跃、重力、地面检测）

**状态**: 🟡 50% 完成（基础刚体物理与角色控制器已实现，等待约束/关节/软体等高级特性）

**模块**:
- `PhysicsWorld`: 物理世界与步进模拟
- `RigidBody`, `BodyId`: 刚体与句柄
- `ColliderShape`: 碰撞体形状枚举
- `Collision`: 碰撞检测结果（法线、穿深、接触点）
- `CollisionManifold`: 单帧碰撞集合
- `PhysicsConfig`: 物理配置（重力、时间步长、子步数等）
- `CharacterController`: 角色控制器

**使用示例**:
```rust
use deusex_physics::{PhysicsWorld, RigidBody, BodyId, CharacterController, ColliderShape};

let mut world = PhysicsWorld::default();

// 添加地面平面
let floor_id = world.add_body(RigidBody::plane(BodyId::null(), glam::Vec3::Y, 0.0));

// 添加一个球体
let ball_id = world.add_body(RigidBody::sphere(
    BodyId::null(),
    glam::Vec3::new(0.0, 10.0, 0.0),
    1.0,
    1.0,
));

// 创建角色控制器
let mut cc = CharacterController::new(ball_id, 0.5, 1.8);

// 物理步进
world.step();

// 角色控制（需要可变访问 world）
cc.set_move_input(glam::Vec3::new(1.0, 0.0, 0.0));
cc.update(&mut world, 1.0 / 60.0);

// 查询碰撞结果
for collision in world.last_manifold().collisions.iter() {
    println!("碰撞: {:?} 与 {:?}, 穿深: {}", collision.body_a, collision.body_b, collision.penetration);
}
```

---

## 🎮 游戏逻辑

### deusex-game

**职责**: 游戏主逻辑

**主要功能**:
- Game Loop 管理（固定步长 Tick @60Hz + 变步长渲染）
- Tick 处理（累加器模式）
- 场景栈管理（push/pop/replace，enter/exit 生命周期）
- 输入分发给当前场景
- 渲染帧协调（与 deusex-renderer 集成）
- `run_loop()` 阻塞主循环入口
- OpenAL 音频上下文持有（可选）

**状态**: 🟡 30% 完成（游戏循环框架已实现，等待实际游戏场景）

**使用示例**:
```rust
use deusex_game::Game;

let mut game = Game::new();
game.run_loop(|input| {
    // 从 SDL3 等输入后端填充 input
}, &mut renderer);
```

---

### deusex-audio

**职责**: 音频后端（基于 OpenAL）

**主要功能**:
- 音频设备管理
- OpenAL 设备/Context 初始化与销毁（RAII）

**状态**: 🟡 30% 完成（只有后端初始化框架）

**使用示例**:
```rust
use deusex_audio::AudioContext;

let audio = AudioContext::new()?;
// 音频设备已就绪，可开始播放声音
```）

---

### deusex-ai

**职责**: AI 行为系统

**主要功能**:
- 行为树
- 状态机
- 路径导航
- 决策 AI

**状态**: ⚪ 0% 完成

---

### deusex-physics

**职责**: 物理引擎

**主要功能**:
- 碰撞检测
- 运动学
- 模拟

**状态**: ⚪ 0% 完成

---

## 🛠️ 工具

### deusex-tools

**职责**: 开发工具

**主要功能**:
- 包编辑器
- 修改调试工具
- 资源转换工具

**状态**: ✅ 80% 完成

---

### deusex-launcher

**职责**: 游戏启动器

**主要功能**:
- 游戏安装检测
- 双模式启动：
  - 静默模式：检测安装路径
  - GUI 模式：启动引擎
- 资源管理

**状态**: ✅ 100% 完成（GUI 模式仅为框架）

---

## 📊 Crate 依赖关系

```
deusex-bytecode → deusex-ast
     ↓
deusex-vm       → deusex-bytecode, deusex-package
                 → deusex-runtime
deusex-codegen   → deusex-bytecode, deusex-ast
deusex-launcher  → deusex-input, deusex-renderer, deusex-audio, deusex-vm
                 → (可选: deusex-game)
```

---

## 🔮 开发路线图

### Phase 1: 核心引擎 ✅
- [x] deusex-bytecode
- [x] deusex-vm
- [x] deusex-package
- [x] deusex-runtime（基础）
- [x] deusex-codegen（scaffolding）
- [x] deusex-ast（定义）

### Phase 2: 媒体子系统 🚧
- [ ] deusex-renderer（实际绘制）
- [ ] deusex-audio（完整管线）
- [ ] deusex-input（游戏控制）

### Phase 3: 游戏逻辑 ⏳
- [ ] deusex-game（主循环）
- [ ] deusex-ai（AI 系统）
- [ ] deusex-physics（物理引擎）

---

## 📝 常见问题

**Q: 为什么有些 crate 是空的占位？**

A: 这是为了保持项目结构完整性，为未来的开发预留接口。

**Q: 如何测试某个特定 crate？**

A: 运行 `cargo test -p deusex-vm` 或 `cargo test --all-targets`

**Q: 渲染模块为什么还没有实现？**

A: 渲染模块需要完成基础后端的集成，目前框架已完成，等待后续实现具体绘制。

---

**最后更新**: 2024-11-06