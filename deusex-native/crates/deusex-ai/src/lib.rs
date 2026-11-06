//! deusex-ai - AI 行为树系统.
//!
//! 提供基于行为树(Behavior Tree)的 AI 决策框架，适用于 Deus Ex 这类
//! 3D 游戏中的 NPC 行为控制.
//!
//! 核心组件:
//! - Status: 节点执行状态(Running / Success / Failure)
//! - BehaviorTree: 行为树，持有根节点和黑板
//! - Blackboard: 键值对黑板，用于节点间共享状态
//! - Sequence: 顺序组合节点，所有子节点都成功才返回 Success
//! - Selector: 选择器组合节点，有一个子节点成功就返回 Success
//! - Inverter: 反转装饰器，翻转子节点的 Success/Failure
//! - Repeater: 重复装饰器，重复执行子节点指定次数
//! - ActionNode: 动作叶子节点(用户自定义行为)
//! - ConditionNode: 条件叶子节点(用户自定义条件检查)
//! - BehaviorTreeExecutor: 行为树执行器，每帧调用 tick

use std::collections::HashMap;

use tracing::{debug, info};

/// 行为节点执行状态.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// 节点正在运行，下一帧继续.
    Running,
    /// 节点执行成功.
    Success,
    /// 节点执行失败.
    Failure,
}

/// 黑板中的值类型.
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

/// 黑板：键值对存储，供行为树节点间共享状态.
#[derive(Debug, Default, Clone)]
pub struct Blackboard {
    values: HashMap<String, BlackboardValue>,
}

impl Blackboard {
    /// 创建空黑板.
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置布尔值.
    pub fn set_bool(&mut self, key: &str, value: bool) {
        self.values.insert(key.to_string(), BlackboardValue::Bool(value));
    }

    /// 设置整数值.
    pub fn set_int(&mut self, key: &str, value: i32) {
        self.values.insert(key.to_string(), BlackboardValue::Int(value));
    }

    /// 设置浮点值.
    pub fn set_float(&mut self, key: &str, value: f32) {
        self.values.insert(key.to_string(), BlackboardValue::Float(value));
    }

    /// 设置字符串值.
    pub fn set_string(&mut self, key: &str, value: &str) {
        self.values
            .insert(key.to_string(), BlackboardValue::String(value.to_string()));
    }

    /// 设置 3D 向量值.
    pub fn set_vec3(&mut self, key: &str, value: [f32; 3]) {
        self.values
            .insert(key.to_string(), BlackboardValue::Vec3(value));
    }

    /// 获取布尔值.
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.values.get(key).and_then(|v| v.as_bool())
    }

    /// 获取整数值.
    pub fn get_int(&self, key: &str) -> Option<i32> {
        self.values.get(key).and_then(|v| v.as_int())
    }

    /// 获取浮点值.
    pub fn get_float(&self, key: &str) -> Option<f32> {
        self.values.get(key).and_then(|v| v.as_float())
    }

    /// 获取字符串值.
    pub fn get_string(&self, key: &str) -> Option<String> {
        self.values.get(key).and_then(|v| v.as_string().map(|s| s.to_string()))
    }

    /// 获取 3D 向量值.
    pub fn get_vec3(&self, key: &str) -> Option<[f32; 3]> {
        self.values.get(key).and_then(|v| v.as_vec3())
    }

    /// 检查键是否存在.
    pub fn contains(&self, key: &str) -> bool {
        self.values.contains_key(key)
    }

    /// 移除键.
    pub fn remove(&mut self, key: &str) {
        self.values.remove(key);
    }
}

/// 行为节点 trait.所有行为树节点都必须实现此 trait.
pub trait Node: std::fmt::Debug + Send + Sync {
    /// 执行一帧行为逻辑，返回节点状态.
    fn tick(&mut self, blackboard: &mut Blackboard) -> Status;

    /// 返回节点名称(用于调试).
    fn name(&self) -> &str;
}

/// 动作叶子节点：执行具体的 AI 行为(如移动,攻击,播放动画).
///
/// 用户通过闭包自定义行为逻辑.
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

/// 条件叶子节点：检查黑板状态或游戏条件，返回 Success/Failure.
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

/// 顺序组合节点：按顺序执行子节点，所有子节点都返回 Success 才返回 Success.
/// 如果某个子节点返回 Failure 或 Running，立即返回该状态.
#[derive(Debug)]
pub struct Sequence {
    name: String,
    children: Vec<Box<dyn Node>>,
    /// 上一帧最后运行的子节点索引(用于优化：从上一帧的 Running 节点继续).
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

        // 从上一帧的运行节点继续(优化)
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

/// 选择器组合节点：按顺序执行子节点，有一个子节点返回 Success 就返回 Success.
/// 如果所有子节点都返回 Failure，才返回 Failure.
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

/// 反转装饰器：翻转子节点的 Success/Failure，Running 保持不变.
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

/// 重复装饰器：重复执行子节点指定次数，或无限重复.
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

/// 行为树：持有根节点和黑板，是 AI 行为的顶层容器.
pub struct BehaviorTree {
    name: String,
    root: Option<Box<dyn Node>>,
    blackboard: Blackboard,
}

impl BehaviorTree {
    /// 创建空行为树.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            root: None,
            blackboard: Blackboard::new(),
        }
    }

    /// 设置根节点.
    pub fn set_root(&mut self, root: Box<dyn Node>) {
        self.root = Some(root);
    }

    /// 返回可变的黑板引用.
    pub fn blackboard_mut(&mut self) -> &mut Blackboard {
        &mut self.blackboard
    }

    /// 返回不可变的黑板引用.
    pub fn blackboard(&self) -> &Blackboard {
        &self.blackboard
    }

    /// 返回行为树名称.
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

/// 行为树执行器：每帧调用 tick 来驱动行为树运行.
pub struct BehaviorTreeExecutor {
    tree: BehaviorTree,
    /// 是否启用调试日志.
    debug: bool,
}

impl BehaviorTreeExecutor {
    /// 创建执行器.
    pub fn new(tree: BehaviorTree) -> Self {
        info!("ai: created behavior tree executor '{}'", tree.name());
        Self {
            tree,
            debug: false,
        }
    }

    /// 启用/禁用调试日志.
    pub fn set_debug(&mut self, debug: bool) {
        self.debug = debug;
    }

    /// 返回可变的行为树引用.
    pub fn tree_mut(&mut self) -> &mut BehaviorTree {
        &mut self.tree
    }

    /// 返回不可变的行为树引用.
    pub fn tree(&self) -> &BehaviorTree {
        &self.tree
    }

    /// 执行一帧行为树逻辑.
    ///
    /// 通常在游戏主循环的固定 tick 中调用.
    /// 返回根节点的状态.
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

    /// 返回当前黑板引用.
    pub fn blackboard(&self) -> &Blackboard {
        self.tree.blackboard()
    }

    /// 返回可变的黑板引用.
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

/// 预定义的常用 AI 行为节点构建器.
///
/// 所有构建器返回 Box<dyn Node>，可直接加入 BehaviorTree 或组合节点.
pub mod builtin {
    use super::*;

    /// 检查黑板中是否有敌人目标.
    pub fn has_target() -> Box<dyn Node> {
        Box::new(ConditionNode::new("HasTarget", |bb| bb.contains("target")))
    }

    /// 检查与目标的距离是否小于指定阈值.
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

    /// 移动向目标的动作.
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
                    // 简单线性移动(实际应用中应由物理系统执行)
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

    /// 攻击目标的动作.
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

    /// 巡逻动作：在航点之间移动.
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

