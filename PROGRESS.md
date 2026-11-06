# Deus Ex Native - 项目进度追踪

## 📋 项目概述

**Deus Ex GOTY / Revision** 的原生 Linux 移植项目（Rust 实现），基于 Unreal Engine 1 的游戏逻辑还原。

- **状态**: 🟡 基础框架完成，Phase 2 媒体子系统 + 物理引擎已实现
- **代码量**: ~28,000 行 Rust 代码
- **构建依赖**: Cargo nightly

---

## 🎯 开发目标

### ✅ Phase 1 - 核心引擎基础（已完成）

| 模块 | 代码行数 | 完成度 | 说明 |
|------|---------|--------|------|
| `deusex-bytecode` | 2,445 | ✅ 100% | UnrealScript 字节码解析器、CFG 构建函数反编译器 |
| `deusex-vm` | 4,455 | ✅ 100% | 虚拟机直接执行原始字节码（支持跳转、latent CALL、switch） |
| `deusex-package` | 1,421 | ✅ 100% | UE1 包格式读写工具（名称表、导入/导出表、属性系统） |
| `deusex-codegen` | 1,180 | ✅ 90% | 将 Decompiled AST 生成 Rust scaffolds（部分 control flow 仅用注释标记） |
| `deusex-ast` | 363 | ⚠️ 80% | AST 类型定义（部分表达式/语句待补充） |
| `deusex-launcher` | 270 | ✅ 100% | 双模式启动器（静默检测 + GUI 引擎入口点） |
| `deusex-runtime` | 1,881 | ✅ 70% | 运行时系统（反射、GC、属性系统、world tick、latent 处理） |

### 🚧 Phase 2 - 媒体子系统（进行中）

| 模块 | 代码行数 | 完成度 | 说明 |
|------|---------|--------|------|
| `deusex-renderer` | ~450 | 🟡 70% | wgpu 渲染管线、GPU 初始化、纹理上传、Sprite 绘制、双缓冲呈现、窗口 resize |
| `deusex-audio` | ~450 | 🟡 70% | OpenAL 设备/Context、AudioBuffer(WAV 解析)、AudioSource(播放/音量/音调/3D位置/循环)、AudioManager(统一管理)、Listener 3D 音频、UmxPlayer(libopenmpt .umx 模块流式播放) |
| `deusex-physics` | ~750 | 🟡 75% | 刚体物理世界、球体/AABB/平面碰撞体、AABB 广阶段剔除、窗口阶段碰撞检测、冲量碰撞响应、欧拉积分、角色控制器(移动/跳跃/地面检测)、弹簧/距离/点对点约束、射线检测(raycast) |
| `deusex-ai` | ~650 | 🟡 60% | 行为树系统（Sequence/Selector/Inverter/Repeater 组合节点、Action/Condition 叶子节点）、黑板(Blackboard)、行为树执行器(BehaviorTreeExecutor)、内置行为节点(巡逻/追逐/攻击/等待)、感知系统(视线检测/听觉)、A* 寻路、导航网格、路径跟随 |

### ✅ Phase 3 部分预启 - 游戏主循环与输入绑定

| 模块 | 代码行数 | 完成度 | 说明 |
|------|---------|--------|------|
| `deusex-game` | ~200 | 🟡 35% | 游戏主循环、固定步长 Tick（60Hz）、场景栈管理（push/pop/replace+enter/exit）、输入分发、渲染帧协调 |
| `deusex-input` | ~300 | 🟡 85% | SDL3 输入处理、InputAction 枚举、InputBinding(按键绑定)、InputManager(动作查询/边缘检测/重新绑定)、默认 WASD 绑定 |
| `deusex-ai` | 4 | ⚪ 0% | 占位（AI 行为树、移动寻路、交互） |

---

## 🔧 已实现的核心功能

### 1️⃣ Bytecode 执行系统

- ✅ 解析 UnrealScript 字节码至 AST
- ✅ 虚拟机直接执行 bytecode（无需反编译）
- ✅ 支持跳转、调用、switch case、latent CALL 等控制流指令
- ✅ 完整的值类型系统（Int/Float/Bool/String/Name/Byte/Vector/Rotator/Color/Object）
- 📝 示例：`examples/run_script.rs` 可加载和执行 `.u` 脚本

### 2️⃣ UE1 资源系统

- ✅ 包格式解析（.u/.uax/.ukx/.ut）
- ✅ 名称表与导入/导出表映射
- ✅ 属性序列化（多种 Unreal 属性类型：Object/Int/Array/String/Vector/Rotator/Color）
- ✅ GC（垃圾回收）框架
- ✅ 反射系统（class 注册、UObject 创建）

### 3️⃣ 双模式启动器

