//! deusex-ai - AI 行为树系统.
//!
//! 提供基于行为树（Behavior Tree）的 AI 决策框架，适用于 Deus Ex 这类
//! 3D 游戏中的 NPC 行为控制。
//!
//! 核心组件:
//! - `Status`: 节点执行状态（Running / Success / Failure）
//! - `BehaviorTree`: 行为树，持有根节点和黑板
//! - `Blackboard`: 键值对黑板，用于节点间共享状态
//! - `Sequence`: 顺序组合节点，所有子节点都成功才返回 Success
//! - `Selector`: 选择器组合节点，有一个子节点成功就返回 Success
//! - `Inverter`: 反转装饰器，翻转子节点的 Success/Failure
//! - `Repeater`: 重复装饰器，重复执行子节点指定次数
//! - `ActionNode`: 动作叶子节点（用户自定义行为）
//! - `ConditionNode`: 条件叶子节点（用户自定义条件检查）
//! - `BehaviorTreeExecutor`: 行为树执行器，每帧调用 tick

use std::collections::HashMap;

use tracing::{debug, info};

/// 行为节点执行状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// 节点正在运行，下一帧继续。
    Running,
    /// 节点执行成功。
    Success,
    /// 节点执行失败。
    Failure,
}

/// 黑板中的值类型。
#[derive(Debug, Clone, PartialEq)]
pub enum BlackboardValue {
    Bool(bool),
    Int(i32),
    Float(f32),
    String(String),
    Vec3([f32; 3]),
}