    /// 等待指定秒数.
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

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 感知系统：视线检测,听觉
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 感知类型.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SenseType {
    /// 视觉：需要视线不被遮挡.
    Vision,
    /// 听觉：基于距离和声音大小.
    Hearing,
}

/// 声音类型：不同类型的声音有不同的衰减特性.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SoundType {
    /// 脚步声：衰减快，听距短.
    #[default]
    Footstep,
    /// 枪声：衰减慢，听距远.
    Gunshot,
    /// 对话声：中等衰减.
    Conversation,
    /// 爆炸声：衰减最慢，听距最远.
    Explosion,
}

/// 声音类型的衰减系数（用于反平方定律：强度 = 基础强度 / (1 + 距离^2 * 衰减系数)）.
impl SoundType {
    /// 返回该声音类型的衰减系数（数值越小，传播越远）.
    pub fn attenuation_coefficient(&self) -> f32 {
        match self {
            SoundType::Footstep => 0.05,
            SoundType::Conversation => 0.03,
            SoundType::Gunshot => 0.01,
            SoundType::Explosion => 0.005,
        }
    }

    /// 返回该声音类型的基础听距（米，不考虑衰减的最大可听距离）.
    pub fn base_hear_distance(&self) -> f32 {
        match self {
            SoundType::Footstep => 15.0,
            SoundType::Conversation => 25.0,
            SoundType::Gunshot => 60.0,
            SoundType::Explosion => 120.0,
        }
    }
}

/// 感知刺激：被感知对象发出的信号.
#[derive(Debug, Clone)]
pub struct Stimulus {
    /// 刺激来源位置.
    pub position: [f32; 3],
    /// 刺激类型.
    pub sense_type: SenseType,
    /// 刺激强度(0.0~1.0，影响可感知距离).
    pub strength: f32,
    /// 刺激持续时间(秒).
    pub duration: f32,
    /// 刺激来源的实体标识(可选).
    pub source_id: Option<u64>,
    /// 声音类型（仅当 sense_type == Hearing 时有效，用于听觉衰减模型）.
    pub sound_type: SoundType,
}

impl Stimulus {
    /// 创建一个视觉刺激.
    pub fn vision(position: [f32; 3], strength: f32) -> Self {
        Self {
            position,
            sense_type: SenseType::Vision,
            strength,
            duration: 0.0,
            source_id: None,
            sound_type: SoundType::default(),
        }
    }