- 🎮 **资源检测模式**（`--no-feature gui`）:
  - 自动检测 GOTY / Revision 安装路径
  - 统计 `.u` 包数量
- 🎨 **引擎入口点**（`--features gui`）:
  - SDL3 窗口 + wgpu 渲染设备
  - OpenAL 音频后端
  - 异步 main loop（Tokio::main）
  - 集成 deusex-game 游戏主循环

### 4️⃣ Phase 2 - 媒体子系统（新增）

- 🎨 **deusex-renderer** (wgpu):
  - ✅ GPU 设备/队列/适配器初始化
  - ✅ Swapchain 表面配置与双缓冲呈现
  - ✅ RGBA8 纹理上传（TextureId 句柄管理）
  - ✅ Sprite 渲染管线（顶点着色器 + 片元着色器，Alpha 混合）
  - ✅ 屏幕像素坐标 → NDC 变换
  - ✅ `begin_frame()` / `draw_sprite()` / `render()` 渲染闭环
  - ✅ 窗口大小调整（resize）
- 🔊 **deusex-audio** (OpenAL):
  - ✅ OpenAL 设备/Context 初始化与销毁（RAII）
  - ✅ `AudioBuffer`: WAV 文件解析与 OpenAL 缓冲上传（PCM 8/16-bit, mono/stereo）
  - ✅ `AudioSource`: 音源播放控制（play/stop/pause）、音量(gain)、音调(pitch)、3D 位置、循环(looping)
  - ✅ `AudioManager`: 统一管理缓冲与音源，`load_buffer`/`create_source`/`bind_buffer` 接口
  - ✅ `Listener`: 3D 音频监听者（位置、朝向、速度），`alListenerfv` 方向向量
  - ✅ `UmxPlayer`: .umx 模块音乐播放（libopenmpt 渲染 + OpenAL 双缓冲流式播放）、支持 MOD/XM/S3M/IT/UMX 格式
  - ✅ `probe_module()`: 模块文件格式探测

- 🧱 **deusex-physics** (刚体物理引擎) - 新增:
  - ✅ `PhysicsWorld`: 物理世界，固定步长模拟（默认 60Hz，4 子步）
  - ✅ `RigidBody`: 刚体（位置、速度、质量、惯性、力累加器、弹性/摩擦系数）
  - ✅ `ColliderShape`: 球体、AABB、无限平面三种碰撞体形状
  - ✅ 广阶段：AABB 包围盒剔除
  - ✅ 窗口阶段：球体-球体、AABB-AABB、球体-AABB、球体-平面、AABB-平面碰撞检测
  - ✅ 碰撞响应：基于冲量的法向+摩擦解算、位置校正（防止下陷）
  - ✅ 欧拉积分速度/位置，重力、速度剪裁（防止隧道效应）
  - ✅ `CharacterController`: 角色控制器（WASD 移动、跳跃、重力、地面检测）
  - ✅ `raycast()`: 射线检测（球体/AABB/平面），用于视线检测等

### 🆕 Phase 2 预启 - AI 感知与导航寻路（新增）

- 🧠 **deusex-ai** (AI 行为树与感知寻路) - 🟡 90%:
  - ✅ BehaviorTree: 行为树容器（根节点 + 黑板）
  - ✅ Blackboard: 键值对黑板（Bool/Int/Float/String/Vec3）
  - ✅ BehaviorTreeExecutor: 行为树执行器，每帧 tick
  - ✅ Node trait: 行为节点接口（tick -> Status: Running/Success/Failure）
  - ✅ Sequence/Selector/Inverter/Repeater: 组合与装饰节点
  - ✅ ActionNode/ConditionNode: 叶子节点（用户自定义闭包）
  - ✅ builtin: 内置行为节点（has_target/is_close_to_target/move_to_target/attack_target/patrol/wait）
  - ✅ PerceptionSystem: 感知系统（视线检测+听觉）
  - ✅ PerceptionComponent: 感知组件（FOV、视距、听觉阈值、朝向、位置、视觉/听觉敏感度、FOV 边缘衰减、多点遮挡、HRTF 参数）
  - ✅ Stimulus/PerceptionResult: 感知刺激与结果（支持 SoundType：脚步声/枪声/对话声/爆炸声、HearingInfo 听觉详情）
  - ✅ 视线锥细化：FOV 边缘置信度衰减、距离梯度置信度、多层遮挡检测（遮挡强度 0~1，部分遮挡降低置信度）
  - ✅ 听觉衰减模型：反平方定律（强度 = 基础强度 / (1 + 距离² × 声音类型衰减系数)）、不同声音类型差异衰减、障碍物遮挡额外衰减、强度阈值过滤
  - ✅ 听觉空间化 HRTF：ITD（耳道时间差）、ILD（耳道强度差）、方位角/仰角计算、声源定位置信度
  - ✅ NavGrid: 导航网格（2D 网格，可通过/障碍物，世界↔网格坐标转换）
  - ✅ AStarPathfinder: A* 寻路算法（曼哈顿启发式，四连通）
  - ✅ Path/PathFollower: 路径与路径跟随器（支持路点导航、速度控制、移动方向计算）
  - ✅ 导航网格动态更新：set_rect_obstacle/set_circle_obstacle、clear_obstacle、is_path_invalidated、clear_dirty_regions
  - ✅ 导航网格动态重 triangulation：NavTriangle 三角形网格、build_triangulation/retriangulate_dirty_region（局部重 triangulation）
  - ✅ Crowd 避让：CrowdAgent 代理状态、CrowdManager ORCA 算法（Optimal Reciprocal Collision Avoidance、碰撞时间预测、2D 线性规划速度求解）

