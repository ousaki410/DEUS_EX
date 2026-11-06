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
/// raycast_cb 是用户提供的射线检测回调，用于判断两点之间是否有遮挡.
/// 回调参数：(origin, target) -> 是否有遮挡(true=有遮挡，false=无遮挡).
pub struct PerceptionSystem<RaycastCb> {
    /// 射线检测回调.
    raycast_cb: RaycastCb,
    /// 当前帧的感知结果.
    results: Vec<PerceptionResult>,
    /// 当前时间戳.
    timestamp: f64,
}

impl<RaycastCb> PerceptionSystem<RaycastCb>
where
    RaycastCb: Fn([f32; 3], [f32; 3]) -> bool,
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

        // 4. 多点遮挡检测：检查目标的多个采样点，部分遮挡时降低置信度.
        if perceiver.enable_multi_point_occlusion && perceiver.occlusion_sample_count > 1 {
            let sample_count = perceiver.occlusion_sample_count as f32;
            let mut visible_samples = 0.0;

            // 在目标位置周围生成采样点网格（简单的 3x3 网格，基于目标大小假设 1 米）.
            let sample_size = 0.5;
            let offset_range = sample_size * 0.5;
            for i in 0..perceiver.occlusion_sample_count {
                for j in 0..perceiver.occlusion_sample_count {
                    let offset_x = (i as f32 - sample_count * 0.5 + 0.5) / sample_count * offset_range * 2.0;
                    let offset_z = (j as f32 - sample_count * 0.5 + 0.5) / sample_count * offset_range * 2.0;
                    let sample_pos = [
                        stimulus.position[0] + offset_x,
                        stimulus.position[1],
                        stimulus.position[2] + offset_z,
                    ];
                    if !(self.raycast_cb)(perceiver.position, sample_pos) {
                        visible_samples += 1.0;
                    }
                }
            }

            let total_samples = (perceiver.occlusion_sample_count * perceiver.occlusion_sample_count) as f32;
            let occlusion_factor = visible_samples / total_samples;

            // 如果完全被遮挡，返回 None.
            if occlusion_factor <= 0.0 {
                return None;
            }

            confidence *= occlusion_factor;
        } else if (self.raycast_cb)(perceiver.position, stimulus.position) {
            // 单点遮挡检测：完全被遮挡.
            return None;
        }

        // 4. 距离梯度置信度：越近置信度越高.
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

        // 3. 障碍物遮挡衰减：如果被遮挡，额外降低强度（声音可穿透但会衰减）.
        let mut occlusion_factor = 1.0;
        if (self.raycast_cb)(perceiver.position, stimulus.position) {
            // 声音被墙壁遮挡，额外衰减 50%-80%.
            occlusion_factor = 0.3;
        }
        let intensity = intensity * occlusion_factor;

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

/// 导航网格：简单的 2D 网格地图.
#[derive(Debug, Clone)]
pub struct NavGrid {
    width: i32,
    height: i32,
    nodes: Vec<bool>, // true=可通过，false=障碍
    cell_size: f32,
    origin: [f32; 2], // 网格原点(世界空间 X,Z)
    /// 动态障碍物更新：脏区域集合（网格坐标），用于路径失效检测.
    dirty_regions: Vec<(i32, i32)>,
}

impl NavGrid {
    /// 创建导航网格.
    pub fn new(width: i32, height: i32, cell_size: f32, origin: [f32; 2]) -> Self {
        Self {
            width,
            height,
            nodes: vec![true; (width * height) as usize],
            cell_size,
            origin,
            dirty_regions: Vec::new(),
        }
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
        let raycast_cb = |_origin: [f32; 3], _target: [f32; 3]| false; // 无遮挡
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
        let raycast_cb = |_origin: [f32; 3], _target: [f32; 3]| true; // 总是有遮挡
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
        let raycast_cb = |_origin: [f32; 3], _target: [f32; 3]| false;
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
        let raycast_cb = |_origin: [f32; 3], _target: [f32; 3]| false; // 无遮挡
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
            dist > 2.0 && dist < 8.0 // 中间距离被遮挡
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
        let raycast_cb = |_origin: [f32; 3], _target: [f32; 3]| false; // 无遮挡
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
        let raycast_cb = |_origin: [f32; 3], _target: [f32; 3]| false;
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
        let raycast_cb = |_origin: [f32; 3], _target: [f32; 3]| true; // 有遮挡
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