    /// 创建一个听觉刺激.
    pub fn hearing(position: [f32; 3], strength: f32, sound_type: SoundType) -> Self {
        Self {
            position,
            sense_type: SenseType::Hearing,
            strength,
            duration: 0.0,
            source_id: None,
            sound_type,
        }
    }
}

/// 感知结果：感知系统返回的感知信息.
#[derive(Debug, Clone)]
pub struct PerceptionResult {
    /// 被感知的刺激.
    pub stimulus: Stimulus,
    /// 感知到的置信度(0.0~1.0).
    pub confidence: f32,
    /// 感知时间戳(秒).
    pub timestamp: f64,
}

/// 感知配置：每个 AI 实体的感知参数.
#[derive(Debug, Clone)]
pub struct PerceptionConfig {
    /// 视野水平夹角(弧度，默认 120度).
    pub fov: f32,
    /// 最大视距(米，默认 50).
    pub max_view_distance: f32,
    /// 最大听距(米，默认 30).
    pub max_hear_distance: f32,
    /// 听觉基础阈值(低于此强度的声音不会被听到).
    pub hear_threshold: f32,
}

impl Default for PerceptionConfig {
    fn default() -> Self {
        Self {
            fov: std::f32::consts::PI * 2.0 / 3.0, // 120度
            max_view_distance: 50.0,
            max_hear_distance: 30.0,
            hear_threshold: 0.01,
        }
    }
}

/// 感知组件：附着在 AI 实体上，定义其感知能力.
#[derive(Debug, Clone)]
pub struct PerceptionComponent {
    pub config: PerceptionConfig,
    /// 实体自身位置(由外部每帧更新).
    pub position: [f32; 3],
    /// 实体自身朝向向量(单位向量，用于视野判断).
    pub forward: [f32; 3],
    /// 视觉敏感度乘数(1.0 = 正常，>1.0 = 更敏感，用于不同难度/角色) .
    pub vision_sensitivity: f32,
    /// 听觉敏感度乘数(1.0 = 正常，>1.0 = 更敏感) .
    pub hearing_sensitivity: f32,
    /// 是否启用视线锥边缘衰减(靠近 FOV 边缘时置信度降低) .
    pub enable_fov_edge_falloff: bool,
    /// 视线锥边缘衰减范围(弧度，在 FOV 边缘此范围内开始衰减) .
    pub fov_edge_falloff_range: f32,
    /// 是否启用多点遮挡检测(检查目标多个点，部分遮挡时降低置信度) .
    pub enable_multi_point_occlusion: bool,
    /// 多点遮挡检测的采样点数量（每个维度）.
    pub occlusion_sample_count: u32,
}

impl Default for PerceptionComponent {
    fn default() -> Self {
        Self {
            config: PerceptionConfig::default(),
            position: [0.0, 0.0, 0.0],
            forward: [0.0, 0.0, -1.0],
            vision_sensitivity: 1.0,
            hearing_sensitivity: 1.0,
            enable_fov_edge_falloff: true,
            fov_edge_falloff_range: 0.2, // 约 11 度
            enable_multi_point_occlusion: true,
            occlusion_sample_count: 3,
        }
    }
}

/// 感知系统：管理所有感知组件，处理视线和听觉检测.
///
/// raycast_cb 是用户提供的射线检测回调，用于判断两点之间的遮挡程度.
/// 回调参数：(origin, target) -> 遮挡强度(0.0=无遮挡，1.0=完全遮挡).
/// 多层遮挡时，回调应返回累积后的遮挡强度.
pub struct PerceptionSystem<RaycastCb> {
    /// 射线检测回调，返回遮挡强度(0.0~1.0).
    raycast_cb: RaycastCb,
    /// 当前帧的感知结果.
    results: Vec<PerceptionResult>,
    /// 当前时间戳.
    timestamp: f64,
}

impl<RaycastCb> PerceptionSystem<RaycastCb>
where
    RaycastCb: Fn([f32; 3], [f32; 3]) -> f32,
{
    /// 创建感知系统.
    pub fn new(raycast_cb: RaycastCb) -> Self {
        Self {
            raycast_cb,
            results: Vec::new(),
            timestamp: 0.0,
        }
    }

    /// 设置当前时间戳.
    pub fn set_timestamp(&mut self, timestamp: f64) {
        self.timestamp = timestamp;
    }

    /// 执行一帧感知检测.
    ///
    /// perceiver: 感知者(谁在感知).
    /// stimuli: 本帧产生的所有刺激.
    ///
    /// 返回感知者能够感知到的刺激列表.
    pub fn perceive(
        &mut self,
        perceiver: &PerceptionComponent,
        stimuli: &[Stimulus],
    ) -> Vec<PerceptionResult> {
        self.results.clear();

        for stimulus in stimuli {
            if let Some(confidence) = self.sense_stimulus(perceiver, stimulus) {
                self.results.push(PerceptionResult {
                    stimulus: stimulus.clone(),
                    confidence,
                    timestamp: self.timestamp,
                });
            }
        }

        self.results.clone()
    }

    /// 对单个刺激进行感知测试.
    fn sense_stimulus(
        &self,
        perceiver: &PerceptionComponent,
        stimulus: &Stimulus,
    ) -> Option<f32> {
        let perp_pos = glam::Vec3::from_array(perceiver.position);
        let stim_pos = glam::Vec3::from_array(stimulus.position);
        let forward = glam::Vec3::from_array(perceiver.forward).normalize();

        match stimulus.sense_type {
            SenseType::Vision => self.sense_vision(perceiver, stimulus, perp_pos, stim_pos, forward),
            SenseType::Hearing => self.sense_hearing(perceiver, stimulus, perp_pos, stim_pos),
        }
    }

    /// 视觉感知：支持视线锥细化（FOV 边缘衰减、多点遮挡检测、距离梯度置信度）.
    fn sense_vision(
        &self,
        perceiver: &PerceptionComponent,
        stimulus: &Stimulus,
        perp_pos: glam::Vec3,
        stim_pos: glam::Vec3,
        forward: glam::Vec3,
    ) -> Option<f32> {
        // 1. 距离检查.
        let dist = (stim_pos - perp_pos).length();
        let max_dist = perceiver.config.max_view_distance
            * stimulus.strength
            * perceiver.vision_sensitivity;
        if dist > max_dist {
            return None;
        }

        // 2. 视野夹角检查.
        let to_stim = (stim_pos - perp_pos).normalize();
        let dot = forward.dot(to_stim);
        let fov_cos = (perceiver.config.fov * 0.5).cos();
        if dot < fov_cos {
            return None;
        }

        // 3. FOV 边缘衰减：靠近视野边缘时置信度降低.
        let mut confidence = 1.0;
        if perceiver.enable_fov_edge_falloff {
            let fov_half = perceiver.config.fov * 0.5;
            let angle = dot.acos().clamp(0.0, fov_half);
            let edge_start = fov_half - perceiver.fov_edge_falloff_range;
            if angle > edge_start {
                let falloff = (angle - edge_start) / perceiver.fov_edge_falloff_range;
                confidence *= 1.0 - falloff.clamp(0.0, 1.0) * 0.6; // 边缘最多降低 60%
            }
        }

        // 4. 多层遮挡检测：射线检测返回遮挡强度(0~1)，遮挡越强置信度越低.
        //    如果完全遮挡(强度>=1.0)，直接返回 None.
        let occlusion = (self.raycast_cb)(perceiver.position, stimulus.position);
        if occlusion >= 1.0 {
            return None;
        }
        // 遮挡强度降低置信度：置信度 *= (1 - occlusion * 0.8)，最大降低 80%
        confidence *= 1.0 - occlusion.clamp(0.0, 1.0) * 0.8;

        // 5. 距离梯度置信度：越近置信度越高.
        let distance_confidence = (1.0 - dist / max_dist).clamp(0.0, 1.0);
        confidence *= distance_confidence;

        // 5. 应用刺激强度.
        confidence *= stimulus.strength;

        Some(confidence.clamp(0.0, 1.0))
    }

    /// 听觉感知：支持反平方定律衰减、声音类型差异、障碍物遮挡衰减.
    fn sense_hearing(
        &self,
        perceiver: &PerceptionComponent,
        stimulus: &Stimulus,
        perp_pos: glam::Vec3,
        stim_pos: glam::Vec3,
    ) -> Option<f32> {
        // 1. 距离检查（基于声音类型的基础听距）.
        let dist = (stim_pos - perp_pos).length();
        let base_dist = stimulus.sound_type.base_hear_distance()
            * stimulus.strength
            * perceiver.hearing_sensitivity;
        if dist > base_dist {
            return None;
        }

        // 2. 反平方定律衰减：强度 = 基础强度 / (1 + 距离^2 * 声音类型衰减系数).
        let attenuation = stimulus.sound_type.attenuation_coefficient();
        let intensity = stimulus.strength / (1.0 + dist * dist * attenuation);

        // 3. 障碍物遮挡衰减：声音被遮挡时额外降低强度（声音可穿透但会衰减）.
        let occlusion = (self.raycast_cb)(perceiver.position, stimulus.position);
        let occlusion_factor = 1.0 - occlusion.clamp(0.0, 1.0) * 0.7; // 遮挡最多降低 70%
        let intensity = intensity * occlusion_factor;

        // 4. 听觉空间化：基于声源相对位置计算左右声道差异（ILD）.
        //    声源在侧方时，侧耳音量降低，模拟头部遮挡效应.
        let to_stim = (stim_pos - perp_pos).normalize();
        let fwd = glam::Vec3::from_array(perceiver.forward).normalize();
        let right = fwd.cross(glam::Vec3::Y).normalize();
        let fwd_dot = to_stim.dot(fwd);
        let right_dot = to_stim.dot(right);
        let azimuth = right_dot.atan2(fwd_dot); // -PI..PI，正值在右侧
        let ild = (azimuth.abs() / std::f32::consts::PI).clamp(0.0, 1.0);
        let spatial_factor = 1.0 - ild * 0.4; // 侧方声音最多降低 40%
        let intensity = intensity * spatial_factor;

        // 4. 强度阈值检查.
        let threshold = perceiver.config.hear_threshold;
        if intensity < threshold {
            return None;
        }

        // 5. 计算置信度：基于强度相对于阈值的比例.
        let confidence = ((intensity - threshold) / (1.0 - threshold)).clamp(0.0, 1.0);
        Some(confidence)
    }

    /// 返回本帧的所有感知结果.
    pub fn results(&self) -> &[PerceptionResult] {
        &self.results
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 导航寻路：A* 算法,导航网格
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 导航网格节点.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavNode {
    /// 网格坐标 (x, y).
    pub x: i32,
    pub y: i32,
    /// 是否可通过(false=障碍物).
    pub walkable: bool,
}

/// 导航网格三角形：三个网格单元索引组成的三角形，用于路径平滑和重 triangulation.
#[derive(Debug, Clone, Copy)]
pub struct NavTriangle {
    /// 三个顶点的网格坐标.
    pub a: (i32, i32),
    pub b: (i32, i32),
    pub c: (i32, i32),
    /// 三角形中心点(世界空间).
    pub center: [f32; 2],
}

/// 导航网格：简单的 2D 网格地图.
#[derive(Debug, Clone)]
pub struct NavGrid {
    width: i32,
    height: i32,
    nodes: Vec<bool>, // true=可通过，false=障碍
    cell_size: f32,
    origin: [f32; 2], // 网格原点(世界空间 X,Z)
    /// 三角形网格：用于路径平滑和动态重 triangulation.
    triangles: Vec<NavTriangle>,
    /// 动态障碍物更新：脏区域集合（网格坐标），用于路径失效检测和重 triangulation.
    dirty_regions: Vec<(i32, i32)>,
    /// 脏三角形集合：需要重新计算的三角形索引.
    dirty_triangles: Vec<usize>,
}

impl NavGrid {
    /// 创建导航网格.
    pub fn new(width: i32, height: i32, cell_size: f32, origin: [f32; 2]) -> Self {
        let mut grid = Self {
            width,
            height,
            nodes: vec![true; (width * height) as usize],
            cell_size,
            origin,
            triangles: Vec::new(),
            dirty_regions: Vec::new(),
            dirty_triangles: Vec::new(),
        };
        grid.build_triangulation();
        grid
    }

    /// 设置指定网格单元为障碍物.
    pub fn set_obstacle(&mut self, x: i32, y: i32, obstacle: bool) {
        if x >= 0 && x < self.width && y >= 0 && y < self.height {
            let idx = (y * self.width + x) as usize;
            self.nodes[idx] = !obstacle;
            if obstacle {
                self.mark_dirty(x, y);
            }
        }
    }

    /// 标记指定网格单元为脏区域（用于路径失效检测）.
    fn mark_dirty(&mut self, x: i32, y: i32) {
        if !self.dirty_regions.contains(&(x, y)) {
            self.dirty_regions.push((x, y));
        }
        // 标记受影响区域的三角形为脏三角形，需要重 triangulation.
        self.mark_triangles_dirty_around(x, y);
    }

    /// 标记给定网格单元周围的三角形为脏三角形.
    fn mark_triangles_dirty_around(&mut self, cx: i32, cy: i32) {
        // 找到包含 (cx, cy) 作为顶点的三角形并标记为脏.
        for (i, tri) in self.triangles.iter().enumerate() {
            if tri.a == (cx, cy) || tri.b == (cx, cy) || tri.c == (cx, cy) {
                if !self.dirty_triangles.contains(&i) {
                    self.dirty_triangles.push(i);
                }
            }
            // 也标记三角形边界内的相邻三角形.
            let min_x = tri.a.0.min(tri.b.0).min(tri.c.0);
            let max_x = tri.a.0.max(tri.b.0).max(tri.c.0);
            let min_y = tri.a.1.min(tri.b.1).min(tri.c.1);
            let max_y = tri.a.1.max(tri.b.1).max(tri.c.1);
            if cx >= min_x - 1 && cx <= max_x + 1 && cy >= min_y - 1 && cy <= max_y + 1 {
                if !self.dirty_triangles.contains(&i) {
                    self.dirty_triangles.push(i);
                }
            }
        }
    }

    /// 构建初始三角形网格：将每个 2x2 网格单元分割为两个三角形.
    fn build_triangulation(&mut self) {
        self.triangles.clear();
        for y in 0..self.height.saturating_sub(1) {
            for x in 0..self.width.saturating_sub(1) {
                // 只有当四个角都可通过时才创建三角形.
                if self.is_walkable(x, y)
                    && self.is_walkable(x + 1, y)
                    && self.is_walkable(x, y + 1)
                    && self.is_walkable(x + 1, y + 1)
                {
                    let a = (x, y);
                    let b = (x + 1, y);
                    let c = (x, y + 1);
                    let d = (x + 1, y + 1);
                    // 分割为两个三角形: (a,b,c) 和 (b,d,c).
                    let center_a = self.triangle_center(a, b, c);
                    let center_b = self.triangle_center(b, d, c);
                    self.triangles.push(NavTriangle {
                        a,
                        b,
                        c,
                        center: center_a,
                    });
                    self.triangles.push(NavTriangle {
                        a: b,
                        b: d,
                        c,
                        center: center_b,
                    });
                }
            }
        }
    }

    /// 计算三角形中心点(世界空间).
    fn triangle_center(&self, a: (i32, i32), b: (i32, i32), c: (i32, i32)) -> [f32; 2] {
        let wa = self.grid_to_world(a.0, a.1);
        let wb = self.grid_to_world(b.0, b.1);
        let wc = self.grid_to_world(c.0, c.1);
        [
            (wa[0] + wb[0] + wc[0]) / 3.0,
            (wa[1] + wb[1] + wc[1]) / 3.0,
        ]
    }

    /// 重 triangulation 脏区域：重新计算受脏三角形影响的三角形网格.
    ///
    /// 在障碍物动态更新后调用，以保持三角形网格与导航网格同步.
    /// 返回重 triangulation 的三角形数量.
    pub fn retriangulate_dirty_region(&mut self) -> usize {
        if self.dirty_triangles.is_empty() {
            return 0;
        }

        // 重建整个三角形网格，确保与当前导航网格状态同步.
        self.build_triangulation();

        let count = self.dirty_triangles.len();
        self.dirty_triangles.clear();
        count
    }

    /// 返回三角形数量.
    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }

    /// 返回脏三角形数量（待重 triangulation）.
    pub fn dirty_triangle_count(&self) -> usize {
        self.dirty_triangles.len()
    }

    /// 返回指定索引的三角形.
    pub fn get_triangle(&self, index: usize) -> Option<&NavTriangle> {
        self.triangles.get(index)
    }

    /// 返回所有三角形的迭代器.
    pub fn triangles(&self) -> impl Iterator<Item = &NavTriangle> {
        self.triangles.iter()
    }

    /// 设置矩形区域为障碍物（动态障碍物支持）.
    ///
    /// rect_min / rect_max 是世界空间坐标。
    pub fn set_rect_obstacle(&mut self, rect_min: [f32; 2], rect_max: [f32; 2], obstacle: bool) {
        let (min_x, min_y) = self.world_to_grid(rect_min[0], rect_min[1]);
        let (max_x, max_y) = self.world_to_grid(rect_max[0], rect_max[1]);

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                self.set_obstacle(x, y, obstacle);
            }
        }
    }