impl BlackboardValue {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            BlackboardValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i32> {
        match self {
            BlackboardValue::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f32> {
        match self {
            BlackboardValue::Float(f) => Some(*f),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            BlackboardValue::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_vec3(&self) -> Option<[f32; 3]> {
        match self {
            BlackboardValue::Vec3(v) => Some(*v),
            _ => None,
        }
    }
}

/// 黑板：键值对存储，供行为树节点间共享状态。
#[derive(Debug, Default, Clone)]
pub struct Blackboard {
    values: HashMap<String, BlackboardValue>,
}

impl Blackboard {
    /// 创建空黑板。
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置布尔值。
    pub fn set_bool(&mut self, key: &str, value: bool) {
        self.values.insert(key.to_string(), BlackboardValue::Bool(value));
    }

    /// 设置整数值。
    pub fn set_int(&mut self, key: &str, value: i32) {
        self.values.insert(key.to_string(), BlackboardValue::Int(value));
    }

    /// 设置浮点值。
    pub fn set_float(&mut self, key: &str, value: f32) {
        self.values.insert(key.to_string(), BlackboardValue::Float(value));
    }

    /// 设置字符串值。
    pub fn set_string(&mut self, key: &str, value: &str) {
        self.values
            .insert(key.to_string(), BlackboardValue::String(value.to_string()));
    }

    /// 设置 3D 向量值。
    pub fn set_vec3(&mut self, key: &str, value: [f32; 3]) {
        self.values
            .insert(key.to_string(), BlackboardValue::Vec3(value));
    }

    /// 获取布尔值。
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.values.get(key).and_then(|v| v.as_bool())
    }

    /// 获取整数值。
    pub fn get_int(&self, key: &str) -> Option<i32> {
        self.values.get(key).and_then(|v| v.as_int())
    }

    /// 获取浮点值。
    pub fn get_float(&self, key: &str) -> Option<f32> {
        self.values.get(key).and_then(|v| v.as_float())
    }

    /// 获取字符串值。
    pub fn get_string(&self, key: &str) -> Option<String> {
        self.values.get(key).and_then(|v| v.as_string().map(|s| s.to_string()))
    }

    /// 获取 3D 向量值。
    pub fn get_vec3(&self, key: &str) -> Option<[f32; 3]> {
        self.values.get(key).and_then(|v| v.as_vec3())
    }

    /// 检查键是否存在。
    pub fn contains(&self, key: &str) -> bool {
        self.values.contains_key(key)
    }

    /// 移除键。
    pub fn remove(&mut self, key: &str) {
        self.values.remove(key);
    }
}

/// 行为节点 trait。所有行为树节点都必须实现此 trait。
pub trait Node: std::fmt::Debug + Send + Sync {
    /// 执行一帧行为逻辑，返回节点状态。
    fn tick(&mut self, blackboard: &mut Blackboard) -> Status;

    /// 返回节点名称（用于调试）。
    fn name(&self) -> &str;
}

/// 动作叶子节点：执行具体的 AI 行为（如移动、攻击、播放动画）。
///
/// 用户通过闭包自定义行为逻辑。
pub struct ActionNode<F>
where
    F: FnMut(&mut Blackboard) -> Status + Send + Sync,
{
    name: String,
    func: F,
}

impl<F> ActionNode<F>
where
    F: FnMut(&mut Blackboard) -> Status + Send + Sync,
{
    pub fn new(name: &str, func: F) -> Self {
        Self {
            name: name.to_string(),
            func,
        }
    }
}

impl<F> std::fmt::Debug for ActionNode<F>
where
    F: FnMut(&mut Blackboard) -> Status + Send + Sync,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActionNode").field("name", &self.name).finish()
    }
}

impl<F> Node for ActionNode<F>
where
    F: FnMut(&mut Blackboard) -> Status + Send + Sync,
{
    fn tick(&mut self, blackboard: &mut Blackboard) -> Status {
        (self.func)(blackboard)
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// 条件叶子节点：检查黑板状态或游戏条件，返回 Success/Failure。
pub struct ConditionNode<F>
where
    F: Fn(&Blackboard) -> bool + Send + Sync,
{
    name: String,
    func: F,
}

impl<F> ConditionNode<F>
where
    F: Fn(&Blackboard) -> bool + Send + Sync,
{
    pub fn new(name: &str, func: F) -> Self {
        Self {
            name: name.to_string(),
            func,
        }
    }
}

impl<F> std::fmt::Debug for ConditionNode<F>
where
    F: Fn(&Blackboard) -> bool + Send + Sync,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConditionNode").field("name", &self.name).finish()
    }
}

impl<F> Node for ConditionNode<F>
where
    F: Fn(&Blackboard) -> bool + Send + Sync,
{
    fn tick(&mut self, blackboard: &mut Blackboard) -> Status {
        if (self.func)(blackboard) {
            Status::Success
        } else {
            Status::Failure
        }
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// 顺序组合节点：按顺序执行子节点，所有子节点都返回 Success 才返回 Success。
/// 如果某个子节点返回 Failure 或 Running，立即返回该状态。
#[derive(Debug)]
pub struct Sequence {
    name: String,
    children: Vec<Box<dyn Node>>,
    /// 上一帧最后运行的子节点索引（用于优化：从上一帧的 Running 节点继续）。
    last_running: usize,
}

impl Sequence {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            children: Vec::new(),
            last_running: 0,
        }
    }

    pub fn add_child(&mut self, child: Box<dyn Node>) {
        self.children.push(child);
    }
}

impl Node for Sequence {
    fn tick(&mut self, blackboard: &mut Blackboard) -> Status {
        if self.children.is_empty() {
            return Status::Success;
        }

        // 从上一帧的运行节点继续（优化）
        let mut i = self.last_running.min(self.children.len() - 1);

        while i < self.children.len() {
            let status = self.children[i].tick(blackboard);
            match status {
                Status::Running => {
                    self.last_running = i;
                    return Status::Running;
                }
                Status::Failure => {
                    self.last_running = 0;
                    return Status::Failure;
                }
                Status::Success => {
                    i += 1;
                }
            }
        }

        // 所有子节点都成功
        self.last_running = 0;
        Status::Success
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// 选择器组合节点：按顺序执行子节点，有一个子节点返回 Success 就返回 Success。
/// 如果所有子节点都返回 Failure，才返回 Failure。
#[derive(Debug)]
pub struct Selector {
    name: String,
    children: Vec<Box<dyn Node>>,
    last_running: usize,
}

impl Selector {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            children: Vec::new(),
            last_running: 0,
        }
    }

