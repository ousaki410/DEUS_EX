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
deusex-reverse
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
- `Constraint`: 物理约束（弹簧 Spring、距离 Distance、点对点 PointToPoint）
- 约束求解器：位置基础动力学（PBD）风格迭代求解
- `raycast()`: 射线检测（球体/AABB/平面），返回最近命中信息（距离、法线、接触点），用于视线检测等

**状态**: 🟡 70% 完成（基础刚体物理 + 角色控制器 + 约束系统已实现，等待软体/流体等高级特性）

**模块**:
- `PhysicsWorld`: 物理世界与步进模拟
- `RigidBody`, `BodyId`: 刚体与句柄
- `ColliderShape`: 碰撞体形状枚举
- `Collision`: 碰撞检测结果（法线、穿深、接触点）
- `CollisionManifold`: 单帧碰撞集合
- `PhysicsConfig`: 物理配置（重力、时间步长、子步数等）
- `CharacterController`: 角色控制器
- `Constraint`, `ConstraintId`, `ConstraintType`: 物理约束与类型

**使用示例**:
```rust
use deusex_physics::{PhysicsWorld, RigidBody, BodyId, CharacterController, Constraint, ConstraintType};

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

// 添加弹簧约束连接两个球体
let spring = Constraint {
    id: ConstraintId::null(),
    body_a: floor_id,
    body_b: ball_id,
    ctype: ConstraintType::Spring {
        stiffness: 10.0,
        damping: 1.0,
        rest_length: 5.0,
    },
    enabled: true,
};
world.add_constraint(spring);

// 物理步进（自动求解碰撞和约束）
world.step();

// 查询碰撞结果
for collision in world.last_manifold().collisions.iter() {
    println!("碰撞: {:?} 与 {:?}, 穿深: {}", collision.body_a, collision.body_b, collision.penetration);
}
```

---

### deusex-ai

**职责**: AI 行为树系统 + 感知与导航寻路

**主要功能**:
- `BehaviorTree`: 行为树，持有根节点和黑板
- `Blackboard`: 键值对黑板，支持 Bool/Int/Float/String/Vec3 类型，供节点间共享状态
- `BehaviorTreeExecutor`: 行为树执行器，每帧调用 tick
- `Node` trait: 行为节点接口（tick 返回 Status: Running/Success/Failure）
- `Sequence`: 顺序组合节点，所有子节点都成功才返回 Success
- `Selector`: 选择器组合节点，有一个子节点成功就返回 Success
- `Inverter`: 反转装饰器，翻转子节点的 Success/Failure
- `Repeater`: 重复装饰器，重复执行子节点指定次数或无限
- `ActionNode`: 动作叶子节点（用户自定义行为闭包）
- `ConditionNode`: 条件叶子节点（用户自定义条件检查闭包）
- `builtin`: 预定义行为节点（has_target/is_close_to_target/move_to_target/attack_target/patrol/wait）
- `PerceptionSystem`: 感知系统（视线检测+听觉），支持用户提供的射线检测回调
- `PerceptionComponent`: 感知组件（FOV、视距、听觉阈值、朝向、位置、视觉/听觉敏感度、FOV 边缘衰减、多点遮挡、HRTF 头宽/声音速度）
- `Stimulus`/`PerceptionResult`: 感知刺激与结果结构体（支持 SoundType：脚步声/枪声/对话声/爆炸声、HearingInfo 听觉详情 ITD/ILD/方位角/仰角）
- `SenseType`: 感知类型枚举（Vision/Hearing）
- `SoundType`: 声音类型枚举（Footstep/Conversation/Gunshot/Explosion），各自有不同衰减系数和基础听距
- `NavGrid`: 导航网格（2D 网格，可通过/障碍物，世界↔网格坐标转换，动态障碍物支持）
- `AStarPathfinder`: A* 寻路算法（曼哈顿启发式，四连通）
- `Path`/`PathFollower`: 路径与路径跟随器（支持路点导航、速度控制、移动方向计算）
- `NavTriangle`/`NavGrid::retriangulate_dirty_region`: 动态重 triangulation（局部重建，障碍物变化后只重建受影响区域）
- `CrowdAgent`/`CrowdManager`: Crowd 避让系统，基于 ORCA (Optimal Reciprocal Collision Avoidance) 算法，支持碰撞时间预测和 2D 线性规划速度求解

**状态**: 🟡 90% 完成（行为树 + 感知高级特性 + A* 寻路 + 导航网格动态更新 + Crowd ORCA 避让已实现）

