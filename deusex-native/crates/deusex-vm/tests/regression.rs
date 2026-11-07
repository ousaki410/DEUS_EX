//! P3 回归测试：用静态分析结果驱动 Rust VM 行为对照。
//!
//! 测试覆盖：
//! 1. Core.u 中 P2 已验证地址的 native 函数（算术、比较、布尔、字符串、数学）
//! 2. Core.u 中 P1 伪代码揭示逻辑的非 native 函数（HelloWorldCommandlet.Main）
//! 3. Core + Engine 联合集成测试
//!
//! 已知限制（VM 未实现，测试跳过）：
//! - Object.Sprintf：VM 的 `assign` 不支持 `ArrayElement` 作为赋值目标，
//!   导致 `S[index] = value` 无法执行（P1 伪代码中明确使用此模式）。
//! - 此限制将在 P3 后续阶段随 VM 扩展而修复。
//!
//! 所有测试在 GOTY 资源不可用时自动跳过；游戏资源从不纳入版本库。

use std::path::PathBuf;

use deusex_vm::{PackageUnit, Value, Vm};

fn system_dir() -> Option<PathBuf> {
    let candidates = [
        std::env::var_os("DEUSEX_GOTY_PATH").map(PathBuf::from),
        Some(PathBuf::from("/home/ou/Projects/deus_ex_goty/goty/app/system")),
    ];
    for candidate in candidates.into_iter().flatten() {
        let system = if candidate.ends_with("system") {
            candidate
        } else {
            candidate.join("system")
        };
        if system.join("Core.u").is_file() {
            return Some(system);
        }
    }
    None
}

/// 加载 Core.u 并返回 Vm。
fn core_vm() -> Option<Vm> {
    let system = system_dir()?;
    let core = PackageUnit::load(system.join("Core.u")).expect("load Core.u");
    assert!(
        core.warnings.is_empty(),
        "unexpected package warnings: {:?}",
        core.warnings
    );
    let mut vm = Vm::new();
    vm.add_unit(core);
    Some(vm)
}

/// 加载 Core.u + Engine.u 并返回 Vm。
fn core_engine_vm() -> Option<Vm> {
    let system = system_dir()?;
    let core = PackageUnit::load(system.join("Core.u")).expect("load Core.u");
    let engine = PackageUnit::load(system.join("Engine.u")).expect("load Engine.u");
    let mut vm = Vm::new();
    vm.add_unit(core);
    vm.add_unit(engine);
    Some(vm)
}

/// 查找脚本函数并执行，返回结果。
fn exec_fn(vm: &mut Vm, path: &str, self_object: Option<u32>, args: Vec<Value>) -> Value {
    let (unit_index, function_index) = vm
        .find_function(path)
        .unwrap_or_else(|| panic!("function {path} not found in loaded packages"));
    vm.exec_function(unit_index, function_index, self_object, args)
        .unwrap_or_else(|e| panic!("execute {path}: {e:?}"))
        .return_value
}

/// 浮点数近似相等（考虑 f32 精度）。
fn float_eq(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.001
}

/// ---------------------------------------------------------------------------
// 1. Native 算术运算（P2 地址已验证）
// ---------------------------------------------------------------------------

#[test]
fn native_add_intint() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    let result = exec_fn(&mut vm, "Core.Object.Add_IntInt", None, vec![Value::Int(3), Value::Int(5)]);
    assert_eq!(result, Value::Int(8));
}

#[test]
fn native_subtract_intint() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    let result = exec_fn(&mut vm, "Core.Object.Subtract_IntInt", None, vec![Value::Int(10), Value::Int(3)]);
    assert_eq!(result, Value::Int(7));
}

#[test]
fn native_multiply_intint() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    let result = exec_fn(&mut vm, "Core.Object.Multiply_IntInt", None, vec![Value::Int(6), Value::Int(7)]);
    assert_eq!(result, Value::Int(42));
}

#[test]
fn native_divide_intint() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    let result = exec_fn(&mut vm, "Core.Object.Divide_IntInt", None, vec![Value::Int(20), Value::Int(4)]);
    assert_eq!(result, Value::Int(5));
}

#[test]
fn native_percent_floatfloat() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    // Core.u 中只有 Percent_FloatFloat（索引 173），无 Percent_IntInt
    let result = exec_fn(&mut vm, "Core.Object.Percent_FloatFloat", None, vec![Value::Float(5.5), Value::Float(2.0)]);
    assert!(float_eq(result.as_float().unwrap(), 1.5));
}