    pub fn add_child(&mut self, child: Box<dyn Node>) {
        self.children.push(child);
    }
}

impl Node for Selector {
    fn tick(&mut self, blackboard: &mut Blackboard) -> Status {
        if self.children.is_empty() {
            return Status::Failure;
        }

        let mut i = self.last_running.min(self.children.len() - 1);

        while i < self.children.len() {
            let status = self.children[i].tick(blackboard);
            match status {
                Status::Running => {
                    self.last_running = i;
                    return Status::Running;
                }
                Status::Success => {
                    self.last_running = 0;
                    return Status::Success;
                }
                Status::Failure => {
                    i += 1;
                }
            }
        }

        // 所有子节点都失败
        self.last_running = 0;
        Status::Failure
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// 反转装饰器：翻转子节点的 Success/Failure，Running 保持不变。
#[derive(Debug)]
pub struct Inverter {
    name: String,
    child: Box<dyn Node>,
}

impl Inverter {
    pub fn new(name: &str, child: Box<dyn Node>) -> Self {
        Self {
            name: name.to_string(),
            child,
        }
    }
}

impl Node for Inverter {
    fn tick(&mut self, blackboard: &mut Blackboard) -> Status {
        match self.child.tick(blackboard) {
            Status::Success => Status::Failure,
            Status::Failure => Status::Success,
            Status::Running => Status::Running,
        }
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// 重复装饰器：重复执行子节点指定次数，或无限重复。
#[derive(Debug)]
pub struct Repeater {
    name: String,
    child: Box<dyn Node>,
    max_repeats: i32, // -1 表示无限
    count: i32,
}

impl Repeater {
    pub fn new(name: &str, child: Box<dyn Node>, max_repeats: i32) -> Self {
        Self {
            name: name.to_string(),
            child,
            max_repeats,
            count: 0,
        }
    }
}

impl Node for Repeater {
    fn tick(&mut self, blackboard: &mut Blackboard) -> Status {
        if self.max_repeats == -1 {
            // 无限重复
            match self.child.tick(blackboard) {
                Status::Running => Status::Running,
                Status::Success | Status::Failure => {
                    // 子节点结束后立即重置并重新开始
                    Status::Running
                }
            }
        } else {
            if self.count >= self.max_repeats {
                return Status::Success;
            }

            match self.child.tick(blackboard) {
                Status::Running => Status::Running,
                Status::Success => {
                    self.count += 1;
                    if self.count >= self.max_repeats {
                        Status::Success
                    } else {
                        Status::Running
                    }
                }
                Status::Failure => Status::Failure,
            }
        }
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// 行为树：持有根节点和黑板，是 AI 行为的顶层容器。
pub struct BehaviorTree {
    name: String,
    root: Option<Box<dyn Node>>,
    blackboard: Blackboard,
}

impl BehaviorTree {
    /// 创建空行为树。
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            root: None,
            blackboard: Blackboard::new(),
        }
    }

    /// 设置根节点。
    pub fn set_root(&mut self, root: Box<dyn Node>) {
        self.root = Some(root);
    }

    /// 返回可变的黑板引用。
    pub fn blackboard_mut(&mut self) -> &mut Blackboard {
        &mut self.blackboard
    }

    /// 返回不可变的黑板引用。
    pub fn blackboard(&self) -> &Blackboard {
        &self.blackboard
    }

    /// 返回行为树名称。
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl std::fmt::Debug for BehaviorTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BehaviorTree")
            .field("name", &self.name)
            .field("has_root", &self.root.is_some())
            .finish()
    }
}

/// 行为树执行器：每帧调用 tick 来驱动行为树运行。
pub struct BehaviorTreeExecutor {
    tree: BehaviorTree,
    /// 是否启用调试日志。
    debug: bool,
}

impl BehaviorTreeExecutor {
    /// 创建执行器。
    pub fn new(tree: BehaviorTree) -> Self {
        info!("ai: created behavior tree executor '{}'", tree.name());
        Self {
            tree,
            debug: false,
        }
    }

    /// 启用/禁用调试日志。
    pub fn set_debug(&mut self, debug: bool) {
        self.debug = debug;
    }

    /// 返回可变的行为树引用。
    pub fn tree_mut(&mut self) -> &mut BehaviorTree {
        &mut self.tree
    }

    /// 返回不可变的行为树引用。
    pub fn tree(&self) -> &BehaviorTree {
        &self.tree
    }

    /// 执行一帧行为树逻辑。
    ///
    /// 通常在游戏主循环的固定 tick 中调用。
    /// 返回根节点的状态。
    pub fn tick(&mut self) -> Status {
        let root = match &mut self.tree.root {
            Some(r) => r,
            None => return Status::Success,
        };

        let status = root.tick(&mut self.tree.blackboard);

        if self.debug {
            debug!(
                "ai: tree '{}' tick -> {:?}",
                self.tree.name(),
                status
            );
        }

        status
    }

    /// 返回当前黑板引用。
    pub fn blackboard(&self) -> &Blackboard {
        self.tree.blackboard()
    }

    /// 返回可变的黑板引用。
    pub fn blackboard_mut(&mut self) -> &mut Blackboard {
        self.tree.blackboard_mut()
    }
}

impl std::fmt::Debug for BehaviorTreeExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BehaviorTreeExecutor")
            .field("tree", &self.tree)
            .finish()
    }
}

impl Default for BehaviorTreeExecutor {
    fn default() -> Self {
        Self::new(BehaviorTree::new("default"))
    }
}

/// 预定义的常用 AI 行为节点构建器。
///
/// 所有构建器返回 `Box<dyn Node>`，可直接加入 `BehaviorTree` 或组合节点。
pub mod builtin {
    use super::*;