**模块**:
- `BehaviorTree`: 行为树容器
- `Blackboard`: 黑板键值对存储
- `BehaviorTreeExecutor`: 行为树执行器
- `Node`: 行为节点 trait
- `Status`: 节点执行状态（Running/Success/Failure）
- `Sequence`, `Selector`, `Inverter`, `Repeater`: 组合与装饰节点
- `ActionNode`, `ConditionNode`: 叶子节点
- `builtin`: 内置 AI 行为节点
- `PerceptionSystem`, `PerceptionComponent`: 感知系统与组件
- `Stimulus`, `PerceptionResult`, `SenseType`, `SoundType`, `HearingInfo`: 感知数据结构
- `NavGrid`, `NavTriangle`, `AStarPathfinder`, `Path`, `PathFollower`: 导航与寻路
- `CrowdAgent`, `CrowdManager`: Crowd 代理与 ORCA 避让管理器

**使用示例**:
```rust
use deusex_ai::{BehaviorTree, BehaviorTreeExecutor, Sequence, Status};
use deusex_ai::builtin::{has_target, is_close_to_target, attack_target, wait};

let mut tree = BehaviorTree::new("GuardAI");
let mut seq = Sequence::new("GuardSequence");
seq.add_child(has_target());
seq.add_child(is_close_to_target(2.0));
seq.add_child(attack_target());
tree.set_root(Box::new(seq));

let mut executor = BehaviorTreeExecutor::new(tree);
executor.tree_mut().blackboard_mut().set_bool("target", true);
executor.tree_mut().blackboard_mut().set_vec3("self_pos", [0.0, 0.0, 0.0]);
executor.tree_mut().blackboard_mut().set_vec3("target_pos", [1.0, 0.0, 0.0]);

// 每帧调用
let status = executor.tick();
assert_eq!(status, Status::Success);
```

**感知系统示例**:
```rust
use deusex_ai::{PerceptionSystem, PerceptionComponent, Stimulus, SenseType, SoundType};

let raycast_cb = |origin: [f32; 3], target: [f32; 3]| {
    // 返回 true 表示被遮挡
    false
};
let mut system = PerceptionSystem::new(raycast_cb);
system.set_timestamp(100.0);

let perceiver = PerceptionComponent {
    position: [0.0, 0.0, 0.0],
    forward: [0.0, 0.0, -1.0],
    ..Default::default()
};

// 视觉刺激
let stimulus = Stimulus::vision([0.0, 0.0, -10.0], 1.0);
let results = system.perceive(&perceiver, &[stimulus]);

// 听觉刺激（枪声，远距离传播）
let gunshot = Stimulus::hearing([50.0, 0.0, 0.0], 1.0, SoundType::Gunshot);
let results = system.perceive(&perceiver, &[gunshot]);
```

**导航网格动态更新示例**:
```rust
use deusex_ai::{NavGrid, AStarPathfinder, Path};

let mut grid = NavGrid::new(20, 20, 1.0, [0.0, 0.0]);

// 设置矩形障碍物（如一堵墙）
grid.set_rect_obstacle([5.0, 5.0], [7.0, 9.0], true);

// 设置圆形障碍物（如一个柱子）
grid.set_circle_obstacle([12.0, 12.0], 2.0, true);

// 寻路
let path = AStarPathfinder::find_path(&grid, (0, 0), (19, 19));

// 检查路径是否因动态障碍物而失效
if grid.is_path_invalidated(path.as_ref().unwrap()) {
    // 重新寻路
    grid.clear_dirty_regions();
}

// 清除障碍物
grid.clear_obstacle(6, 6);
```

**HRTF 听觉空间化示例**:
```rust
use deusex_ai::{PerceptionSystem, PerceptionComponent, Stimulus, SenseType, SoundType};

let raycast_cb = |origin: [f32; 3], target: [f32; 3]| {
    // 返回遮挡强度 0.0~1.0
    0.0
};
let mut system = PerceptionSystem::new(raycast_cb);
system.set_timestamp(100.0);

let perceiver = PerceptionComponent {
    position: [0.0, 0.0, 0.0],
    forward: [0.0, 0.0, -1.0],
    enable_hrtf: true,
    head_width: 0.215, // 成人平均头宽
    ..Default::default()
};

// 枪声在右侧 10 米处
let gunshot = Stimulus::hearing([10.0, 0.0, 0.0], 1.0, SoundType::Gunshot);
let results = system.perceive(&perceiver, &[gunshot]);

if let Some(result) = results.first() {
    if let Some(hearing) = &result.hearing_info {
        println!("ITD: {} 秒, ILD: {:.2}, 方位角: {:.2} 弧度",
                 hearing.itd, hearing.ild, hearing.azimuth);
        println!("置信度: {:.2}", result.confidence);
    }
}
```