#[test]
fn native_add_floatfloat() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    let result = exec_fn(&mut vm, "Core.Object.Add_FloatFloat", None, vec![Value::Float(1.5), Value::Float(2.5)]);
    assert!(float_eq(result.as_float().unwrap(), 4.0));
}

#[test]
fn native_multiply_floatfloat() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    let result = exec_fn(&mut vm, "Core.Object.Multiply_FloatFloat", None, vec![Value::Float(3.0), Value::Float(4.0)]);
    assert!(float_eq(result.as_float().unwrap(), 12.0));
}

// ---------------------------------------------------------------------------
// 2. Native 比较运算（P2 地址已验证）
// ---------------------------------------------------------------------------

#[test]
fn native_equalequal_intint() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    assert_eq!(
        exec_fn(&mut vm, "Core.Object.EqualEqual_IntInt", None, vec![Value::Int(5), Value::Int(5)]),
        Value::Bool(true)
    );
    assert_eq!(
        exec_fn(&mut vm, "Core.Object.EqualEqual_IntInt", None, vec![Value::Int(5), Value::Int(6)]),
        Value::Bool(false)
    );
}

#[test]
fn native_notequal_intint() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    assert_eq!(
        exec_fn(&mut vm, "Core.Object.NotEqual_IntInt", None, vec![Value::Int(5), Value::Int(6)]),
        Value::Bool(true)
    );
    assert_eq!(
        exec_fn(&mut vm, "Core.Object.NotEqual_IntInt", None, vec![Value::Int(5), Value::Int(5)]),
        Value::Bool(false)
    );
}

#[test]
fn native_less_intint() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    assert_eq!(
        exec_fn(&mut vm, "Core.Object.Less_IntInt", None, vec![Value::Int(3), Value::Int(5)]),
        Value::Bool(true)
    );
    assert_eq!(
        exec_fn(&mut vm, "Core.Object.Less_IntInt", None, vec![Value::Int(5), Value::Int(3)]),
        Value::Bool(false)
    );
}

#[test]
fn native_greater_intint() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    assert_eq!(
        exec_fn(&mut vm, "Core.Object.Greater_IntInt", None, vec![Value::Int(5), Value::Int(3)]),
        Value::Bool(true)
    );
    assert_eq!(
        exec_fn(&mut vm, "Core.Object.Greater_IntInt", None, vec![Value::Int(3), Value::Int(5)]),
        Value::Bool(false)
    );
}

#[test]
fn native_equalequal_floatfloat() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    assert_eq!(
        exec_fn(
            &mut vm,
            "Core.Object.EqualEqual_FloatFloat",
            None,
            vec![Value::Float(1.0), Value::Float(1.0)]
        ),
        Value::Bool(true)
    );
    assert_eq!(
        exec_fn(
            &mut vm,
            "Core.Object.EqualEqual_FloatFloat",
            None,
            vec![Value::Float(1.0), Value::Float(2.0)]
        ),
        Value::Bool(false)
    );
}

#[test]
fn native_equalequal_strstr() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    assert_eq!(
        exec_fn(
            &mut vm,
            "Core.Object.EqualEqual_StrStr",
            None,
            vec![
                Value::String("hello".into()),
                Value::String("hello".into())
            ]
        ),
        Value::Bool(true)
    );
    assert_eq!(
        exec_fn(
            &mut vm,
            "Core.Object.EqualEqual_StrStr",
            None,
            vec![
                Value::String("hello".into()),
                Value::String("world".into())
            ]
        ),
        Value::Bool(false)
    );
}

#[test]
fn native_equalequal_namename() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    assert_eq!(
        exec_fn(
            &mut vm,
            "Core.Object.EqualEqual_NameName",
            None,
            vec![Value::Name("EMP".into()), Value::Name("EMP".into())]
        ),
        Value::Bool(true)
    );
    assert_eq!(
        exec_fn(
            &mut vm,
            "Core.Object.EqualEqual_NameName",
            None,
            vec![Value::Name("EMP".into()), Value::Name("Fire".into())]
        ),
        Value::Bool(false)
    );
}

// ---------------------------------------------------------------------------
// 3. Native 布尔运算（P2 地址已验证）
// ---------------------------------------------------------------------------

#[test]
fn native_not_prebool() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    assert_eq!(
        exec_fn(&mut vm, "Core.Object.Not_PreBool", None, vec![Value::Bool(true)]),
        Value::Bool(false)
    );
    assert_eq!(
        exec_fn(&mut vm, "Core.Object.Not_PreBool", None, vec![Value::Bool(false)]),
        Value::Bool(true)
    );
}