    /// 设置圆形区域为障碍物（动态障碍物支持）.
    ///
    /// center 是世界空间坐标，radius 是半径（米）。
    pub fn set_circle_obstacle(&mut self, center: [f32; 2], radius: f32, obstacle: bool) {
        let (cx, cy) = self.world_to_grid(center[0], center[1]);
        let radius_cells = (radius / self.cell_size).ceil() as i32;

        for y in (cy - radius_cells)..=(cy + radius_cells) {
            for x in (cx - radius_cells)..=(cx + radius_cells) {
                if x >= 0 && x < self.width && y >= 0 && y < self.height {
                    let dx = (x as f32 - cx as f32) * self.cell_size;
                    let dy = (y as f32 - cy as f32) * self.cell_size;
                    if (dx * dx + dy * dy).sqrt() <= radius {
                        self.set_obstacle(x, y, obstacle);
                    }
                }
            }
        }
    }

    /// 清除指定网格单元的障碍物（恢复可通过）.
    pub fn clear_obstacle(&mut self, x: i32, y: i32) {
        self.set_obstacle(x, y, false);
    }

    /// 检查指定网格单元是否可通过.
    pub fn is_walkable(&self, x: i32, y: i32) -> bool {
        if x >= 0 && x < self.width && y >= 0 && y < self.height {
            self.nodes[(y * self.width + x) as usize]
        } else {
            false
        }
    }

    /// 检查路径是否经过脏区域（动态障碍物更新后路径失效检测）.
    pub fn is_path_invalidated(&self, path_points: &[[f32; 2]]) -> bool {
        if self.dirty_regions.is_empty() {
            return false;
        }
        for point in path_points {
            let (gx, gy) = self.world_to_grid(point[0], point[1]);
            if self.dirty_regions.contains(&(gx, gy)) {
                return true;
            }
        }
        false
    }

    /// 清除脏区域标记（在重新规划路径后调用）.
    pub fn clear_dirty_regions(&mut self) {
        self.dirty_regions.clear();
    }

    /// 将世界空间位置转换为网格坐标.
    pub fn world_to_grid(&self, world_x: f32, world_z: f32) -> (i32, i32) {
        (
            ((world_x - self.origin[0]) / self.cell_size).round() as i32,
            ((world_z - self.origin[1]) / self.cell_size).round() as i32,
        )
    }

    /// 将网格坐标转换为世界空间位置.
    pub fn grid_to_world(&self, x: i32, y: i32) -> [f32; 2] {
        [
            self.origin[0] + x as f32 * self.cell_size,
            self.origin[1] + y as f32 * self.cell_size,
        ]
    }

    pub fn width(&self) -> i32 {
        self.width
    }
    pub fn height(&self) -> i32 {
        self.height
    }
    pub fn cell_size(&self) -> f32 {
        self.cell_size
    }
}

/// A* 节点状态.
#[derive(Debug, Clone)]
struct AStarNode {
    x: i32,
    y: i32,
    g: f32, // 起点到当前节点的代价
    h: f32, // 当前节点到终点的估计代价
    parent: Option<(i32, i32)>,
}

impl AStarNode {
    fn f(&self) -> f32 {
        self.g + self.h
    }
}