### 5️⃣ Phase 3 预启 - 游戏主循环与输入绑定（新增）

- 🎮 **deusex-game**:
  - ✅ 游戏主状态（elapsed 时间、退出请求）
  - ✅ 固定步长 Tick（60 Hz，带累加器）
  - ✅ 场景栈管理（push/pop/replace，enter/exit 生命周期）
  - ✅ 输入分发给当前场景
  - ✅ 渲染帧（变步长 + 插值 alpha）
  - ✅ `run_loop()` 阻塞主循环入口
  - ⏳ 实际游戏场景（主菜单、关卡等）待实现

- 🎮 **deusex-input** (输入绑定系统):
  - ✅ `InputState`: 每帧输入快照（键盘/鼠标/退出/按键边缘）
  - ✅ `InputAction`: 游戏动作枚举（MoveForward/Jump/Use/Fire/Menu 等 16 种）
  - ✅ `InputBinding`: 动作到多个按键的映射
  - ✅ `InputManager`: 动作查询（is_down/pressed/released）、边缘检测(update_edges)、默认 WASD 绑定、运行时重新绑定
  - ✅ SDL3 后端：键盘按下/释放、鼠标移动/按钮、窗口关闭

---

## ⏳ 未实现的关键功能（优先级排序）

| 优先级 | 功能 | 模块 | 状态 |
|--------|------|------|------|
| 🔴 P0 | 渲染闭环（Sprite 绘制 + 双缓冲） | `deusex-renderer` | ✅ 已完成 |
| 🔴 P0 | 游戏主循环与固定步长 Tick | `deusex-game` | ✅ 已完成 |
| 🟠 P1 | 场景管理（栈、enter/exit 生命周期） | `deusex-game` | ✅ 已完成 |
| 🟠 P1 | 纹理上传与 Sprite 渲染管线 | `deusex-renderer` | ✅ 已完成 |
| 🟠 P1 | 音频缓冲与音源播放（WAV/OpenAL） | `deusex-audio` | ✅ 已完成 |
| 🟠 P1 | 输入动作绑定系统（Action→Key） | `deusex-input` | ✅ 已完成 |
| 🟠 P1 | 刚体物理世界与碰撞检测/响应 | `deusex-physics` | ✅ 已完成 |
| 🟡 P2 | 3D 空间音频监听者（Listener） | `deusex-audio` | ✅ 已完成 |
| 🟡 P2 | 角色控制器（移动/跳跃/地面检测） | `deusex-physics` | ✅ 已完成 |
| 🟡 P2 | `.umx` 模块音乐播放（MOD/XM/S3M/IT/UMX） | `deusex-audio` | ✅ 已完成 |
| 🟢 P3 | 物理约束（弹簧、距离、点对点关节） | `deusex-physics` | ✅ 已完成 |
| 🟢 P3 | 物理射线检测（视线检测基础） | `deusex-physics` | ✅ 已完成 |
| 🟡 P2 | AI 感知系统（视线检测、听觉） | `deusex-ai` | ✅ 已完成 |
| 🟡 P2 | A* 导航寻路与路径跟随 | `deusex-ai` | ✅ 已完成 |
| 🟢 P3 | AI 行为树（Sequence/Selector/Decorator） | `deusex-ai` | ✅ 已完成 |
| 🟡 P2 | AI 感知高级特性（FOV 边缘衰减、多点遮挡、反平方听觉衰减） | `deusex-ai` | ✅ 已完成 |
| 🟡 P2 | 导航网格动态更新（矩形/圆形障碍物、路径失效检测） | `deusex-ai` | ✅ 已完成 |
| 🟡 P2 | AI 感知高级特性（HRTF 听觉空间化、多层遮挡检测） | `deusex-ai` | ✅ 已完成 |
| 🟡 P2 | 导航网格动态重 triangulation（局部重建） | `deusex-ai` | ✅ 已完成 |
| 🟡 P2 | Crowd 避让（ORCA 算法、碰撞时间预测、2D 线性规划） | `deusex-ai` | ✅ 已完成 |
| 🟢 P3 | 物理引擎高级特性（软体、流体） | `deusex-physics` | ⏳ 待实现 |
| 🟢 P3 | AI 感知最高级（HRTF 颜色过滤、多声源分离） | `deusex-ai` | ⏳ 待实现 |
| 🟢 P3 | 导航网格动态重 triangulation 优化（增量式更新） | `deusex-ai` | ⏳ 待实现 |
| 🟢 P3 | Crowd 避让（完整 ORCA、群体流、领袖追随） | `deusex-ai` | ⏳ 待实现 |

