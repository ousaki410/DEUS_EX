# P1 - 关键包反编译伪代码（已完成）

## 目标

将 `Core`、`Engine`、`DeusEx` 等关键包反编译为**保留符号名的线性 goto 伪代码**，供 P2/P3 静态分析与 native 交叉验证使用。

## 设计决策

- **不复用 `deusex-bytecode::decompile_package`**：现有 AST 反编译会丢失函数名、属性名和对象路径（只剩数字 ID），不适合直接产出可读伪代码。
- 新增 `deusex-bytecode::pseudocode` 线性渲染器：直接对解码后的 `Expr` 树渲染，保留变量名、属性名、对象路径、native 名称及常见运算符（`==`、`!=`、`<`、`>`、`+`、`$`、`++`、`--`、`&&`、`||` 等）。
- 每个跳转目标分配标签（`L0040` / 命名标签），语义与原字节码一致，无需重建结构化控制流。
- `JumpIfNot` 的条件做否定化简（如 `Parms != ""` → `if (Parms == "") goto`），`Not_PreBool` 双重否定抵消。
- `Assert` 的 `offset` 是源码行号而非跳转目标，渲染为 `assert(...) // line N`，不生成标签。
- `Case(0xFFFF)` 为默认分支，不生成标签。
- native 桩函数体只读参数（`NativeParm`），渲染为声明；例如 `static native function int Add_IntInt(A: int, B: int); // native index 146`。
- `ArrayElement` 操作数顺序修正为“索引在前、数组在后”，`Object.Sprintf` 正确渲染为 `S[0] = s0;`。
- State 内函数按 `包.类.State.函数` 归因（如 `DeusEx.AlarmUnit.Active.TakeDamage`）。

## 命令行

从仓库根目录运行（GOTY 资源需已解压到 `goty/app/system`）：

```bash
cd deusex-native
cargo run --release -p deusex-reverse -- \
  ../goty/app/system /tmp/opencode/p1_pseudo --all --decompile
```

- `--decompile`：为每个包生成 `pseudo/<package>.pseudo.md`、`pseudo/index.csv`、`pseudo/SUMMARY.md`。
- 约 6 秒解析全部 38 个包、8208 个函数，生成约 10.8 万行伪代码。

## 提交范围（本次）

| 文件 | 说明 |
|------|------|
| `docs/reverse/P1_SUMMARY.md` | 本总结（本文件） |
| `docs/reverse/pseudo/index.csv` | 8208 行，每行：包、函数、类、State、伪代码行数 |
| `docs/reverse/pseudo/SUMMARY.md` | 各包函数数/行数/文件汇总 |
| `docs/reverse/pseudo/Core.pseudo.md` | `Core.u` 全量伪代码（示例） |
| `docs/reverse/pseudo/Engine.pseudo.md` | `Engine.u` 全量伪代码（示例） |

完整伪代码（38 个包，约 3.9 MB，`DeusEx.pseudo.md` 单独约 2.4 MB）保留在 `/tmp/opencode/p1_pseudo/pseudo/`，未提交，复现命令见上文。

## 统计（基于 `goty/app/system`，38 个包）

| 指标 | 数值 |
|------|------|
| 包 | 38 |
| 函数 | 8208（native 891） |
| State 内函数 | 697 |
| 伪代码总行数 | 约 10.8 万行 |
| 非空包 | 33 个 |
| 最大单包 | `DeusEx`（4933 函数，约 7.4 万行） |

## 已知限制

1. **跨包 native 索引复用**：UE1 在不同包中可复用同一 native 索引（如 `2054` 分别为 `Engine.Actor.GetSoundLength` 与 `DeusEx.ConBindEvents.ClearBindEvents`）。`NativeTable` 保留首个声明，后续重复索引渲染为首个名称，不影响 native 桩声明本身。
2. **线性 goto 形式**：未重建结构化控制流（`if/while/switch`），适合静态阅读与交叉验证，不适合直接生成可执行代码。
3. **DynArrayElement**：未找到实际样本验证解析顺序，暂保持原顺序；若后续发现样本与 `ArrayElement` 不同，再调整。

## 下一步（P2）

用 IDA/Ghidra 对 native 函数做交叉验证，记录地址与签名映射，并用 P1 伪代码作为 native 行为的静态参考。