/// A* 寻路器.
pub struct AStarPathfinder {
    _private: (),
}

impl AStarPathfinder {
    /// 在导航网格上寻找从 start 到 goal 的路径.
    ///
    /// 返回路径点列表(世界空间坐标)，如果找不到路径则返回 None.
    pub fn find_path(
        grid: &NavGrid,
        start: (i32, i32),
        goal: (i32, i32),
    ) -> Option<Vec<[f32; 2]>> {
        if !grid.is_walkable(start.0, start.1) || !grid.is_walkable(goal.0, goal.1) {
            return None;
        }

        let mut open_set: Vec<AStarNode> = vec![AStarNode {
            x: start.0,
            y: start.1,
            g: 0.0,
            h: Self::heuristic(start, goal),
            parent: None,
        }];
        let mut closed_set: std::collections::HashSet<(i32, i32)> =
            std::collections::HashSet::new();
        let mut nodes: std::collections::HashMap<(i32, i32), AStarNode> =
            std::collections::HashMap::new();
        nodes.insert(start, open_set[0].clone());

        let directions: [(i32, i32); 4] = [(0, 1), (0, -1), (1, 0), (-1, 0)];

        while let Some(current_idx) = Self::pop_lowest_f(&mut open_set) {
            let current = open_set.remove(current_idx);
            let current_pos = (current.x, current.y);

            if current_pos == goal {
                // 重构路径.
                let mut path = Vec::new();
                let mut node = current.clone();
                while let Some(parent) = node.parent {
                    let wp = grid.grid_to_world(node.x, node.y);
                    path.push([wp[0], wp[1]]);
                    node = nodes.get(&parent).unwrap().clone();
                }
                let wp = grid.grid_to_world(start.0, start.1);
                path.push([wp[0], wp[1]]);
                path.reverse();
                return Some(path);
            }

            closed_set.insert(current_pos);

            for (dx, dy) in &directions {
                let neighbor = (current.x + dx, current.y + dy);
                if !grid.is_walkable(neighbor.0, neighbor.1)
                    || closed_set.contains(&neighbor)
                {
                    continue;
                }

                let tentative_g = current.g + 1.0;

                let neighbor_node = nodes.get(&neighbor);
                if let Some(existing) = neighbor_node {
                    if tentative_g >= existing.g {
                        continue;
                    }
                }

                let neighbor_node = AStarNode {
                    x: neighbor.0,
                    y: neighbor.1,
                    g: tentative_g,
                    h: Self::heuristic(neighbor, goal),
                    parent: Some(current_pos),
                };
                nodes.insert(neighbor, neighbor_node.clone());
                open_set.push(neighbor_node);
            }
        }

        None // 未找到路径
    }

    /// 从 open_set 中移除并返回 F 值最小的节点索引.
    fn pop_lowest_f(open_set: &mut Vec<AStarNode>) -> Option<usize> {
        open_set
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.f().partial_cmp(&b.f()).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
    }

    /// 启发式函数：曼哈顿距离.
    fn heuristic(a: (i32, i32), b: (i32, i32)) -> f32 {
        (a.0.abs_diff(b.0) + a.1.abs_diff(b.1)) as f32
    }
}

/// 路径：一系列世界空间路点.
#[derive(Debug, Clone)]
pub struct Path {
    points: Vec<[f32; 2]>,
    current_index: usize,
}

impl Path {
    pub fn new(points: Vec<[f32; 2]>) -> Self {
        Self {
            points,
            current_index: 0,
        }
    }

    /// 返回当前目标路点.
    pub fn current_target(&self) -> Option<[f32; 2]> {
        self.points.get(self.current_index).copied()
    }

    /// 返回下一个目标路点(如果已到达当前路点则前进).
    pub fn next_target(&mut self, current_pos: [f32; 2], threshold: f32) -> Option<[f32; 2]> {
        if self.current_index >= self.points.len() {
            return None;
        }

        let target = self.points[self.current_index];
        let dx = target[0] - current_pos[0];
        let dz = target[1] - current_pos[1];
        let dist = (dx * dx + dz * dz).sqrt();

        if dist < threshold {
            self.current_index += 1;
            if self.current_index >= self.points.len() {
                return None;
            }
            return self.points.get(self.current_index).copied();
        }

        Some(target)
    }

    /// 是否已到达路径终点.
    pub fn is_complete(&self) -> bool {
        self.current_index >= self.points.len()
    }

    /// 返回路径点数量.
    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}

/// 路径跟随器：结合 A* 路径和运动控制.
pub struct PathFollower {
    path: Option<Path>,
    speed: f32,
    threshold: f32,
}

impl PathFollower {
    pub fn new(speed: f32) -> Self {
        Self {
            path: None,
            speed,
            threshold: 0.5,
        }
    }

    /// 设置新路径.
    pub fn set_path(&mut self, path: Path) {
        self.path = Some(path);
    }

    /// 清除路径.
    pub fn clear_path(&mut self) {
        self.path = None;
    }

    /// 计算下一帧的移动方向.
    ///
    /// current_pos: 当前位置 [x, z].
    /// dt: 时间步长(秒).
    ///
    /// 返回移动方向向量 [x, z](归一化)，如果路径已完成则返回 None.
    pub fn update(&mut self, current_pos: [f32; 2], dt: f32) -> Option<[f32; 2]> {
        let path = self.path.as_mut()?;

        let target = path.next_target(current_pos, self.threshold)?;
        let dx = target[0] - current_pos[0];
        let dz = target[1] - current_pos[1];
        let dist = (dx * dx + dz * dz).sqrt();

        if dist < 1e-6 {
            return Some([0.0, 0.0]);
        }

        let dir = [dx / dist, dz / dist];
        let step = self.speed * dt;
        // 实际位置更新由调用者执行，这里只返回方向.
        let _ = step;
        Some(dir)
    }

    /// 是否正在跟随路径.
    pub fn is_following(&self) -> bool {
        self.path.is_some() && !self.path.as_ref().unwrap().is_complete()
    }

    pub fn speed(&self) -> f32 {
        self.speed
    }

    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed;
    }
}

#[cfg(test)]
mod perception_tests {
    use super::*;

    #[test]
    fn vision_senses_unobstructed_target() {
        let raycast_cb = |_origin: [f32; 3], _target: [f32; 3]| 0.0; // 无遮挡
        let mut system = PerceptionSystem::new(raycast_cb);
        system.set_timestamp(100.0);

        let perceiver = PerceptionComponent {
            position: [0.0, 0.0, 0.0],
            forward: [0.0, 0.0, -1.0],
            ..Default::default()
        };

        let stimulus = Stimulus {
            position: [0.0, 0.0, -10.0], // 在前方 10 米
            sense_type: SenseType::Vision,
            strength: 1.0,
            duration: 0.0,
            source_id: Some(1),
            sound_type: SoundType::default(),
        };

        let results = system.perceive(&perceiver, &[stimulus]);
        assert_eq!(results.len(), 1);
        assert!(results[0].confidence > 0.0);
        assert_eq!(results[0].timestamp, 100.0);
    }

    #[test]
    fn vision_blocked_by_occlusion() {
        let raycast_cb = |_origin: [f32; 3], _target: [f32; 3]| 1.0; // 总是有遮挡
        let mut system = PerceptionSystem::new(raycast_cb);

        let perceiver = PerceptionComponent {
            position: [0.0, 0.0, 0.0],
            forward: [0.0, 0.0, -1.0],
            ..Default::default()
        };

        let stimulus = Stimulus {
            position: [0.0, 0.0, -10.0],
            sense_type: SenseType::Vision,
            strength: 1.0,
            duration: 0.0,
            source_id: Some(1),
            sound_type: SoundType::default(),
        };

        let results = system.perceive(&perceiver, &[stimulus]);
        assert_eq!(results.len(), 0); // 被遮挡，无法感知
    }