---

## 📚 技术架构

### 依赖关系图

```
deusex-bytecode  →  deusex-ast
     ↓
deusex-vm       →  deusex-bytecode, deusex-package
                 →  deusex-runtime
deusex-codegen   →  deusex-bytecode, deusex-ast
deusex-launcher  →  deusex-input, deusex-renderer, deusex-audio, deusex-vm

Boxed dependencies (features):
deusex-launcher --gui-> deusex-input --sdl3-> serde|clap
deusex-launcher --gui-> deusex-renderer --wgpu---> wasm32 or x86_64
deusex-launcher --gui-> deusex-audio --openal--> openal-sys
```

### 关键 README 文件

| 文件 | 说明 |
|------|------|
| `README.md` (当前) | 快速开始与简要说明 |
| `PROGRESS.md` (本文档) | 详细进度追踪 |
| `CRATES.md` | 各 crate 详细说明 |
| `AUDIENCE.md` | 设计意图与技术选型 |

---

## 🧪 测试状态

| 测试类型 | 状态 | 说明 |
|---------|------|------|
| 单元测试 | ⚠️ 部分完成 | `bytecode/`, `codegen/` 模块有测试；`integration/` 为空 |
| 集成测试 | ⚪ 未实现 | nil tests |
| 性能测试 | ⚪ 未实现 | nil |
| CI/CD | ❌ 未配置 | GitHub Actions 待添加 |

---

## 🚦 状态标签

- ✅ 100% 完成
- 🟡 70-90% 完成
- 🟠 30-70% 完成
- ⚪ <30% 或空模块

---

## 📅 时间线

| 日期 | 事件 |
|------|------|
| 2024-10 | 项目启动，创建 Rust 工作区 |
| 2024-10-xx | 实现 `deusex-bytecode`, `deusex-vm` 核心模块 |
| 2024-11-05 | 完成 `deusex-package`, `deusex-launcher` |
| 2024-11-06 | 当前状态：阶段性成果文档记录完成 |

---

## 📋 待办事项

### 短期任务（1-2 周）

- [ ] 补全 `deusex-ast` 缺失的 AST 类型
- [ ] 实现 `deusex-renderer` 基础渲染循环
- [ ] 在 CI 中验证 `bytecode` 模块测试覆盖率
- [ ] 添加 CI/CD 配置文件（GitHub Actions）

### 中期任务（1 个月）

- [ ] 实现 `deusex-game` 游戏主循环
- [ ] 实现 `deusex-audio` 音频管线
- [ ] 实现常见 UI 组件（HUD、菜单）
- [ ] 完成键盘/鼠标输入绑定

### 长期任务（2-3 个月）

- [ ] 实现 `deusex-physics` 物理引擎
- [ ] 实现完整 AI 行为树
- [ ] 多人联机支持
- [ ] 性能优化与内存优化

---

## 🔗 参考资源

- [Unreal Engine 1 Docs](https://docs.unrealengine.com/5.0/zh-CN/unreal-engine-1-blueprints-in-unreal-engine-5/)
- [UnrealScript Bytecode Spec](https://github.com/Polynomial-Architected-Code/UE1-Bytecode-Std)
- [wgpu Documentation](https://docs.rs/wgpu/latest/wgpu/)
- [OpenAL Documentation](https://openal.org/documentation/)
- [SDL3 Documentation](https://wiki.libsdl.org/SDL3/)

---

## 📝 更新日志

### 2024-11-06

- 创建 `PROGRESS.md` 作为项目的状态追踪文档
- 记录 4 个已完成模块、3 个进行中模块、3 个未开始模块
- 绘制依赖关系图及技术架构说明
- 添加待办清单与优先级排序

---

**最后更新**: 2024-11-06
**维护者**: Deus Ex Native Team