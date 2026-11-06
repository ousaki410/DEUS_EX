# 贡献指南

感谢你对 Deus Ex Native 项目的兴趣！本文档将帮助你了解如何为项目做出贡献。

---

## 📋 准备工作

### 1. Fork 仓库

1. 访问 GitHub 仓库页面
2. 点击右上角的 "Fork" 按钮
3. 将你的 Fork 克隆到本地：

```bash
git clone https://github.com/YOUR_USERNAME/deus_ex_native.git
cd deus_ex_native/deusex-native
```

### 2. 安装开发依赖

```bash
# 更新 Rust 工具链
rustup update

# 安装稳定工具链（推荐）
rustup default stable

# 可选：安装 clippy 和 rustfmt
rustup component add clippy rustfmt

# 安装 SDL3 和 OpenAL（GUI 模式需要）
# Ubuntu/Debian
sudo apt install libsdl3-dev libopenal-dev

# macOS
brew install sdl3 openal-soft
```

---

## 🚀 开发流程

### 1. 创建功能分支

```bash
# 从主分支创建新分支
git checkout -b feature/your-feature-name

# 分支命名规范
# - feature: 新功能
# - fix: 修复 bug
# - docs: 文档更新
# - refactor: 代码重构
# - test: 测试相关
# - chore: 杂项（依赖更新、构建脚本等）
```

### 2. 进行更改

1. **提交前检查**
   ```bash
   # 格式化代码
   cargo fmt

   # 运行 clippy 检查
   cargo clippy --all-targets --all-features
   ```

2. **运行测试**
   ```bash
   # 运行所有测试
   cargo test --all-targets

   # 运行测试并显示详细输出
   cargo test --all-targets -- --nocapture

   # 运行特定测试
   cargo test -p deusex-vm test_vm_function
   ```

3. **构建项目**
   ```bash
   # 构建 debug 版本
   cargo build

   # 构建 release 版本
   cargo build --release
   ```

### 3. 提交更改

```bash
# 添加更改
git add .

# 提交（使用有意义的信息）
git commit -m "feat: Add new feature description"

# Commit 信息规范：
# feat: 新功能
# fix: 修复 bug
# docs: 文档更新
# refactor: 代码重构
# test: 添加测试
# chore: 构建/工具更新
# style: 代码格式化
# perf: 性能优化
```

### 4. 推送并创建 PR

```bash
# 推送到你的 fork
git push origin feature/your-feature-name

# 在 GitHub 上创建 Pull Request
```

**PR 模板要求**

PR 描述应包含：

```markdown
## 变更说明

简短描述这次 PR 做了什么。

**相关问题**: Closes #123

## 测试情况

- [ ] 添加了测试
- [ ] 测试通过
- [ ] 无需测试（纯文档/refactor）

## 截图 / 视频（如适用）

添加相关截图或视频。

## 自查清单

- [ ] 运行了 `cargo fmt`
- [ ] 运行了 `cargo clippy`
- [ ] 运行了 `cargo test`
- [ ] 代码符合项目编码规范
- [ ] 添加了必要的文档注释
- [ ] 更新了相关文档（README.md, PROGRESS.md）
```

---

## 🐛 报告 Bug

### Bug 报告模板

```markdown
## 描述

简短的 bug 描述。

## 重现步骤

1. 步骤 1
2. 步骤 2
3. 步骤 3

## 期望行为

应该发生什么？

## 实际行为

实际发生了什么？

## 复制代码

如果有相关的代码片段，请提供：

```bash
# 示例命令
cargo build --release --features gui
```

## 环境信息

- OS: [例如: Ubuntu 22.04]
- Rust version: `rustc --version`
- 前端依赖: SDL3/OpenAL/wgpu 版本

## 日志输出

粘贴相关错误日志或输出。

## 额外信息

其他有用的信息。
```

---

## 📝 贡献指南

### 代码风格

- 遵循 Rust 官方代码风格：`cargo fmt`
- 使用 `cargo clippy` 检查潜在问题
- 函数和类型要有清晰的文档注释 (`///`)
- 变量命名使用俄罗斯的名词单数形式，必要时加后缀 (例如：intparm/diffparm)

### 添加新特性

1. 在相关 crate 中添加新模块
2. 更新 `Cargo.toml` 中的依赖（如有）
3. 编写测试用例
4. 更新文档
5. 更新 `PROGRESS.md`

### 添加新模块

建议的模块结构：

```
crates/new-module/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── mod_*.rs
    └── tests/
```

---

## 💡 重要原则

### 保持简洁

- 不要过度设计
- 如果不确定，使用最简单的实现
- 优先考虑可读性

### 向前兼容

尽量避免破坏性更改。如果必须，请提供迁移路径。

### 文档优先

- 重要的 API 必须有文档注释
- README.md 应该易于理解
- 复杂逻辑应有详细说明

---

## 🤝 社区准则

- 尊重所有贡献者
- 积极讨论，保持礼貌
- 提供建设性的反馈
- 避免人身攻击

---

## 📞 需要帮助？

如果你有任何问题：

1. 查看 [FAQ](#TODO)
2. 搜索现有的 Issues
3. 在 [Discussions](#TODO) 中发帖
4. 联系维护者

---

## 🎯 愿意贡献？

我们欢迎各种类型的贡献：

- 🐛 Report bugs
- 💡 提出新功能
- 📝 文档改进
- 💪 代码贡献
- 🧪 添加测试

请记住：每一点贡献都很重要！感谢你的帮助！✨

---

**最后更新**: 2024-11-06