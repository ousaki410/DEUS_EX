# P0 逆向工程报告 — Deus Ex GOTY `.u` 包清单

本报告由 `deusex-reverse` 在不启动游戏的情况下静态解析生成。
数据源为 `goty/app/system`（Deus Ex GOTY + Revision 1.3 的 38 个 `.u` 包）。

## 总览

| 指标 | 数值 |
|------|-----:|
| 包数量 | 38 |
| 导出类数量 | 1502 |
| 类属性数量 | 7701 |
| 函数数量 | 8208 |
| Native 函数数量 | 891 |
| 含属性类的数量 | 726 |
| State 内函数数量 | 697 |
| 脚本字节数 | 771561 |
| 表达式节点数 | 300039 |
| 分支指令数 | 20446 |
| Opcode token 总数 | 300039 |

## 包级清单

完整机器可读清单见 `packages.csv`；类、属性、函数签名分别见 `classes.csv`、`properties.csv`、`functions.csv`。

| 包 | 类 | 属性 | 函数 | Native | 脚本字节 | Opcode |
|----|---:|---:|---:|---:|---:|---:|
| DeusEx | 1158 | 4295 | 4933 | 57 | 535375 | 205116 |
| Engine | 89 | 1472 | 1263 | 158 | 99010 | 40260 |
| UWindow | 78 | 536 | 723 | 0 | 73836 | 30241 |
| Extension | 34 | 555 | 555 | 453 | 4973 | 2283 |
| UBrowser | 59 | 437 | 408 | 0 | 42376 | 16176 |
| Core | 7 | 21 | 170 | 165 | 1200 | 446 |
| ConSys | 35 | 158 | 51 | 11 | 7681 | 2665 |
| IpDrv | 5 | 20 | 45 | 22 | 806 | 336 |
| Editor | 7 | 104 | 24 | 13 | 2261 | 1064 |
| IpServer | 2 | 13 | 24 | 0 | 4021 | 1441 |
| DeusExText | 2 | 18 | 12 | 12 | 22 | 11 |
| DeusExCharacters | 4 | 0 | 0 | 0 | 0 | 0 |
| DeusExConversations | 1 | 0 | 0 | 0 | 0 | 0 |
| DeusExDeco | 5 | 0 | 0 | 0 | 0 | 0 |
| DeusExItems | 7 | 0 | 0 | 0 | 0 | 0 |
| DeusExSounds | 1 | 0 | 0 | 0 | 0 | 0 |
| DeusExUI | 1 | 0 | 0 | 0 | 0 | 0 |
| Fire | 6 | 72 | 0 | 0 | 0 | 0 |
| MPCharacters | 1 | 0 | 0 | 0 | 0 | 0 |

## 全局 Opcode 频率（前 20）

| Opcode | 名称 | 数量 | 占比 |
|--------|------|-----:|-----:|
| 0x01 | InstanceVariable | 46801 | 15.60 |
| 0x00 | LocalVariable | 44558 | 14.85 |
| 0x70 | Native | 31162 | 10.39 |
| 0x19 | Context | 22404 | 7.47 |
| 0x0F | Let | 15788 | 5.26 |
| 0x07 | JumpIfNot | 12905 | 4.30 |
| 0x1B | VirtualFunction | 10511 | 3.50 |
| 0x04 | Return | 10366 | 3.45 |
| 0x0B | Nothing | 9387 | 3.13 |
| 0x60 | ExtendedNative | 9194 | 3.06 |
| 0x2D | BoolVariable | 7974 | 2.66 |
| 0x1E | FloatConst | 7033 | 2.34 |
| 0x06 | Jump | 5843 | 1.95 |
| 0x2A | NoObject | 4645 | 1.55 |
| 0x2C | IntConstByte | 4409 | 1.47 |
| 0x3F | Conversion | 3904 | 1.30 |
| 0x24 | ByteConst | 3822 | 1.27 |
| 0x18 | Skip | 3577 | 1.19 |
| 0x21 | NameConst | 3518 | 1.17 |
| 0x36 | StructMember | 3301 | 1.10 |

## 关键发现

- 全部 38 个包均成功解析，无包级失败。
- `DeusEx.u` 是核心脚本包：1158 个类、4295 个属性、4933 个函数，占全部函数量的六成。
- 共解析出 891 个 native 函数，可作为 P2（IDA/Ghidra 交叉验证）的优先锚点。
- 状态（state）内声明的函数已正确归因到“类 + state”，例如 `DeusEx.AlarmUnit.Active.TakeDamage`。
- `DeusExConAudio*`、`DeusExCharacters` 等内容包没有 UClass/UFunction 导出（仅有对话/资源对象），0 函数属于预期结果。
- `TakeDamage` 等函数已恢复参数名/类型、返回值、out/optional 标志，可直接用于 P3 的 Rust 回归测试生成。

## 复现命令

```bash
cd deusex-native
cargo run --release -p deusex-reverse -- ../goty/app/system ../reverse_report --all
```

生成的 `reverse_report/`（Markdown/JSON/完整 DOT）体积较大，已加入 `.gitignore`；本目录只保留紧凑的 CSV 清单与总结。