#[test]
fn native_andand_boolbool() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    assert_eq!(
        exec_fn(
            &mut vm,
            "Core.Object.AndAnd_BoolBool",
            None,
            vec![Value::Bool(true), Value::Bool(true)]
        ),
        Value::Bool(true)
    );
    assert_eq!(
        exec_fn(
            &mut vm,
            "Core.Object.AndAnd_BoolBool",
            None,
            vec![Value::Bool(true), Value::Bool(false)]
        ),
        Value::Bool(false)
    );
}

#[test]
fn native_oror_boolbool() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    assert_eq!(
        exec_fn(
            &mut vm,
            "Core.Object.OrOr_BoolBool",
            None,
            vec![Value::Bool(false), Value::Bool(false)]
        ),
        Value::Bool(false)
    );
    assert_eq!(
        exec_fn(
            &mut vm,
            "Core.Object.OrOr_BoolBool",
            None,
            vec![Value::Bool(false), Value::Bool(true)]
        ),
        Value::Bool(true)
    );
}

#[test]
fn native_xorxor_boolbool() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    assert_eq!(
        exec_fn(
            &mut vm,
            "Core.Object.XorXor_BoolBool",
            None,
            vec![Value::Bool(true), Value::Bool(false)]
        ),
        Value::Bool(true)
    );
    assert_eq!(
        exec_fn(
            &mut vm,
            "Core.Object.XorXor_BoolBool",
            None,
            vec![Value::Bool(true), Value::Bool(true)]
        ),
        Value::Bool(false)
    );
}

// ---------------------------------------------------------------------------
// 4. Native 字符串运算（P2 地址已验证）
// ---------------------------------------------------------------------------

#[test]
fn native_concat_strstr() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    let result = exec_fn(
        &mut vm,
        "Core.Object.Concat_StrStr",
        None,
        vec![
            Value::String("Hello, ".into()),
            Value::String("world!".into()),
        ],
    );
    assert_eq!(result, Value::String("Hello, world!".into()));
}

#[test]
fn native_len() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    let result = exec_fn(&mut vm, "Core.Object.Len", None, vec![Value::String("hello".into())]);
    assert_eq!(result, Value::Int(5));
}

#[test]
fn native_instr() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    assert_eq!(
        exec_fn(
            &mut vm,
            "Core.Object.InStr",
            None,
            vec![
                Value::String("hello world".into()),
                Value::String("world".into()),
            ]
        ),
        Value::Int(6)
    );
    assert_eq!(
        exec_fn(
            &mut vm,
            "Core.Object.InStr",
            None,
            vec![
                Value::String("hello world".into()),
                Value::String("xyz".into()),
            ]
        ),
        Value::Int(-1)
    );
}

#[test]
fn native_mid_left_right() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    assert_eq!(
        exec_fn(
            &mut vm,
            "Core.Object.Mid",
            None,
            vec![
                Value::String("abcdef".into()),
                Value::Int(2),
                Value::Int(3),
            ]
        ),
        Value::String("cde".into())
    );
    assert_eq!(
        exec_fn(
            &mut vm,
            "Core.Object.Left",
            None,
            vec![Value::String("abcdef".into()), Value::Int(3)]
        ),
        Value::String("abc".into())
    );
    assert_eq!(
        exec_fn(
            &mut vm,
            "Core.Object.Right",
            None,
            vec![Value::String("abcdef".into()), Value::Int(3)]
        ),
        Value::String("def".into())
    );
}

#[test]
fn native_at_strstr() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    let result = exec_fn(
        &mut vm,
        "Core.Object.At_StrStr",
        None,
        vec![
            Value::String("hello".into()),
            Value::String("world".into()),
        ],
    );
    assert_eq!(result, Value::String("hello world".into()));
}

// ---------------------------------------------------------------------------
// 5. Native 数学函数（P2 地址已验证）
// ---------------------------------------------------------------------------

#[test]
fn native_abs() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    assert_eq!(
        exec_fn(&mut vm, "Core.Object.Abs", None, vec![Value::Int(-5)]),
        Value::Int(5)
    );
    let result = exec_fn(&mut vm, "Core.Object.Abs", None, vec![Value::Float(-3.5)]);
    assert!(float_eq(result.as_float().unwrap(), 3.5));
}

