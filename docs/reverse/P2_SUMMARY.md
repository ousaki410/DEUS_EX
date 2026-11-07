# P2 - Native 函数地址与签名交叉验证（已完成）

## 目标

用 IDA/Ghidra 对原版 Deus Ex 二进制（DLL/EXE）中的 native 函数做地址与签名交叉验证，建立 **native 索引 → DLL 地址 → 脚本签名** 的映射表，供 P3 静态分析与 Rust 回归测试使用。

## 方法

1. 使用 Ghidra Headless Analyzer (`analyzeHeadless`) 对 GOTY 目录下的关键 DLL 进行自动分析（函数识别、交叉引用）。
2. 在每个 DLL 的数据段中扫描 `GNatives` 函数指针数组（连续指向 `.text` 段中函数入口的 4 字节指针）。
3. 将每个数组条目反解为 Ghidra 识别的函数入口地址和名称（如 `execPlaySound`、`execSpawn`）。
4. 通过函数名模糊匹配（去掉 `exec` 前缀、下划线、大小写不敏感），将 P1 伪代码清单中的 native 函数与二进制地址关联。

## 分析的二进制文件

| DLL | 基址 | .text 范围 | GNatives 数组 | 条目数 | 有效条目 |
|-----|------|-----------|--------------|--------|---------|
| Core.dll | 0x10100000 | 0x10101000–0x10174fff | 0x101AA684 | 900 | 255 |
| Engine.dll | 0x10300000 | 0x10301000–0x10425fff | 0x1047CA98 | 142 | 142 |
| DeusEx.dll | 0x10000000 | 0x10001000–0x10026fff | 0x100342B4 | 21 | 21 |
| Extension.dll | 0x10000000 | 0x10001000–0x10058fff | 0x10098F58 | 90 | 90 |
| ConSys.dll | 0x10000000 | 0x10001000–0x10011aff | 0x1001DB10 | 7 | 7 |
| DeusExText.dll | 0x10000000 | 0x10001000–0x10006652 | 0x1000A0EC | 12 | 12 |

> 注：`GNatives` 数组条目数可能大于有效函数入口数，部分槽位为空或指向非函数数据。

## 交叉验证结果

P1 共识别出 806 个 native 函数（805 个唯一索引）。通过函数名匹配，396 个 (49%) 已在地址级验证：

| 包 | P1 native 数 | 已匹配地址 | 匹配率 | 未匹配主要原因 |
|-----|-------------|-----------|--------|--------------|
| Core | 154 | 150 | 97% | DebugInfo 高索引 native (4000–4003) |
| Engine | 119 | 105 | 88% | 部分特定 native 未在 Engine.dll GNatives 中 |
| DeusExText | 12 | 12 | 100% | — |
| DeusEx | 57 | 21 | 37% | 大部分 native 实现在 Engine.dll 或其他模块 |
| ConSys | 11 | 7 | 64% | ConEvent 相关 native 未找到 |
| Extension | 453 | 101 | 22% | 多数 native 在其他 DLL 或命名差异较大 |
| **总计** | **806** | **396** | **49%** | — |

### 匹配示例

| P1 native_index | 全名 | DLL | GNatives 索引 | 二进制地址 | Ghidra 函数名 |
|-----------------|------|-----|--------------|-----------|-------------|
| 146 | Core.Object.Add_IntInt | Core.dll | 85 | 0x10103044 | execAdd_IntInt |
| 129 | Core.Object.Not_PreBool | Core.dll | 66 | 0x10101604 | execNot_PreBool |
| 254 | Core.Object.EqualEqual_NameName | Core.dll | 210 | 0x101014ce | execEqualEqual_NameName |
| 264 | Engine.Actor.PlaySound | Engine.dll | 70 | 0x10302612 | execPlaySound |
| 278 | Engine.Actor.Spawn | Engine.dll | 73 | 0x103011ef | execSpawn |
| 279 | Engine.Actor.Destroy | Engine.dll | 72 | 0x10302324 | execDestroy |
| 280 | Engine.Actor.SetTimer | Engine.dll | 71 | 0x103012d5 | execSetTimer |

完整映射表：`/tmp/opencode/p2_native_mapping.csv`（396 行）。

## 已知限制

1. **native_index 非数组索引**：P1 中的 `native_index` 是脚本字节码中的 native 操作数，不直接等于 `GNatives` 数组索引。地址映射通过函数名模糊匹配建立，可能存在误匹配（如跨包同名函数）。
2. **未匹配的 native**：410 个 native 未在地址级验证，可能实现在未分析的 DLL（Render.dll、WinDrv.dll、SoftDrv.dll 等）中，或使用 `execHighNative` 机制（索引 0–15）。
3. **跨 DLL 同名**：部分函数名在多个 DLL 中出现（如 `PlaySound`），取首个匹配，可能不准确。

## 下一步（P3）

用 P1 伪代码和 P2 地址映射驱动 Rust 回归测试，对比 VM 行为与原版 native 实现。
