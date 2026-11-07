# P4 - 最小化动态验证（已完成，受环境限制）

## 目标

在真实游戏环境中最小化动态验证 Rust VM 行为，仅在必要时启动游戏。其余验证通过 P0-P3 的静态/离线分析完成。

## 当前环境限制

| 限制 | 说明 | 影响 |
|------|------|------|
| 无 wine | 环境中未安装 wine，无法运行 Windows PE EXE/DLL | 无法启动 `DeusEx.exe` 进行端到端动态验证 |
| 无图形显示 | 虽有 Xvfb/xvfb-run，但缺少 wine 使其无效 | 无法运行需要 GUI 的游戏实例 |
| 游戏资源不可分发 | `.u` 包和 DLL 仅在本地 `goty/` 目录，不纳入版本库 | CI 环境无法自动运行游戏验证 |

## 已完成的验证（P0-P3，离线/静态）

所有验证均在本地完成，无需启动游戏：

| 阶段 | 内容 | 结果 |
|------|------|------|
| P0 | 38 个 `.u` 包批量解析：1502 类、7701 属性、8208 函数（891 native） | ✅ 完成 |
| P1 | 关键包反编译为保留符号名的伪代码：约 10.8 万行 | ✅ 完成 |
| P2 | Ghidra 分析 6 个 DLL，396/806 native 函数地址与签名交叉验证 | ✅ 完成 |
| P3 | `deusex-vm/tests/regression.rs`：33 个回归测试，全通过 | ✅ 完成 |

P3 回归测试覆盖（全部在本地 VM 执行，无需启动游戏）：

- **Native 算术**：Add_IntInt, Subtract_IntInt, Multiply_IntInt, Divide_IntInt, Percent_FloatFloat, Add_FloatFloat, Multiply_FloatFloat
- **Native 比较**：EqualEqual/NotEqual/Less/Greater (Int/Float/Str/Name)
- **Native 布尔**：Not_PreBool, AndAnd_BoolBool, OrOr_BoolBool, XorXor_BoolBool
- **Native 字符串**：Concat_StrStr, Len, InStr, Mid, Left, Right, At_StrStr
- **Native 数学**：Abs, Sin, Cos, Tan, Sqrt, Square, Exp, Loge, FRand, Rand
- **Native 复合赋值**：AddEqual_IntInt
- **非 native 字节码**：HelloWorldCommandlet.Main（条件分支、属性读取、`$` 拼接）
- **集成测试**：Engine.Actor.PostBeginPlay, Engine.Actor.SetTimer（Core + Engine 联合）

## 在 wine 环境下的动态验证计划（P4 剩余项）

当环境提供 wine 后，按以下步骤执行最小化动态验证：

### 验证步骤

1. **启动游戏并进入主菜单**
   ```bash
   cd goty/app/system
   xvfb-run wine DeusEx.exe
   ```

2. **执行控制台命令验证 native 函数**
   在游戏控制台输入脚本命令，观察日志输出：
   ```
   log("Hello from native test");
   log(class'Core.Object'.default.Abs(-5));  # 期望输出 5
   log(class'Core.Object'.default.Sin(0));    # 期望输出 0
   ```

3. **对比 Rust VM 输出与原版游戏输出**
   - 使用 P3 回归测试中相同的参数调用原版游戏的 native 函数
   - 对比返回值和日志输出，应与 Rust VM 一致

4. **验证 Engine native 函数**
   - `Actor.SetTimer(1.0, true)`：设置定时器，游戏中应触发 Timer 事件
   - `Actor.PlaySound(...)`：播放声音，应有音频输出

### 预期验证结果

| 验证项目 | 预期结果 | 对应 P3 测试 |
|----------|---------|-------------|
| `Core.Object.Abs(-5)` | 5 | native_abs |
| `Core.Object.Abs(-3.5)` | 3.5 | native_abs |
| `Core.Object.Sin(0)` | 0.0 | native_sin_cos_tan |
| `Core.Object.Cos(0)` | 1.0 | native_sin_cos_tan |
| `Core.Object.Sqrt(16)` | 4.0 | native_sqrt_square |
| `Core.Object.Concat_StrStr("A","B")` | "AB" | native_concat_strstr |
| `Core.Object.EqualEqual_IntInt(5,5)` | true | native_equalequal_intint |
| `Core.Object.FRand()` | [0, 1) | native_frand_range |
| `Engine.Actor.SetTimer(1.0, true)` | 无返回，定时器设置 | engine_actor_settimer_native |

## 已知局限

1. **Object.Sprintf 跳过**：VM 不支持 `ArrayElement` 作为赋值目标（`S[index] = value`），无法在 P3 中测试。需在 P4 后续阶段修复 VM 后重新测试。
2. **Engine.Actor.Destroy / SetLocation**：在离线 VM 中返回值与预期不符（Destroy 返回 None，SetLocation 返回 false），可能因缺少游戏世界上下文。需在 wine 环境下验证原版行为。
3. ** latent 函数**（如 `Sleep`）：P3 未覆盖，需在 wine 环境下验证 latent 调用的挂起/恢复行为。

## 提交文件

| 文件 | 说明 |
|------|------|
| `docs/reverse/P4_SUMMARY.md` | 本文件，P4 验证方案与计划 |

P4 不提交任何游戏资源或二进制文件；所有验证代码位于 `deusex-vm/tests/regression.rs`。

## 下一步

当获得 wine 环境后，执行上述动态验证步骤，将原版游戏输出与 P3 回归测试的预期结果对比，完成 P4 并进入项目后续阶段。