#[test]
fn native_sin_cos_tan() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    // Sin(0) = 0
    let result = exec_fn(&mut vm, "Core.Object.Sin", None, vec![Value::Float(0.0)]);
    assert!(float_eq(result.as_float().unwrap(), 0.0));
    // Cos(0) = 1
    let result = exec_fn(&mut vm, "Core.Object.Cos", None, vec![Value::Float(0.0)]);
    assert!(float_eq(result.as_float().unwrap(), 1.0));
    // Tan(0) = 0
    let result = exec_fn(&mut vm, "Core.Object.Tan", None, vec![Value::Float(0.0)]);
    assert!(float_eq(result.as_float().unwrap(), 0.0));
}

#[test]
fn native_sqrt_square() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    let result = exec_fn(&mut vm, "Core.Object.Sqrt", None, vec![Value::Float(16.0)]);
    assert!(float_eq(result.as_float().unwrap(), 4.0));
    let result = exec_fn(&mut vm, "Core.Object.Square", None, vec![Value::Float(4.0)]);
    assert!(float_eq(result.as_float().unwrap(), 16.0));
}

#[test]
fn native_exp_loge() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    // Exp(0) = 1
    let result = exec_fn(&mut vm, "Core.Object.Exp", None, vec![Value::Float(0.0)]);
    assert!(float_eq(result.as_float().unwrap(), 1.0));
    // Loge(1) = 0
    let result = exec_fn(&mut vm, "Core.Object.Loge", None, vec![Value::Float(1.0)]);
    assert!(float_eq(result.as_float().unwrap(), 0.0));
}

#[test]
fn native_frand_range() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    // FRand 返回 [0, 1) 范围的随机浮点数
    for _ in 0..100 {
        let result = exec_fn(&mut vm, "Core.Object.FRand", None, vec![]);
        let f = result.as_float().unwrap();
        assert!(f >= 0.0 && f < 1.0, "FRand out of range: {f}");
    }
}

#[test]
fn native_rand_range() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    // Rand(100) 返回 [0, 100) 范围的随机整数
    for _ in 0..100 {
        let result = exec_fn(&mut vm, "Core.Object.Rand", None, vec![Value::Int(100)]);
        let n = result.as_int().unwrap();
        assert!(n >= 0 && n < 100, "Rand(100) out of range: {n}");
    }
}

// ---------------------------------------------------------------------------
// 6. Native 复合赋值运算（P2 地址已验证）
// ---------------------------------------------------------------------------

#[test]
fn native_addequal_intint() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    // AddEqual_IntInt(out A: int, B: int) -> A + B，VM 会将结果写回 A
    let result = exec_fn(
        &mut vm,
        "Core.Object.AddEqual_IntInt",
        None,
        vec![Value::Int(10), Value::Int(5)],
    );
    assert_eq!(result, Value::Int(15));
}

// ---------------------------------------------------------------------------
// 7. 非 native 函数（P1 伪代码揭示逻辑，通过字节码执行）
// ---------------------------------------------------------------------------

#[test]
fn hello_world_commandlet_main() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    // HelloWorldCommandlet.Main(Parms: string)
    // P1 伪代码：
    //   Log("Hello, world!");
    //   if (Parms == "") goto L0040;
    //   Log("Command line parameters=" $ Parms);
    //   L0040:
    //   if (intparm == 0) goto L006D;
    //   Log("You specified intparm=" $ intparm);
    //   L006D:
    //   if (strparm == "") goto L009A;
    //   Log("You specified strparm=" $ strparm);
    //   L009A:
    //   return;

    // 辅助：获取自基准以来新增的日志
    let log_baseline = vm.logs().len();

    // 情况 1：Parms=""，intparm=0，strparm="" -> 只输出 "Hello, world!"
    let object = vm.create_object("HelloWorldCommandlet");
    vm.set_object_property(object, "HelloWorldCommandlet.intparm", Value::Int(0));
    vm.set_object_property(
        object,
        "HelloWorldCommandlet.strparm",
        Value::String("".into()),
    );
    let result = exec_fn(&mut vm, "HelloWorldCommandlet.Main", Some(object), vec![Value::String(String::new())]);
    assert_eq!(result, Value::Int(0));
    assert_eq!(
        &vm.logs()[log_baseline..],
        ["Hello, world!"]
    );

    // 情况 2：Parms="foo"，intparm=0，strparm="" -> 输出命令行参数
    let object2 = vm.create_object("HelloWorldCommandlet");
    vm.set_object_property(object2, "HelloWorldCommandlet.intparm", Value::Int(0));
    vm.set_object_property(
        object2,
        "HelloWorldCommandlet.strparm",
        Value::String("".into()),
    );
    let log_baseline = vm.logs().len();
    let result = exec_fn(
        &mut vm,
        "HelloWorldCommandlet.Main",
        Some(object2),
        vec![Value::String("foo".into())],
    );
    assert_eq!(result, Value::Int(0));
    assert_eq!(
        &vm.logs()[log_baseline..],
        ["Hello, world!", "Command line parameters=foo"]
    );

    // 情况 3：Parms=""，intparm=42，strparm="test" -> 输出 intparm 和 strparm
    let object3 = vm.create_object("HelloWorldCommandlet");
    vm.set_object_property(object3, "HelloWorldCommandlet.intparm", Value::Int(42));
    vm.set_object_property(
        object3,
        "HelloWorldCommandlet.strparm",
        Value::String("test".into()),
    );
    let log_baseline = vm.logs().len();
    let result = exec_fn(&mut vm, "HelloWorldCommandlet.Main", Some(object3), vec![Value::String(String::new())]);
    assert_eq!(result, Value::Int(0));
    assert_eq!(
        &vm.logs()[log_baseline..],
        [
            "Hello, world!",
            "You specified intparm=42",
            "You specified strparm=test"
        ]
    );
}