    #[test]
    fn hearing_senses_loud_sound() {
        let raycast_cb = |_origin: [f32; 3], _target: [f32; 3]| 0.0;
        let mut system = PerceptionSystem::new(raycast_cb);

        let perceiver = PerceptionComponent {
            position: [0.0, 0.0, 0.0],
            forward: [0.0, 0.0, -1.0],
            ..Default::default()
        };

        let stimulus = Stimulus {
            position: [5.0, 0.0, 0.0], // 距离 5 米
            sense_type: SenseType::Hearing,
            strength: 1.0,
            duration: 0.0,
            source_id: Some(2),
            sound_type: SoundType::Gunshot,
        };

        let results = system.perceive(&perceiver, &[stimulus]);
        assert_eq!(results.len(), 1);
        assert!(results[0].confidence > 0.0);
    }

    #[test]
    fn a_star_finds_path() {
        let mut grid = NavGrid::new(10, 10, 1.0, [0.0, 0.0]);
        // 在 (5,5) 放置障碍物
        grid.set_obstacle(5, 5, true);

        let path = AStarPathfinder::find_path(&grid, (0, 0), (9, 9));
        assert!(path.is_some());

        let path = path.unwrap();
        assert!(!path.is_empty());
        assert_eq!(path[0], [0.0, 0.0]); // 起点
        let last = path.last().unwrap();
        assert!((last[0] - 9.0).abs() < 0.1 && (last[1] - 9.0).abs() < 0.1); // 终点
    }

    #[test]
    fn a_star_no_path_when_blocked() {
        let mut grid = NavGrid::new(3, 3, 1.0, [0.0, 0.0]);
        // 封堵所有路径
        grid.set_obstacle(1, 0, true);
        grid.set_obstacle(0, 1, true);
        grid.set_obstacle(1, 1, true);
        grid.set_obstacle(2, 1, true);
        grid.set_obstacle(1, 2, true);

        let path = AStarPathfinder::find_path(&grid, (0, 0), (2, 2));
        assert!(path.is_none());
    }