    /// 检查黑板中是否有敌人目标。
    pub fn has_target() -> Box<dyn Node> {
        Box::new(ConditionNode::new("HasTarget", |bb| bb.contains("target")))
    }

    /// 检查与目标的距离是否小于指定阈值。
    pub fn is_close_to_target(threshold: f32) -> Box<dyn Node> {
        Box::new(ConditionNode::new("IsCloseToTarget", move |bb| {
            if let Some(pos) = bb.get_vec3("self_pos") {
                if let Some(target) = bb.get_vec3("target_pos") {
                    let dx = pos[0] - target[0];
                    let dy = pos[1] - target[1];
                    let dz = pos[2] - target[2];
                    let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                    return dist < threshold;
                }
            }
            false
        }))
    }

    /// 移动向目标的动作。
    pub fn move_to_target(speed: f32) -> Box<dyn Node> {
        Box::new(ActionNode::new("MoveToTarget", move |bb| {
            if let Some(pos) = bb.get_vec3("self_pos") {
                if let Some(target) = bb.get_vec3("target_pos") {
                    let dx = target[0] - pos[0];
                    let dy = target[1] - pos[1];
                    let dz = target[2] - pos[2];
                    let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                    if dist < 0.1 {
                        return Status::Success;
                    }
                    // 简单线性移动（实际应用中应由物理系统执行）
                    let step = speed * 0.016; // 假设 60fps
                    let new_pos = [
                        pos[0] + (dx / dist) * step,
                        pos[1] + (dy / dist) * step,
                        pos[2] + (dz / dist) * step,
                    ];
                    bb.set_vec3("self_pos", new_pos);
                    return Status::Running;
                }
            }
            Status::Failure
        }))
    }

    /// 攻击目标的动作。
    pub fn attack_target() -> Box<dyn Node> {
        Box::new(ActionNode::new("AttackTarget", |bb| {
            if bb.contains("target") {
                debug!("ai: attacking target");
                Status::Success
            } else {
                Status::Failure
            }
        }))
    }

    /// 巡逻动作：在航点之间移动。
    pub fn patrol(waypoints: Vec<[f32; 3]>) -> Box<dyn Node> {
        Box::new(ActionNode::new("Patrol", move |bb| {
            let current_idx = bb.get_int("patrol_idx").unwrap_or(0) as usize;
            if current_idx >= waypoints.len() {
                bb.set_int("patrol_idx", 0);
                return Status::Success;
            }

            let target = waypoints[current_idx];
            if let Some(pos) = bb.get_vec3("self_pos") {
                let dx = target[0] - pos[0];
                let dy = target[1] - pos[1];
                let dz = target[2] - pos[2];
                let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                if dist < 0.5 {
                    // 到达航点，切换到下一个
                    let next = (current_idx + 1) % waypoints.len();
                    bb.set_int("patrol_idx", next as i32);
                    return Status::Running;
                }
                let step = 2.0 * 0.016;
                let new_pos = [
                    pos[0] + (dx / dist) * step,
                    pos[1] + (dy / dist) * step,
                    pos[2] + (dz / dist) * step,
                ];
                bb.set_vec3("self_pos", new_pos);
                return Status::Running;
            }
            Status::Failure
        }))
    }