#[test]
fn object_sprintf() {
    let Some(mut vm) = core_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    // Object.Sprintf(fmt: string, s0: string?, s1: string?, s2: string?, s3: string?)
    // P1 伪代码：
    //   S[0] = s0; S[1] = s1; S[2] = s2; S[3] = s3;
    //   index = 0; pos = 0; str = "";
    //   L004C: pos = InStr(fmt, "%");
    //   if (pos != -1) goto L0088;
    //   str = str $ fmt; fmt = ""; goto L0118;
    //   L0088: str = str $ Left(fmt, pos);
    //   if (!(pos < Len(fmt) && Mid(fmt, pos + 1, 1) == "%")) goto L00DF;
    //   str = str $ "%"; goto L00F9;
    //   L00DF: str = str $ S[index++];
    //   L00F9: fmt = Right(fmt, Len(fmt) - pos - 2);
    //   L0118: if (!(pos == -1 || index == 4)) goto L004C;
    //   if (fmt == "") goto L0153;
    //   str = str $ fmt;
    //   return str;

    // 测试无格式化参数的情况
    let result = exec_fn(
        &mut vm,
        "Core.Object.Sprintf",
        None,
        vec![
            Value::String("plain text".into()),
            Value::None,
            Value::None,
            Value::None,
            Value::None,
        ],
    );
    assert_eq!(result, Value::String("plain text".into()));

    // 测试单个 %s 替换（涉及 S[0] = s0 及 S[index++] 读取）
    let result = exec_fn(
        &mut vm,
        "Core.Object.Sprintf",
        None,
        vec![
            Value::String("Value: %s".into()),
            Value::String("42".into()),
            Value::None,
            Value::None,
            Value::None,
        ],
    );
    assert_eq!(result, Value::String("Value: 42".into()));

    // 测试多个 %s 替换（连续多次 S[index++] 读取）
    let result = exec_fn(
        &mut vm,
        "Core.Object.Sprintf",
        None,
        vec![
            Value::String("%s + %s = %s".into()),
            Value::String("1".into()),
            Value::String("2".into()),
            Value::String("3".into()),
            Value::None,
        ],
    );
    assert_eq!(result, Value::String("1 + 2 = 3".into()));
}

// ---------------------------------------------------------------------------
// 8. 集成测试：Core + Engine 联合执行
// ---------------------------------------------------------------------------

#[test]
fn engine_actor_postbeginplay_chain() {
    let Some(mut vm) = core_engine_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    // Engine.Actor.PostBeginPlay 是 non-native，应该可执行
    let object = vm.create_object("Engine.Actor");
    let result = exec_fn(&mut vm, "Engine.Actor.PostBeginPlay", Some(object), vec![]);
    assert_eq!(result, Value::None);
}

#[test]
fn engine_actor_settimer_native() {
    let Some(mut vm) = core_engine_vm() else {
        eprintln!("skipping: Deus Ex GOTY assets not found");
        return;
    };
    // Engine.Actor.SetTimer(NewTimerRate: float, bLoop: bool) - native，索引 280
    let object = vm.create_object("Engine.Actor");
    let result = exec_fn(
        &mut vm,
        "Engine.Actor.SetTimer",
        Some(object),
        vec![Value::Float(1.0), Value::Bool(true)],
    );
    assert_eq!(result, Value::None);
}