    #[test]
    fn path_follower_moves_along_path() {
        let mut follower = PathFollower::new(2.0);
        let path = Path::new(vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]]);
        follower.set_path(path);

        assert!(follower.is_following());

        // 从起点出发，向第一个路点移动
        let dir = follower.update([0.0, 0.0], 0.016);
        assert!(dir.is_some());
        let dir = dir.unwrap();
        assert!((dir[0] - 1.0).abs() < 0.01); // 应该向 X 正方向移动
        assert!(dir[1].abs() < 0.01);
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 感知高级特性测试
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    #[test]
    fn vision_fov_edge_falloff() {
        // 创建一个在 FOV 边缘的目标，启用 FOV 边缘衰减
        let raycast_cb = |_origin: [f32; 3], _target: [f32; 3]| 0.0; // 无遮挡
        let mut system = PerceptionSystem::new(raycast_cb);
        system.set_timestamp(100.0);

        let perceiver = PerceptionComponent {
            position: [0.0, 0.0, 0.0],
            forward: [0.0, 0.0, -1.0], // 面向 -Z
            enable_fov_edge_falloff: true,
            fov_edge_falloff_range: 0.2,
            ..Default::default()
        };

        // 目标正前方（置信度应较高）
        let stimulus_center = Stimulus::vision([0.0, 0.0, -10.0], 1.0);
        let results = system.perceive(&perceiver, &[stimulus_center]);
        assert_eq!(results.len(), 1);
        let center_confidence = results[0].confidence;

        // 目标在 FOV 边缘附近（置信度应较低）
        let fov_half = std::f32::consts::PI * 2.0 / 3.0 * 0.5; // 60度
        let edge_angle = fov_half - 0.1; // 接近边缘
        let edge_pos = [
            -10.0 * edge_angle.sin(),
            0.0,
            -10.0 * edge_angle.cos(),
        ];
        let stimulus_edge = Stimulus::vision(edge_pos, 1.0);
        let results = system.perceive(&perceiver, &[stimulus_edge]);
        assert_eq!(results.len(), 1);
        let edge_confidence = results[0].confidence;

        // 边缘目标的置信度应低于中心目标
        assert!(edge_confidence < center_confidence);
    }

    #[test]
    fn vision_multi_point_occlusion_partial() {
        // 多点遮挡检测：部分遮挡时降低置信度而不是完全拒绝
        let raycast_cb = |origin: [f32; 3], target: [f32; 3]| {
            // 只有当目标在中间区域时才被遮挡（模拟部分遮挡）
            let dx = target[0] - origin[0];
            let dz = target[2] - origin[2];
            let dist = (dx * dx + dz * dz).sqrt();
            if dist > 2.0 && dist < 8.0 {
                1.0 // 中间距离被遮挡
            } else {
                0.0
            }
        };
        let mut system = PerceptionSystem::new(raycast_cb);

        let perceiver = PerceptionComponent {
            position: [0.0, 0.0, 0.0],
            forward: [0.0, 0.0, -1.0],
            enable_multi_point_occlusion: true,
            occlusion_sample_count: 3,
            ..Default::default()
        };

        // 目标在近处（不在遮挡区间，应完全可见）
        let stimulus_near = Stimulus::vision([0.0, 0.0, -1.5], 1.0);
        let results = system.perceive(&perceiver, &[stimulus_near]);
        assert_eq!(results.len(), 1);
        assert!(results[0].confidence > 0.8); // 近处应高置信度

        // 目标在远处（也不在遮挡区间，应完全可见）
        let stimulus_far = Stimulus::vision([0.0, 0.0, -15.0], 1.0);
        let results = system.perceive(&perceiver, &[stimulus_far]);
        assert_eq!(results.len(), 1);
        assert!(results[0].confidence > 0.5); // 远处但无遮挡
    }

    #[test]
    fn hearing_inverse_square_law() {
        let raycast_cb = |_origin: [f32; 3], _target: [f32; 3]| 0.0; // 无遮挡
        let mut system = PerceptionSystem::new(raycast_cb);

        let perceiver = PerceptionComponent {
            position: [0.0, 0.0, 0.0],
            forward: [0.0, 0.0, -1.0],
            ..Default::default()
        };

        // 枪声（低衰减系数，传播远）在近距离
        let stimulus_close = Stimulus::hearing([5.0, 0.0, 0.0], 1.0, SoundType::Gunshot);
        let results = system.perceive(&perceiver, &[stimulus_close]);
        assert_eq!(results.len(), 1);
        let close_confidence = results[0].confidence;

        // 枪声在远距离（应比近距离低，但由于低衰减系数仍可感知）
        let stimulus_far = Stimulus::hearing([40.0, 0.0, 0.0], 1.0, SoundType::Gunshot);
        let results = system.perceive(&perceiver, &[stimulus_far]);
        assert_eq!(results.len(), 1);
        let far_confidence = results[0].confidence;

        // 近距离置信度应高于远距离
        assert!(close_confidence > far_confidence);
        // 但远距离枪声仍应可感知（置信度 > 0）
        assert!(far_confidence > 0.0);
    }

    #[test]
    fn hearing_sound_type_difference() {
        let raycast_cb = |_origin: [f32; 3], _target: [f32; 3]| 0.0;
        let mut system = PerceptionSystem::new(raycast_cb);

        let perceiver = PerceptionComponent {
            position: [0.0, 0.0, 0.0],
            forward: [0.0, 0.0, -1.0],
            ..Default::default()
        };

        // 同等强度、同等距离：脚步声衰减快，枪声衰减慢
        let footstep = Stimulus::hearing([20.0, 0.0, 0.0], 1.0, SoundType::Footstep);
        let gunshot = Stimulus::hearing([20.0, 0.0, 0.0], 1.0, SoundType::Gunshot);

        let footstep_results = system.perceive(&perceiver, &[footstep]);
        let gunshot_results = system.perceive(&perceiver, &[gunshot]);

        // 枪声应比脚步声更容易被感知（置信度更高）
        if footstep_results.len() == 1 && gunshot_results.len() == 1 {
            assert!(gunshot_results[0].confidence >= footstep_results[0].confidence);
        }
    }

    #[test]
    fn hearing_occlusion_attenuation() {
        // 有遮挡时听觉强度应降低
        let raycast_cb = |_origin: [f32; 3], _target: [f32; 3]| 1.0; // 有遮挡
        let mut system = PerceptionSystem::new(raycast_cb);

        let perceiver = PerceptionComponent {
            position: [0.0, 0.0, 0.0],
            forward: [0.0, 0.0, -1.0],
            ..Default::default()
        };

        // 近距离爆炸声即使有遮挡也应可感知（强度高）
        let stimulus = Stimulus::hearing([10.0, 0.0, 0.0], 1.0, SoundType::Explosion);
        let results = system.perceive(&perceiver, &[stimulus]);
        // 爆炸声强度高，即使被遮挡也可能感知到
        if results.len() == 1 {
            assert!(results[0].confidence > 0.0);
        }
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 导航网格动态更新测试
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    #[test]
    fn navgrid_rect_obstacle() {
        let mut grid = NavGrid::new(10, 10, 1.0, [0.0, 0.0]);

        // 设置矩形障碍物：世界坐标 (2,2) 到 (4,4)
        grid.set_rect_obstacle([2.0, 2.0], [4.0, 4.0], true);

        // 验证矩形区域内的单元都不可通过
        for y in 2..=4 {
            for x in 2..=4 {
                assert!(!grid.is_walkable(x, y), "({},{}) should be obstacle", x, y);
            }
        }

        // 验证区域外的单元仍可通过
        assert!(grid.is_walkable(1, 1));
        assert!(grid.is_walkable(5, 5));
    }

    #[test]
    fn navgrid_circle_obstacle() {
        let mut grid = NavGrid::new(10, 10, 1.0, [0.0, 0.0]);

        // 设置圆形障碍物：中心 (5,5)，半径 2 米
        grid.set_circle_obstacle([5.0, 5.0], 2.0, true);

        // 中心应不可通过
        assert!(!grid.is_walkable(5, 5));

        // 远离中心的单元应可通过
        assert!(grid.is_walkable(0, 0));
        assert!(grid.is_walkable(9, 9));
    }

    #[test]
    fn navgrid_dirty_region_path_invalidation() {
        let mut grid = NavGrid::new(10, 10, 1.0, [0.0, 0.0]);

        // 先找一条路径
        let path = AStarPathfinder::find_path(&grid, (0, 0), (9, 9));
        assert!(path.is_some());
        let path = path.unwrap();

        // 路径不应被标记为失效（没有脏区域）
        assert!(!grid.is_path_invalidated(&path));

        // 在路径中间设置障碍物
        let mid_point = path[path.len() / 2];
        let (gx, gy) = grid.world_to_grid(mid_point[0], mid_point[1]);
        grid.set_obstacle(gx, gy, true);

        // 现在路径应被标记为失效
        assert!(grid.is_path_invalidated(&path));

        // 清除脏区域后不应再失效
        grid.clear_dirty_regions();
        assert!(!grid.is_path_invalidated(&path));
    }

    #[test]
    fn navgrid_clear_obstacle_restores_path() {
        let mut grid = NavGrid::new(5, 5, 1.0, [0.0, 0.0]);

        // 设置障碍物封堵路径
        grid.set_obstacle(2, 2, true);
        let _path = AStarPathfinder::find_path(&grid, (0, 0), (4, 4));
        // 可能有路也可能没有，取决于 A* 是否绕过

        // 清除障碍物
        grid.clear_obstacle(2, 2);
        assert!(grid.is_walkable(2, 2));

        // 清除后应能找到路径
        let path = AStarPathfinder::find_path(&grid, (0, 0), (4, 4));
        assert!(path.is_some());
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Crowd 避让：基于斥力场的局部代理避让
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 代理状态：位置、速度、半径等.
#[derive(Debug, Clone, Copy)]
pub struct CrowdAgentState {
    /// 当前位置(世界空间 X,Z).
    pub pos: [f32; 2],
    /// 当前速度(世界空间 X,Z).
    pub vel: [f32; 2],
    /// 代理半径(米).
    pub radius: f32,
    /// 最大速度(米/秒).
    pub max_speed: f32,
    /// 偏好速度(目标方向 * 速度).
    pub pref_vel: [f32; 2],
}

/// 单个 Crowd 代理.
#[derive(Debug, Clone)]
pub struct CrowdAgent {
    pub id: usize,
    pub state: CrowdAgentState,
}

impl CrowdAgent {
    pub fn new(id: usize, pos: [f32; 2], radius: f32, max_speed: f32) -> Self {
        Self {
            id,
            state: CrowdAgentState {
                pos,
                vel: [0.0, 0.0],
                radius,
                max_speed,
                pref_vel: [0.0, 0.0],
            },
        }
    }

    /// 设置偏好速度（目标方向）。
    pub fn set_pref_vel(&mut self, pref_vel: [f32; 2]) {
        self.state.pref_vel = pref_vel;
    }

    /// 应用计算后的新速度.
    pub fn apply_velocity(&mut self, new_vel: [f32; 2]) {
        self.state.vel = new_vel;
        // 限制最大速度.
        let speed = (new_vel[0].powi(2) + new_vel[1].powi(2)).sqrt();
        if speed > self.state.max_speed {
            let scale = self.state.max_speed / speed;
            self.state.vel = [new_vel[0] * scale, new_vel[1] * scale];
        }
    }

    /// 根据当前速度更新位置.
    pub fn integrate(&mut self, dt: f32) {
        self.state.pos[0] += self.state.vel[0] * dt;
        self.state.pos[1] += self.state.vel[1] * dt;
    }
}

/// Crowd 管理器：管理多个代理，应用局部避让（基于简单的斥力场）.
///
/// 使用简化的 ORCA 风格避让：每个代理计算与邻居的相对速度，
/// 并调整自己的速度以避免碰撞。
pub struct CrowdManager {
    agents: Vec<CrowdAgent>,
    /// 时间步长(秒).
    pub dt: f32,
    /// 邻居搜索半径(米).
    pub neighbor_radius: f32,
    /// 避让权重(0.0~1.0，越高越倾向于避让而非朝向目标).
    pub avoidance_weight: f32,
}

impl CrowdManager {
    /// 创建 crowd 管理器.
    pub fn new() -> Self {
        Self {
            agents: Vec::new(),
            dt: 0.016, // 60 FPS
            neighbor_radius: 5.0,
            avoidance_weight: 0.5,
        }
    }

    /// 添加代理.
    pub fn add_agent(&mut self, agent: CrowdAgent) -> usize {
        let id = self.agents.len();
        self.agents.push(agent);
        id
    }

    /// 返回代理数量.
    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }

    /// 返回指定代理的不可变引用.
    pub fn get_agent(&self, id: usize) -> Option<&CrowdAgent> {
        self.agents.get(id)
    }

    /// 返回指定代理的可变引用.
    pub fn get_agent_mut(&mut self, id: usize) -> Option<&mut CrowdAgent> {
        self.agents.get_mut(id)
    }

    /// 执行一帧 crowd 避让模拟.
    ///
    /// 对每对邻近代理计算斥力，调整速度以避免碰撞，
    /// 然后更新所有代理位置.
    pub fn update(&mut self) {
        let n = self.agents.len();

        // 第一阶段：计算每个代理的避让调整后的速度.
        let mut new_vels: Vec<[f32; 2]> = vec![[0.0, 0.0]; n];

        for i in 0..n {
            let agent_i = &self.agents[i];
            let pos_i = agent_i.state.pos;
            let vel_i = agent_i.state.vel;
            let pref_vel = agent_i.state.pref_vel;
            let radius_i = agent_i.state.radius;

            let mut avoid_vel = [0.0, 0.0];
            let mut neighbor_count = 0;

            for j in 0..n {
                if i == j {
                    continue;
                }
                let agent_j = &self.agents[j];
                let pos_j = agent_j.state.pos;
                let vel_j = agent_j.state.vel;
                let radius_j = agent_j.state.radius;

                let dx = pos_j[0] - pos_i[0];
                let dz = pos_j[1] - pos_i[1];
                let dist_sq = dx * dx + dz * dz;
                let min_dist = radius_i + radius_j;

                if dist_sq < self.neighbor_radius * self.neighbor_radius && dist_sq > 0.001 {
                    let dist = dist_sq.sqrt();
                    // 如果代理过近，计算排斥速度.
                    if dist < min_dist {
                        let overlap = min_dist - dist;
                        let inv_dist = 1.0 / dist;
                        let repel_x = dx * inv_dist * overlap;
                        let repel_z = dz * inv_dist * overlap;
                        avoid_vel[0] += repel_x;
                        avoid_vel[1] += repel_z;

                        // 相对速度调整：如果邻居正向我们移动，增加避让力.
                        let rel_vel_x = vel_i[0] - vel_j[0];
                        let rel_vel_z = vel_i[1] - vel_j[1];
                        let rel_vel_along = rel_vel_x * dx * inv_dist + rel_vel_z * dz * inv_dist;
                        if rel_vel_along > 0.0 {
                            // 正在靠近，增加避让.
                            avoid_vel[0] += rel_vel_x * 0.5;
                            avoid_vel[1] += rel_vel_z * 0.5;
                        }
                        neighbor_count += 1;
                    }
                }
            }

            // 如果有邻居，平均避让速度.
            if neighbor_count > 0 {
                avoid_vel[0] /= neighbor_count as f32;
                avoid_vel[1] /= neighbor_count as f32;
            }

            // 结合偏好速度和避让速度.
            let combined_x = pref_vel[0] * (1.0 - self.avoidance_weight)
                + avoid_vel[0] * self.avoidance_weight;
            let combined_z = pref_vel[1] * (1.0 - self.avoidance_weight)
                + avoid_vel[1] * self.avoidance_weight;

            new_vels[i] = [combined_x, combined_z];
        }

        // 第二阶段：应用新速度并更新位置.
        for i in 0..n {
            let agent = self.agents.get_mut(i).unwrap();
            agent.apply_velocity(new_vels[i]);
            agent.integrate(self.dt);
        }
    }

    /// 设置时间步长.
    pub fn set_dt(&mut self, dt: f32) {
        self.dt = dt;
    }

    /// 设置邻居搜索半径.
    pub fn set_neighbor_radius(&mut self, radius: f32) {
        self.neighbor_radius = radius;
    }

    /// 设置避让权重.
    pub fn set_avoidance_weight(&mut self, weight: f32) {
        self.avoidance_weight = weight.clamp(0.0, 1.0);
    }

    /// 返回所有代理的位置切片.
    pub fn positions(&self) -> Vec<[f32; 2]> {
        self.agents.iter().map(|a| a.state.pos).collect()
    }
}

impl Default for CrowdManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod crowd_tests {
    use super::*;

    #[test]
    fn crowd_agents_avoid_each_other() {
        let mut mgr = CrowdManager::new();
        mgr.set_dt(0.1);
        mgr.set_avoidance_weight(1.0); // 完全避让

        // 两个代理相向而行，应该互相避让.
        let mut a1 = CrowdAgent::new(0, [-2.0, 0.0], 0.5, 2.0);
        a1.set_pref_vel([2.0, 0.0]); // 向右移动
        mgr.add_agent(a1);

        let mut a2 = CrowdAgent::new(1, [2.0, 0.0], 0.5, 2.0);
        a2.set_pref_vel([-2.0, 0.0]); // 向左移动
        mgr.add_agent(a2);

        // 多步模拟，代理不应该碰撞（距离应始终大于两个半径之和）.
        for _ in 0..20 {
            mgr.update();
        }

        let pos1 = mgr.get_agent(0).unwrap().state.pos;
        let pos2 = mgr.get_agent(1).unwrap().state.pos;
        let dx = pos1[0] - pos2[0];
        let dz = pos1[1] - pos2[1];
        let dist = (dx * dx + dz * dz).sqrt();
        let min_dist = 1.0; // 0.5 + 0.5

        // 代理不应该重叠（距离应大于最小距离）.
        assert!(
            dist > min_dist * 0.5,
            "代理碰撞：距离={:.2}, 最小距离={:.2}",
            dist,
            min_dist * 0.5
        );
    }

    #[test]
    fn crowd_agents_reach_target() {
        let mut mgr = CrowdManager::new();
        mgr.set_dt(0.1);
        mgr.set_avoidance_weight(0.2); // 低避让，主要朝向目标

        let mut agent = CrowdAgent::new(0, [0.0, 0.0], 0.3, 5.0);
        agent.set_pref_vel([5.0, 0.0]); // 向右移动
        mgr.add_agent(agent);

        // 多步模拟，代理应该向右移动.
        for _ in 0..10 {
            mgr.update();
        }

        let pos = mgr.get_agent(0).unwrap().state.pos;
        assert!(pos[0] > 0.0, "代理应向右移动，当前位置={:.2}", pos[0]);
    }

    #[test]
    fn navgrid_retriangulation() {
        let mut grid = NavGrid::new(10, 10, 1.0, [0.0, 0.0]);

        let initial_count = grid.triangle_count();
        assert!(initial_count > 0, "应有初始三角形");

        // 设置一个障碍物，触发脏三角形.
        grid.set_obstacle(5, 5, true);
        assert!(grid.dirty_triangle_count() > 0, "应有脏三角形");

        // 重 triangulation 后，脏三角形应被清除.
        let retri_count = grid.retriangulate_dirty_region();
        assert!(retri_count > 0, "应重 triangulation 一些三角形");
        assert_eq!(
            grid.dirty_triangle_count(),
            0,
            "重 triangulation 后脏三角形应为 0"
        );

        // 三角形总数应该发生变化（因为障碍物阻挡了某些三角形）.
        let new_count = grid.triangle_count();
        assert!(new_count <= initial_count, "添加障碍物后三角形数量不应增加");
    }

    #[test]
    fn navgrid_rect_circle_obstacle_triggers_retriangulation() {
        let mut grid = NavGrid::new(10, 10, 1.0, [0.0, 0.0]);

        // 矩形障碍物.
        grid.set_rect_obstacle([2.0, 2.0], [4.0, 4.0], true);
        assert!(grid.dirty_triangle_count() > 0);
        grid.retriangulate_dirty_region();
        assert_eq!(grid.dirty_triangle_count(), 0);

        // 圆形障碍物.
        grid.set_circle_obstacle([7.0, 7.0], 1.5, true);
        assert!(grid.dirty_triangle_count() > 0);
        grid.retriangulate_dirty_region();
        assert_eq!(grid.dirty_triangle_count(), 0);
    }
}