    /// 等待指定秒数。
    pub fn wait(seconds: f32) -> Box<dyn Node> {
        Box::new(ActionNode::new("Wait", move |bb| {
            let key = "wait_elapsed";
            let elapsed = bb.get_float(key).unwrap_or(0.0);
            let new_elapsed = elapsed + 0.016; // 假设 60fps
            bb.set_float(key, new_elapsed);
            if new_elapsed >= seconds {
                bb.remove(key);
                Status::Success
            } else {
                Status::Running
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_all_success() {
        let mut seq = Sequence::new("TestSequence");
        seq.add_child(Box::new(ConditionNode::new("AlwaysTrue", |_| true)));
        seq.add_child(Box::new(ConditionNode::new("AlwaysTrue2", |_| true)));

        let mut bb = Blackboard::new();
        assert_eq!(seq.tick(&mut bb), Status::Success);
    }

    #[test]
    fn sequence_one_failure() {
        let mut seq = Sequence::new("TestSequence");
        seq.add_child(Box::new(ConditionNode::new("AlwaysTrue", |_| true)));
        seq.add_child(Box::new(ConditionNode::new("AlwaysFalse", |_| false)));

        let mut bb = Blackboard::new();
        assert_eq!(seq.tick(&mut bb), Status::Failure);
    }

    #[test]
    fn selector_one_success() {
        let mut sel = Selector::new("TestSelector");
        sel.add_child(Box::new(ConditionNode::new("AlwaysFalse", |_| false)));
        sel.add_child(Box::new(ConditionNode::new("AlwaysTrue", |_| true)));

        let mut bb = Blackboard::new();
        assert_eq!(sel.tick(&mut bb), Status::Success);
    }

    #[test]
    fn selector_all_failure() {
        let mut sel = Selector::new("TestSelector");
        sel.add_child(Box::new(ConditionNode::new("AlwaysFalse", |_| false)));
        sel.add_child(Box::new(ConditionNode::new("AlwaysFalse2", |_| false)));

        let mut bb = Blackboard::new();
        assert_eq!(sel.tick(&mut bb), Status::Failure);
    }

    #[test]
    fn inverter_flips() {
        let mut inv = Inverter::new(
            "TestInverter",
            Box::new(ConditionNode::new("AlwaysTrue", |_| true)),
        );

        let mut bb = Blackboard::new();
        assert_eq!(inv.tick(&mut bb), Status::Failure);
    }

    #[test]
    fn blackboard_get_set() {
        let mut bb = Blackboard::new();
        bb.set_bool("has_target", true);
        bb.set_int("health", 100);
        bb.set_float("speed", 5.5);
        bb.set_string("name", "Guard");
        bb.set_vec3("pos", [1.0, 2.0, 3.0]);

        assert_eq!(bb.get_bool("has_target"), Some(true));
        assert_eq!(bb.get_int("health"), Some(100));
        assert_eq!(bb.get_float("speed"), Some(5.5));
        assert_eq!(bb.get_string("name"), Some("Guard".to_string()));
        assert_eq!(bb.get_vec3("pos"), Some([1.0, 2.0, 3.0]));
        assert!(!bb.get_bool("nonexistent").unwrap_or(false));
    }

    #[test]
    fn behavior_tree_tick() {
        let mut tree = BehaviorTree::new("GuardAI");
        let mut seq = Sequence::new("GuardSequence");
        seq.add_child(builtin::has_target());
        seq.add_child(builtin::is_close_to_target(2.0));
        seq.add_child(builtin::attack_target());
        tree.set_root(Box::new(seq));

        let mut executor = BehaviorTreeExecutor::new(tree);
        executor.tree_mut().blackboard_mut().set_bool("target", true);
        executor.tree_mut().blackboard_mut().set_vec3("self_pos", [0.0, 0.0, 0.0]);
        executor.tree_mut().blackboard_mut().set_vec3("target_pos", [1.0, 0.0, 0.0]);

        let status = executor.tick();
        assert_eq!(status, Status::Success);
    }

    #[test]
    fn builtin_wait_node() {
        let mut wait = builtin::wait(0.05); // 约 4 帧 (60fps)
        let mut bb = Blackboard::new();

        // 前三帧：Running
        assert_eq!(wait.tick(&mut bb), Status::Running);
        assert_eq!(wait.tick(&mut bb), Status::Running);
        assert_eq!(wait.tick(&mut bb), Status::Running);
        // 第四帧：Success
        assert_eq!(wait.tick(&mut bb), Status::Success);
    }
}