**Crowd 避让示例**:
```rust
use deusex_ai::{CrowdManager, CrowdAgent};

let mut crowd = CrowdManager::new();
crowd.set_dt(0.016); // 60 FPS
crowd.set_neighbor_radius(5.0);
crowd.set_avoidance_weight(0.7);

// 添加两个相向而行的代理
let mut a1 = CrowdAgent::new(0, [-2.0, 0.0], 0.5, 2.0);
a1.set_pref_vel([2.0, 0.0]); // 向右
crowd.add_agent(a1);

let mut a2 = CrowdAgent::new(1, [2.0, 0.0], 0.5, 2.0);
a2.set_pref_vel([-2.0, 0.0]); // 向左
crowd.add_agent(a2);

// 每帧调用 update，代理会自动互相避让
for _ in 0..100 {
    crowd.update();
    let pos = crowd.get_agent(0).unwrap().state.pos;
    println!("代理 0 位置: ({:.2}, {:.2})", pos[0], pos[1]);
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

**职责**: AI 行为树系统 + 感知与导航寻路

**主要功能**:
- `BehaviorTree`: 行为树，持有根节点和黑板
- `Blackboard`: 键值对黑板，支持 Bool/Int/Float/String/Vec3 类型，供节点间共享状态
- `BehaviorTreeExecutor`: 行为树执行器，每帧调用 tick
- `Node` trait: 行为节点接口（tick 返回 Status: Running/Success/Failure）
- `Sequence`: 顺序组合节点，所有子节点都成功才返回 Success
- `Selector`: 选择器组合节点，有一个子节点成功就返回 Success
- `Inverter`: 反转装饰器，翻转子节点的 Success/Failure
- `Repeater`: 重复装饰器，重复执行子节点指定次数或无限
- `ActionNode`: 动作叶子节点（用户自定义行为闭包）
- `ConditionNode`: 条件叶子节点（用户自定义条件检查闭包）
- `builtin`: 预定义行为节点（has_target/is_close_to_target/move_to_target/attack_target/patrol/wait）
- `PerceptionSystem`: 感知系统（视线检测+听觉），支持用户提供的射线检测回调
- `PerceptionComponent`: 感知组件（FOV、视距、听觉阈值、朝向、位置、视觉/听觉敏感度、FOV 边缘衰减、多点遮挡）
- `Stimulus`/`PerceptionResult`: 感知刺激与结果结构体（支持 SoundType：脚步声/枪声/对话声/爆炸声）
- `SenseType`: 感知类型枚举（Vision/Hearing）
- `SoundType`: 声音类型枚举（Footstep/Conversation/Gunshot/Explosion），各自有不同衰减系数和基础听距
- `NavGrid`: 导航网格（2D 网格，可通过/障碍物，世界↔网格坐标转换，动态障碍物支持）
- `AStarPathfinder`: A* 寻路算法（曼哈顿启发式，四连通）
- `Path`/`PathFollower`: 路径与路径跟随器（支持路点导航、速度控制、移动方向计算）
- `NavTriangle`/`NavGrid::retriangulate_dirty_region`: 动态重 triangulation，障碍物变化后重建受影响三角形
- `CrowdAgent`/`CrowdManager`: Crowd 避让系统，基于 ORCA (Optimal Reciprocal Collision Avoidance) 算法，支持碰撞时间预测和 2D 线性规划速度求解
- `GoapState`/`GoapAction`/`GoapPlanner`/`GoapAgent`: GOAP 目标导向动作规划系统，支持固定代价和启发式搜索的固定代价搜索

**状态**: 🟡 90% 完成（行为树 + 感知高级特性 + A* 寻路 + 导航网格动态更新 + Crowd ORCA 避让 + GOAP 规划器已实现）

**模块**:
- `BehaviorTree`: 行为树容器
- `Blackboard`: 黑板键值对存储
- `BehaviorTreeExecutor`: 行为树执行器
- `Node`: 行为节点 trait
- `Status`: 节点执行状态（Running/Success/Failure）
- `Sequence`, `Selector`, `Inverter`, `Repeater`: 组合与装饰节点
- `ActionNode`, `ConditionNode`: 叶子节点
- `builtin`: 内置 AI 行为节点
- `PerceptionSystem`, `PerceptionComponent`: 感知系统与组件
- `Stimulus`, `PerceptionResult`, `SenseType`, `SoundType`, `HearingInfo`: 感知数据结构
- `NavGrid`, `NavTriangle`, `AStarPathfinder`, `Path`, `PathFollower`: 导航与寻路
- `CrowdAgent`, `CrowdManager`: Crowd 代理与 ORCA 避让管理器
- `GoapState`, `GoapAction`, `GoapPlanner`, `GoapAgent`: GOAP 目标导向动作规划

**使用示例**:
```rust
use deusex_ai::{BehaviorTree, BehaviorTreeExecutor, Sequence, Status};
use deusex_ai::builtin::{has_target, is_close_to_target, attack_target, wait};

