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
- GPU 设备管理
- 窗口集成
- 渲染循环
- 资源加载 (纹理)

**状态**: 🟡 40% 完成（只有框架，无实际绘制）

**使用示例**:
```rust
use deusex_renderer::{Gpu, Renderer};

let gpu = Gpu::new().await?;
let renderer = Renderer::new(gpu, surface, 1280, 720, format)?;
loop {
    renderer.render()?;
}
```

---

### deusex-audio

**职责**: 音频后端（基于 OpenAL）

**主要功能**:
- 音频设备管理
- 声音播放
- 音频通道管理

**状态**: 🟡 30% 完成（只有框架）

---

### deusex-input

**职责**: 输入系统（基于 SDL3）

**主要功能**:
- 键盘输入处理
- 鼠标输入处理
- 游戏手柄支持

**状态**: ✅ 70% 完成（基础输入处理）

**使用示例**:
```rust
use deusex_input::sdl::{SdlContext, InputState};

let mut sdl = SdlContext::init()?;
let mut state = InputState::default();
sdl.pump(&mut state);
```

---

## 🎮 游戏逻辑

### deusex-game

**职责**: 游戏主逻辑

**主要功能**:
- Game Loop 管理
- Tick 处理
- 场景管理
- AI 系统
- 玩家控制

**状态**: ⚪ 0% 完成（仅有占位函数）

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