let mut tree = BehaviorTree::new("GuardAI");
let mut seq = Sequence::new("GuardSequence");
seq.add_child(has_target());
seq.add_child(is_close_to_target(2.0));
seq.add_child(attack_target());
tree.set_root(Box::new(seq));

let mut executor = BehaviorTreeExecutor::new(tree);
executor.tree_mut().blackboard_mut().set_bool("target", true);
executor.tree_mut().blackboard_mut().set_vec3("self_pos", [0.0, 0.0, 0.0]);
executor.tree_mut().blackboard_mut().set_vec3("target_pos", [1.0, 0.0, 0.0]);

// 每帧调用
let status = executor.tick();
assert_eq!(status, Status::Success);
```

**GOAP 示例**:
```rust
use deusex_ai::{GoapAgent, GoapAction, GoapPlanner, GoapState};

let mut planner = GoapPlanner::new();
planner.add_action(GoapAction::new(
    "pickup_item",
    vec![("has_item".to_string(), false)],
    vec![("has_item".to_string(), true)],
    1.0,
    |_| true,
    |state| {
        let mut new = state.clone();
        new.set("has_item", true);
        new
    },
));
planner.add_action(GoapAction::new(
    "use_item",
    vec![("has_item".to_string(), true)],
    vec![("item_used".to_string(), true)],
    1.0,
    |_| true,
    |state| {
        let mut new = state.clone();
        new.set("item_used", true);
        new
    },
));

let mut state = GoapState::new();
state.set("has_item", false);
state.set("item_used", false);

let plan = planner.plan(&state, &vec![("item_used".to_string(), true)]);
assert_eq!(plan, Some(vec!["pickup_item", "use_item"]));
```

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

### deusex-reverse

**职责**: 零运行逆向工程工具（P0）

**主要功能**:
- 批量扫描目录内全部 UE1 `.u` 包
- 类清单：名称、父类、outer、类 flags、native 标记
- 属性清单：`UProperty` 链、类型、数组维度、flags、复制偏移
- 函数签名：参数名/类型、返回值、`out`/`optional`/`skip`/`coerce`
- State 内函数归因（`类.State.函数`）
- 表达式树扫描：native / Final / Virtual / Global 调用与对象引用
- 分支与 opcode 频率统计
- 输出 Markdown、JSON manifest、CSV、Graphviz DOT

**命令行**:
```bash
cd deusex-native
cargo run --release -p deusex-reverse -- ../goty/app/system ../reverse_report --all
# 只分析单个包：
cargo run --release -p deusex-reverse -- ../goty/app/system ./out --filter Core --all
```

**产出位置**:
- `docs/reverse/P0_SUMMARY.md`：P0 总结与关键发现
- `docs/reverse/packages.csv`：包级统计
- `docs/reverse/classes.csv`：类清单
- `docs/reverse/properties.csv`：属性清单
- `docs/reverse/functions.csv`：函数签名与调用统计
- `docs/reverse/opcodes.csv`：全局 opcode 频率

完整 Markdown/JSON/DOT 报告约 24MB，默认输出到 `reverse_report/` 且不纳入版本库。

**状态**: ✅ 100% 完成（P0/P1），为 P2 native 交叉验证提供输入

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
deusex-reverse   → deusex-package, deusex-bytecode
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

### 逆向分析阶段（P0 → P4）
- [x] **P0** `deusex-reverse`：38 个 `.u` 批量解析，类/属性/函数/opcode 清单
- [x] **P1** 关键包反编译（`Core` / `Engine` / `DeusEx`）→ 伪代码清单（`deusex-bytecode::pseudocode`）
- [x] **P2** Ghidra 交叉验证 native 函数地址与签名（396/806 已验证）
- [x] **P3** 静态分析驱动 Rust 回归测试（`deusex-vm/tests/regression.rs`）
- [x] **P4** 最小化动态验证方案记录（`docs/reverse/P4_SUMMARY.md`；受环境限制，无 wine 无法运行 Windows EXE）

---

## 📝 常见问题

**Q: 为什么有些 crate 是空的占位？**

A: 这是为了保持项目结构完整性，为未来的开发预留接口。

**Q: 如何测试某个特定 crate？**

A: 运行 `cargo test -p deusex-vm` 或 `cargo test --all-targets`

**Q: 渲染模块为什么还没有实现？**

A: 渲染模块需要完成基础后端的集成，目前框架已完成，等待后续实现具体绘制。

---

**最后更新**: 2026-11-07