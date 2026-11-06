//! deusex-ai - AI 行为树与感知寻路系统.
//!
//! 提供基于行为树(Behavior Tree)的 AI 决策框架，适用于 Deus Ex 这类
//! 3D 游戏中的 NPC 行为控制.
//!
//! 核心组件:
//! - Status: 节点执行状态
//! - BehaviorTree: 行为树
//! - Blackboard: 键值对黑板
//! - Sequence/Selector/Inverter/Repeater: 组合节点
//! - ActionNode/ConditionNode: 叶子节点
//! - BehaviorTreeExecutor: 行为树执行器
//! - PerceptionSystem: 感知系统
//! - PerceptionComponent: 感知组件
//! - Stimulus/PerceptionResult: 感知刺激与结果
//! - SenseType/SoundType: 感知类型
//! - NavGrid/AStarPathfinder/Path/PathFollower: 导航寻路
//! - NavTriangle/NavGrid::retriangulate_dirty_region: 动态重 triangulation
//! - CrowdAgent/CrowdManager: Crowd 避让(ORCA)
//! - GoapState/GoapAction/GoapPlanner/GoapAgent: GOAP 规划

use std::collections::HashMap;

use glam::Vec3;
use tracing::{debug, info};

/// 行为节点执行状态.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Running,
    Success,
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

/// 黑板.
#[derive(Debug, Default, Clone)]
pub struct Blackboard {
    values: HashMap<String, BlackboardValue>,
}

impl Blackboard {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set_bool(&mut self, key: &str, value: bool) {
        self.values.insert(key.to_string(), BlackboardValue::Bool(value));
    }
    pub fn set_int(&mut self, key: &str, value: i32) {
        self.values.insert(key.to_string(), BlackboardValue::Int(value));
    }
    pub fn set_float(&mut self, key: &str, value: f32) {
        self.values.insert(key.to_string(), BlackboardValue::Float(value));
    }
    pub fn set_string(&mut self, key: &str, value: &str) {
        self.values.insert(key.to_string(), BlackboardValue::String(value.to_string()));
    }
    pub fn set_vec3(&mut self, key: &str, value: [f32; 3]) {
        self.values.insert(key.to_string(), BlackboardValue::Vec3(value));
    }
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.values.get(key).and_then(|v| v.as_bool())
    }
    pub fn get_int(&self, key: &str) -> Option<i32> {
        self.values.get(key).and_then(|v| v.as_int())
    }
    pub fn get_float(&self, key: &str) -> Option<f32> {
        self.values.get(key).and_then(|v| v.as_float())
    }
    pub fn get_string(&self, key: &str) -> Option<String> {
        self.values.get(key).and_then(|v| v.as_string().map(|s| s.to_string()))
    }
    pub fn get_vec3(&self, key: &str) -> Option<[f32; 3]> {
        self.values.get(key).and_then(|v| v.as_vec3())
    }
    pub fn contains(&self, key: &str) -> bool {
        self.values.contains_key(key)
    }
    pub fn remove(&mut self, key: &str) {
        self.values.remove(key);
    }
}

/// 行为节点 trait.
pub trait Node: std::fmt::Debug + Send + Sync {
    fn tick(&mut self, blackboard: &mut Blackboard) -> Status;
    fn name(&self) -> &str;
}

/// 动作叶子节点.
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
        Self { name: name.to_string(), func }
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

/// 条件叶子节点.
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
        Self { name: name.to_string(), func }
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

/// 顺序组合节点.
#[derive(Debug)]
pub struct Sequence {
    name: String,
    children: Vec<Box<dyn Node>>,
    last_running: usize,
}

impl Sequence {
    pub fn new(name: &str) -> Self {
        Self { name: name.to_string(), children: Vec::new(), last_running: 0 }
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
        self.last_running = 0;
        Status::Success
    }
    fn name(&self) -> &str {
        &self.name
    }
}

/// 选择器组合节点.
#[derive(Debug)]
pub struct Selector {
    name: String,
    children: Vec<Box<dyn Node>>,
    last_running: usize,
}

impl Selector {
    pub fn new(name: &str) -> Self {
        Self { name: name.to_string(), children: Vec::new(), last_running: 0 }
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
        self.last_running = 0;
        Status::Failure
    }
    fn name(&self) -> &str {
        &self.name
    }
}

/// 反转装饰器.
#[derive(Debug)]
pub struct Inverter {
    name: String,
    child: Box<dyn Node>,
}

impl Inverter {
    pub fn new(name: &str, child: Box<dyn Node>) -> Self {
        Self { name: name.to_string(), child }
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

/// 重复装饰器.
#[derive(Debug)]
pub struct Repeater {
    name: String,
    child: Box<dyn Node>,
    max_repeats: i32,
    count: i32,
}

impl Repeater {
    pub fn new(name: &str, child: Box<dyn Node>, max_repeats: i32) -> Self {
        Self { name: name.to_string(), child, max_repeats, count: 0 }
    }
}

impl Node for Repeater {
    fn tick(&mut self, blackboard: &mut Blackboard) -> Status {
        if self.max_repeats == -1 {
            match self.child.tick(blackboard) {
                Status::Running => Status::Running,
                Status::Success | Status::Failure => Status::Running,
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

/// 行为树.
pub struct BehaviorTree {
    name: String,
    root: Option<Box<dyn Node>>,
    blackboard: Blackboard,
}

impl BehaviorTree {
    pub fn new(name: &str) -> Self {
        Self { name: name.to_string(), root: None, blackboard: Blackboard::new() }
    }
    pub fn set_root(&mut self, root: Box<dyn Node>) {
        self.root = Some(root);
    }
    pub fn blackboard_mut(&mut self) -> &mut Blackboard {
        &mut self.blackboard
    }
    pub fn blackboard(&self) -> &Blackboard {
        &self.blackboard
    }
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl std::fmt::Debug for BehaviorTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BehaviorTree").field("name", &self.name).field("has_root", &self.root.is_some()).finish()
    }
}

/// 行为树执行器.
pub struct BehaviorTreeExecutor {
    tree: BehaviorTree,
    debug: bool,
}

impl BehaviorTreeExecutor {
    pub fn new(tree: BehaviorTree) -> Self {
        info!("ai: created behavior tree executor '{}'", tree.name());
        Self { tree, debug: false }
    }
    pub fn set_debug(&mut self, debug: bool) {
        self.debug = debug;
    }
    pub fn tree_mut(&mut self) -> &mut BehaviorTree {
        &mut self.tree
    }
    pub fn tree(&self) -> &BehaviorTree {
        &self.tree
    }
    pub fn tick(&mut self) -> Status {
        let root = match &mut self.tree.root {
            Some(r) => r,
            None => return Status::Success,
        };
        let status = root.tick(&mut self.tree.blackboard);
        if self.debug {
            debug!("ai: tree '{}' tick -> {:?}", self.tree.name(), status);
        }
        status
    }
    pub fn blackboard(&self) -> &Blackboard {
        self.tree.blackboard()
    }
    pub fn blackboard_mut(&mut self) -> &mut Blackboard {
        self.tree.blackboard_mut()
    }
}

impl std::fmt::Debug for BehaviorTreeExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BehaviorTreeExecutor").field("tree", &self.tree).finish()
    }
}

impl Default for BehaviorTreeExecutor {
    fn default() -> Self {
        Self::new(BehaviorTree::new("default"))
    }
}

/// 预定义 AI 行为节点构建器.
pub mod builtin {
    use super::*;
    pub fn has_target() -> Box<dyn Node> {
        Box::new(ConditionNode::new("HasTarget", |bb| bb.contains("target")))
    }
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
                    let step = speed * 0.016;
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
    pub fn wait(seconds: f32) -> Box<dyn Node> {
        Box::new(ActionNode::new("Wait", move |bb| {
            let key = "wait_elapsed";
            let elapsed = bb.get_float(key).unwrap_or(0.0);
            let new_elapsed = elapsed + 0.016;
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

/// 感知类型.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SenseType {
    Vision,
    Hearing,
}

/// 声音类型.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SoundType {
    #[default]
    Footstep,
    Gunshot,
    Conversation,
    Explosion,
}

impl SoundType {
    pub fn attenuation_coefficient(&self) -> f32 {
        match self {
            SoundType::Footstep => 0.05,
            SoundType::Conversation => 0.03,
            SoundType::Gunshot => 0.01,
            SoundType::Explosion => 0.005,
        }
    }
    pub fn base_hear_distance(&self) -> f32 {
        match self {
            SoundType::Footstep => 15.0,
            SoundType::Conversation => 25.0,
            SoundType::Gunshot => 60.0,
            SoundType::Explosion => 120.0,
        }
    }
}

/// 感知刺激.
#[derive(Debug, Clone)]
pub struct Stimulus {
    pub position: [f32; 3],
    pub sense_type: SenseType,
    pub strength: f32,
    pub duration: f32,
    pub source_id: Option<u64>,
    pub sound_type: SoundType,
}

impl Stimulus {
    pub fn vision(position: [f32; 3], strength: f32) -> Self {
        Self {
            position, sense_type: SenseType::Vision, strength,
            duration: 0.0, source_id: None, sound_type: SoundType::default(),
        }
    }
    pub fn hearing(position: [f32; 3], strength: f32, sound_type: SoundType) -> Self {
        Self {
            position, sense_type: SenseType::Hearing, strength,
            duration: 0.0, source_id: None, sound_type,
        }
    }
}

/// 感知结果.
#[derive(Debug, Clone)]
pub struct PerceptionResult {
    pub stimulus: Stimulus,
    pub confidence: f32,
    pub timestamp: f64,
    pub hearing_info: Option<HearingInfo>,
}

/// 听觉感知详细信息.
#[derive(Debug, Clone, Copy)]
pub struct HearingInfo {
    pub itd: f32,
    pub ild: f32,
    pub azimuth: f32,
    pub elevation: f32,
}

/// 感知配置.
#[derive(Debug, Clone)]
pub struct PerceptionConfig {
    pub fov: f32,
    pub max_view_distance: f32,
    pub max_hear_distance: f32,
    pub hear_threshold: f32,
}

impl Default for PerceptionConfig {
    fn default() -> Self {
        Self {
            fov: std::f32::consts::PI * 2.0 / 3.0,
            max_view_distance: 50.0,
            max_hear_distance: 30.0,
            hear_threshold: 0.01,
        }
    }
}

/// 感知组件.
#[derive(Debug, Clone)]
pub struct PerceptionComponent {
    pub config: PerceptionConfig,
    pub position: [f32; 3],
    pub forward: [f32; 3],
    pub vision_sensitivity: f32,
    pub hearing_sensitivity: f32,
    pub enable_fov_edge_falloff: bool,
    pub fov_edge_falloff_range: f32,
    pub enable_multi_point_occlusion: bool,
    pub occlusion_sample_count: u32,
    pub head_width: f32,
    pub speed_of_sound: f32,
    pub enable_hrtf: bool,
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
            fov_edge_falloff_range: 0.2,
            enable_multi_point_occlusion: true,
            occlusion_sample_count: 3,
            head_width: 0.215,
            speed_of_sound: 343.0,
            enable_hrtf: true,
        }
    }
}

/// 感知系统.
pub struct PerceptionSystem<RaycastCb> {
    raycast_cb: RaycastCb,
    results: Vec<PerceptionResult>,
    timestamp: f64,
}

impl<RaycastCb> PerceptionSystem<RaycastCb>
where
    RaycastCb: Fn([f32; 3], [f32; 3]) -> f32,
{
    pub fn new(raycast_cb: RaycastCb) -> Self {
        Self { raycast_cb, results: Vec::new(), timestamp: 0.0 }
    }
    pub fn set_timestamp(&mut self, timestamp: f64) {
        self.timestamp = timestamp;
    }
    pub fn perceive(
        &mut self,
        perceiver: &PerceptionComponent,
        stimuli: &[Stimulus],
    ) -> Vec<PerceptionResult> {
        self.results.clear();
        for stimulus in stimuli {
            if let Some((confidence, hearing_info)) = self.sense_stimulus(perceiver, stimulus) {
                self.results.push(PerceptionResult {
                    stimulus: stimulus.clone(),
                    confidence,
                    timestamp: self.timestamp,
                    hearing_info,
                });
            }
        }
        self.results.clone()
    }

    fn sense_stimulus(
        &self,
        perceiver: &PerceptionComponent,
        stimulus: &Stimulus,
    ) -> Option<(f32, Option<HearingInfo>)> {
        let perp_pos = glam::Vec3::from_array(perceiver.position);
        let stim_pos = glam::Vec3::from_array(stimulus.position);
        let forward = glam::Vec3::from_array(perceiver.forward).normalize();
        match stimulus.sense_type {
            SenseType::Vision => self.sense_vision(perceiver, stimulus, perp_pos, stim_pos, forward)
                .map(|c| (c, None)),
            SenseType::Hearing => self.sense_hearing(perceiver, stimulus, perp_pos, stim_pos)
                .map(|(c, info)| (c, Some(info))),
        }
    }

    fn sense_vision(
        &self,
        perceiver: &PerceptionComponent,
        stimulus: &Stimulus,
        perp_pos: glam::Vec3,
        stim_pos: glam::Vec3,
        forward: glam::Vec3,
    ) -> Option<f32> {
        let dist = (stim_pos - perp_pos).length();
        let max_dist = perceiver.config.max_view_distance
            * stimulus.strength * perceiver.vision_sensitivity;
        if dist > max_dist {
            return None;
        }
        let to_stim = (stim_pos - perp_pos).normalize();
        let dot = forward.dot(to_stim);
        let fov_cos = (perceiver.config.fov * 0.5).cos();
        if dot < fov_cos {
            return None;
        }
        let mut confidence = 1.0;
        if perceiver.enable_fov_edge_falloff {
            let fov_half = perceiver.config.fov * 0.5;
            let angle = dot.acos().clamp(0.0, fov_half);
            let edge_start = fov_half - perceiver.fov_edge_falloff_range;
            if angle > edge_start {
                let falloff = (angle - edge_start) / perceiver.fov_edge_falloff_range;
                confidence *= 1.0 - falloff.clamp(0.0, 1.0) * 0.6;
            }
        }
        let occlusion = (self.raycast_cb)(perceiver.position, stimulus.position);
        if occlusion >= 1.0 {
            return None;
        }
        confidence *= 1.0 - occlusion.clamp(0.0, 1.0) * 0.8;
        let distance_confidence = (1.0 - dist / max_dist).clamp(0.0, 1.0);
        confidence *= distance_confidence;
        confidence *= stimulus.strength;
        Some(confidence.clamp(0.0, 1.0))
    }

    fn sense_hearing(
        &self,
        perceiver: &PerceptionComponent,
        stimulus: &Stimulus,
        perp_pos: glam::Vec3,
        stim_pos: glam::Vec3,
    ) -> Option<(f32, HearingInfo)> {
        let dist = (stim_pos - perp_pos).length();
        let base_dist = stimulus.sound_type.base_hear_distance()
            * stimulus.strength * perceiver.hearing_sensitivity;
        if dist > base_dist {
            return None;
        }
        let attenuation = stimulus.sound_type.attenuation_coefficient();
        let mut intensity = stimulus.strength / (1.0 + dist * dist * attenuation);
        let occlusion = (self.raycast_cb)(perceiver.position, stimulus.position);
        let occlusion_factor = 1.0 - occlusion.clamp(0.0, 1.0) * 0.7;
        intensity *= occlusion_factor;
        let to_stim = (stim_pos - perp_pos).normalize();
        let fwd = glam::Vec3::from_array(perceiver.forward).normalize();
        let right = fwd.cross(glam::Vec3::Y).normalize();
        let fwd_dot = to_stim.dot(fwd);
        let right_dot = to_stim.dot(right);
        let azimuth = right_dot.atan2(fwd_dot);
        let elevation = to_stim.y.asin().clamp(-std::f32::consts::PI / 2.0, std::f32::consts::PI / 2.0);
        let max_itd = perceiver.head_width / perceiver.speed_of_sound;
        let itd = max_itd * right_dot * elevation.cos();
        let itd_normalized = itd.abs().clamp(0.0, 1.0);
        let head_shadow = 1.0 - (right_dot.abs() * 0.6);
        let mut left_intensity = intensity;
        let mut right_intensity = intensity;
        if right_dot > 0.0 {
            left_intensity *= head_shadow;
        } else {
            right_intensity *= head_shadow;
        }
        let ild = ((left_intensity - right_intensity).abs() / intensity.max(0.001)).clamp(0.0, 1.0);
        let elevation_factor = 1.0 - elevation.abs() * 0.3;
        intensity *= elevation_factor;
        let localization_confidence = (itd_normalized * 0.6 + ild * 0.4).clamp(0.0, 1.0);
        let threshold = perceiver.config.hear_threshold;
        if intensity < threshold {
            return None;
        }
        let intensity_confidence = ((intensity - threshold) / (1.0 - threshold)).clamp(0.0, 1.0);
        let confidence = (intensity_confidence * 0.7 + localization_confidence * 0.3).clamp(0.0, 1.0);
        let hearing_info = HearingInfo { itd, ild, azimuth, elevation };
        Some((confidence, hearing_info))
    }
    pub fn results(&self) -> &[PerceptionResult] {
        &self.results
    }
}

/// 导航网格节点.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavNode {
    pub x: i32,
    pub y: i32,
    pub walkable: bool,
}

/// 导航网格三角形.
#[derive(Debug, Clone, Copy)]
pub struct NavTriangle {
    pub a: (i32, i32),
    pub b: (i32, i32),
    pub c: (i32, i32),
    pub center: [f32; 2],
}

/// 导航网格.
#[derive(Debug, Clone)]
pub struct NavGrid {
    width: i32,
    height: i32,
    nodes: Vec<bool>,
    cell_size: f32,
    origin: [f32; 2],
    triangles: Vec<NavTriangle>,
    dirty_regions: Vec<(i32, i32)>,
    dirty_triangles: Vec<usize>,
}

impl NavGrid {
    pub fn new(width: i32, height: i32, cell_size: f32, origin: [f32; 2]) -> Self {
        let mut grid = Self {
            width, height, nodes: vec![true; (width * height) as usize],
            cell_size, origin, triangles: Vec::new(),
            dirty_regions: Vec::new(), dirty_triangles: Vec::new(),
        };
        grid.build_triangulation();
        grid
    }
    pub fn set_obstacle(&mut self, x: i32, y: i32, obstacle: bool) {
        if x >= 0 && x < self.width && y >= 0 && y < self.height {
            let idx = (y * self.width + x) as usize;
            self.nodes[idx] = !obstacle;
            if obstacle {
                self.mark_dirty(x, y);
            }
        }
    }
    fn mark_dirty(&mut self, x: i32, y: i32) {
        if !self.dirty_regions.contains(&(x, y)) {
            self.dirty_regions.push((x, y));
        }
        self.mark_triangles_dirty_around(x, y);
    }
    fn mark_triangles_dirty_around(&mut self, cx: i32, cy: i32) {
        for (i, tri) in self.triangles.iter().enumerate() {
            if tri.a == (cx, cy) || tri.b == (cx, cy) || tri.c == (cx, cy) {
                if !self.dirty_triangles.contains(&i) {
                    self.dirty_triangles.push(i);
                }
            }
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
    pub fn set_rect_obstacle(&mut self, rect_min: [f32; 2], rect_max: [f32; 2], obstacle: bool) {
        let (min_x, min_y) = self.world_to_grid(rect_min[0], rect_min[1]);
        let (max_x, max_y) = self.world_to_grid(rect_max[0], rect_max[1]);
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                self.set_obstacle(x, y, obstacle);
            }
        }
    }
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
    pub fn clear_obstacle(&mut self, x: i32, y: i32) {
        self.set_obstacle(x, y, false);
    }
    pub fn is_walkable(&self, x: i32, y: i32) -> bool {
        if x >= 0 && x < self.width && y >= 0 && y < self.height {
            self.nodes[(y * self.width + x) as usize]
        } else {
            false
        }
    }
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
    pub fn clear_dirty_regions(&mut self) {
        self.dirty_regions.clear();
    }
    pub fn world_to_grid(&self, world_x: f32, world_z: f32) -> (i32, i32) {
        (
            ((world_x - self.origin[0]) / self.cell_size).round() as i32,
            ((world_z - self.origin[1]) / self.cell_size).round() as i32,
        )
    }
    pub fn grid_to_world(&self, x: i32, y: i32) -> [f32; 2] {
        [
            self.origin[0] + x as f32 * self.cell_size,
            self.origin[1] + y as f32 * self.cell_size,
        ]
    }
    pub fn width(&self) -> i32 { self.width }
    pub fn height(&self) -> i32 { self.height }
    pub fn cell_size(&self) -> f32 { self.cell_size }
    fn build_triangulation(&mut self) {
        self.triangles.clear();
        for y in 0..self.height.saturating_sub(1) {
            for x in 0..self.width.saturating_sub(1) {
                if self.is_walkable(x, y) && self.is_walkable(x + 1, y)
                    && self.is_walkable(x, y + 1) && self.is_walkable(x + 1, y + 1)
                {
                    let a = (x, y);
                    let b = (x + 1, y);
                    let c = (x, y + 1);
                    let d = (x + 1, y + 1);
                    let center_a = self.triangle_center(a, b, c);
                    let center_b = self.triangle_center(b, d, c);
                    self.triangles.push(NavTriangle { a, b, c, center: center_a });
                    self.triangles.push(NavTriangle { a: b, b: d, c, center: center_b });
                }
            }
        }
    }
    fn triangle_center(&self, a: (i32, i32), b: (i32, i32), c: (i32, i32)) -> [f32; 2] {
        let wa = self.grid_to_world(a.0, a.1);
        let wb = self.grid_to_world(b.0, b.1);
        let wc = self.grid_to_world(c.0, c.1);
        [(wa[0] + wb[0] + wc[0]) / 3.0, (wa[1] + wb[1] + wc[1]) / 3.0]
    }
    pub fn retriangulate_dirty_region(&mut self) -> usize {
        if self.dirty_triangles.is_empty() {
            return 0;
        }
        let mut min_x = self.width;
        let mut max_x = 0;
        let mut min_y = self.height;
        let mut max_y = 0;
        for &tri_idx in &self.dirty_triangles {
            if let Some(tri) = self.triangles.get(tri_idx) {
                min_x = min_x.min(tri.a.0).min(tri.b.0).min(tri.c.0);
                max_x = max_x.max(tri.a.0).max(tri.b.0).max(tri.c.0);
                min_y = min_y.min(tri.a.1).min(tri.b.1).min(tri.c.1);
                max_y = max_y.max(tri.a.1).max(tri.b.1).max(tri.c.1);
            }
        }
        let min_x = min_x.saturating_sub(1).max(0);
        let max_x = max_x.saturating_add(1).min(self.width.saturating_sub(1));
        let min_y = min_y.saturating_sub(1).max(0);
        let max_y = max_y.saturating_add(1).min(self.height.saturating_sub(1));
        let mut dirty_sorted: Vec<usize> = self.dirty_triangles.clone();
        dirty_sorted.sort_unstable();
        dirty_sorted.dedup();
        for &idx in dirty_sorted.iter().rev() {
            self.triangles.remove(idx);
        }
        for y in min_y..=max_y.saturating_sub(1) {
            for x in min_x..=max_x.saturating_sub(1) {
                if self.is_walkable(x, y) && self.is_walkable(x + 1, y)
                    && self.is_walkable(x, y + 1) && self.is_walkable(x + 1, y + 1)
                {
                    let in_dirty_region = (x >= min_x && x <= max_x && y >= min_y && y <= max_y)
                        || (x + 1 >= min_x && x + 1 <= max_x && y >= min_y && y <= max_y)
                        || (x >= min_x && x <= max_x && y + 1 >= min_y && y + 1 <= max_y)
                        || (x + 1 >= min_x && x + 1 <= max_x && y + 1 >= min_y && y + 1 <= max_y);
                    if !in_dirty_region {
                        continue;
                    }
                    let a = (x, y);
                    let b = (x + 1, y);
                    let c = (x, y + 1);
                    let d = (x + 1, y + 1);
                    let center_a = self.triangle_center(a, b, c);
                    let center_b = self.triangle_center(b, d, c);
                    self.triangles.push(NavTriangle { a, b, c, center: center_a });
                    self.triangles.push(NavTriangle { a: b, b: d, c, center: center_b });
                }
            }
        }
        let count = self.dirty_triangles.len();
        self.dirty_triangles.clear();
        count
    }
    pub fn triangle_count(&self) -> usize { self.triangles.len() }
    pub fn dirty_triangle_count(&self) -> usize { self.dirty_triangles.len() }
    pub fn get_triangle(&self, index: usize) -> Option<&NavTriangle> {
        self.triangles.get(index)
    }
    pub fn triangles(&self) -> impl Iterator<Item = &NavTriangle> {
        self.triangles.iter()
    }
}

/// A* 节点状态.
#[derive(Debug, Clone)]
struct AStarNode {
    x: i32,
    y: i32,
    g: f32,
    h: f32,
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
    pub fn find_path(
        grid: &NavGrid,
        start: (i32, i32),
        goal: (i32, i32),
    ) -> Option<Vec<[f32; 2]>> {
        if !grid.is_walkable(start.0, start.1) || !grid.is_walkable(goal.0, goal.1) {
            return None;
        }
        let mut open_set: Vec<AStarNode> = Vec::new();
        open_set.push(AStarNode {
            x: start.0,
            y: start.1,
            g: 0.0,
            h: Self::heuristic(start, goal),
            parent: None,
        });
        let mut closed_set: Vec<(i32, i32)> = Vec::new();
        let mut nodes: std::collections::HashMap<(i32, i32), AStarNode> =
            std::collections::HashMap::new();
        nodes.insert(start, open_set[0].clone());
        let directions: [(i32, i32); 4] = [(0, 1), (0, -1), (1, 0), (-1, 0)];
        while let Some(best_idx) = Self::pop_lowest_f(&mut open_set) {
            let current = open_set.remove(best_idx);
            let current_pos = (current.x, current.y);
            if current_pos == goal {
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
            closed_set.push(current_pos);
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
        None
    }
    fn pop_lowest_f(open_set: &mut Vec<AStarNode>) -> Option<usize> {
        open_set
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.f().partial_cmp(&b.f()).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
    }
    fn heuristic(a: (i32, i32), b: (i32, i32)) -> f32 {
        (a.0.abs_diff(b.0) + a.1.abs_diff(b.1)) as f32
    }
}

/// 路径.
#[derive(Debug, Clone)]
pub struct Path {
    points: Vec<[f32; 2]>,
    current_index: usize,
}

impl Path {
    pub fn new(points: Vec<[f32; 2]>) -> Self {
        Self { points, current_index: 0 }
    }
    pub fn current_target(&self) -> Option<[f32; 2]> {
        self.points.get(self.current_index).copied()
    }
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
    pub fn is_complete(&self) -> bool {
        self.current_index >= self.points.len()
    }
    pub fn len(&self) -> usize {
        self.points.len()
    }
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}

/// 路径跟随器.
pub struct PathFollower {
    path: Option<Path>,
    speed: f32,
    threshold: f32,
}

impl PathFollower {
    pub fn new(speed: f32) -> Self {
        Self { path: None, speed, threshold: 0.5 }
    }
    pub fn set_path(&mut self, path: Path) {
        self.path = Some(path);
    }
    pub fn clear_path(&mut self) {
        self.path = None;
    }
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
        let _step = self.speed * dt;
        Some(dir)
    }
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

impl Default for PathFollower {
    fn default() -> Self {
        Self::new(2.0)
    }
}

/// Crowd 代理状态.
#[derive(Debug, Clone, Copy)]
pub struct CrowdAgentState {
    pub pos: [f32; 2],
    pub vel: [f32; 2],
    pub radius: f32,
    pub max_speed: f32,
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
    pub fn set_pref_vel(&mut self, pref_vel: [f32; 2]) {
        self.state.pref_vel = pref_vel;
    }
    pub fn apply_velocity(&mut self, new_vel: [f32; 2]) {
        self.state.vel = new_vel;
        let speed = (new_vel[0].powi(2) + new_vel[1].powi(2)).sqrt();
        if speed > self.state.max_speed {
            let scale = self.state.max_speed / speed;
            self.state.vel = [new_vel[0] * scale, new_vel[1] * scale];
        }
    }
    pub fn integrate(&mut self, dt: f32) {
        self.state.pos[0] += self.state.vel[0] * dt;
        self.state.pos[1] += self.state.vel[1] * dt;
    }
}

/// Crowd 管理器.
pub struct CrowdManager {
    agents: Vec<CrowdAgent>,
    pub dt: f32,
    pub neighbor_radius: f32,
    pub avoidance_weight: f32,
}

impl CrowdManager {
    pub fn new() -> Self {
        Self {
            agents: Vec::new(),
            dt: 0.016,
            neighbor_radius: 5.0,
            avoidance_weight: 0.5,
        }
    }
    pub fn add_agent(&mut self, agent: CrowdAgent) -> usize {
        let id = self.agents.len();
        self.agents.push(agent);
        id
    }
    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }
    pub fn get_agent(&self, id: usize) -> Option<&CrowdAgent> {
        self.agents.get(id)
    }
    pub fn get_agent_mut(&mut self, id: usize) -> Option<&mut CrowdAgent> {
        self.agents.get_mut(id)
    }
    pub fn update(&mut self) {
        let n = self.agents.len();
        if n == 0 {
            return;
        }
        let mut new_vels: Vec<[f32; 2]> = vec![[0.0, 0.0]; n];
        for i in 0..n {
            let agent_i = &self.agents[i];
            let pos_i = agent_i.state.pos;
            let vel_i = agent_i.state.vel;
            let pref_vel = agent_i.state.pref_vel;
            let radius_i = agent_i.state.radius;
            let mut constraints: Vec<([f32; 2], f32)> = Vec::new();
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
                if dist_sq < self.neighbor_radius * self.neighbor_radius && dist_sq > 0.0001 {
                    let dist = dist_sq.sqrt();
                    if dist < min_dist {
                        let overlap = min_dist - dist;
                        let inv_dist = 1.0 / dist;
                        let repel_x = dx * inv_dist * overlap;
                        let repel_z = dz * inv_dist * overlap;
                        constraints.push(([repel_x, repel_z], 0.0));
                    } else {
                        let future_dx = dx + (vel_j[0] - vel_i[0]) * self.dt;
                        let future_dz = dz + (vel_j[1] - vel_i[1]) * self.dt;
                        let future_dist_sq = future_dx * future_dx + future_dz * future_dz;
                        if future_dist_sq < min_dist * min_dist {
                            let n_x = dx / dist;
                            let n_z = dz / dist;
                            let rel_vel_x = vel_i[0] - vel_j[0];
                            let rel_vel_z = vel_i[1] - vel_j[1];
                            let a = rel_vel_x * rel_vel_x + rel_vel_z * rel_vel_z;
                            let b = 2.0 * (dx * rel_vel_x + dz * rel_vel_z);
                            let c = dist_sq - min_dist * min_dist;
                            let t_collision = if a > 0.0001 {
                                let disc = b * b - 4.0 * a * c;
                                if disc > 0.0 {
                                    let t = (-b - disc.sqrt()) / (2.0 * a);
                                    if t > 0.0 && t <= self.dt {
                                        t
                                    } else {
                                        self.dt
                                    }
                                } else {
                                    self.dt
                                }
                            } else {
                                self.dt
                            };
                            let needed_separation = (min_dist - dist) / t_collision.max(0.001);
                            let offset = rel_vel_x * n_x + rel_vel_z * n_z + needed_separation;
                            constraints.push(([n_x, n_z], offset));
                        }
                    }
                }
            }
            let optimal_vel = if constraints.is_empty() {
                pref_vel
            } else {
                self.solve_2d_lp(pref_vel, &constraints)
            };
            new_vels[i] = optimal_vel;
        }
        for i in 0..n {
            let agent = self.agents.get_mut(i).unwrap();
            agent.apply_velocity(new_vels[i]);
            agent.integrate(self.dt);
        }
    }
    fn solve_2d_lp(&self, pref_vel: [f32; 2], constraints: &[([f32; 2], f32)]) -> [f32; 2] {
        let mut satisfies_all = true;
        for &(normal, offset) in constraints {
            let dot = normal[0] * pref_vel[0] + normal[1] * pref_vel[1];
            if dot < offset - 1e-6 {
                satisfies_all = false;
                break;
            }
        }
        if satisfies_all {
            return pref_vel;
        }
        let mut best_vel = pref_vel;
        let mut best_dist_sq = f32::INFINITY;
        for i in 0..constraints.len() {
            let (normal_i, offset_i) = constraints[i];
            let dot = normal_i[0] * pref_vel[0] + normal_i[1] * pref_vel[1];
            let t = offset_i - dot;
            let candidate = [
                pref_vel[0] + t * normal_i[0],
                pref_vel[1] + t * normal_i[1],
            ];
            let mut feasible = true;
            for j in 0..constraints.len() {
                if i == j {
                    continue;
                }
                let (normal_j, offset_j) = constraints[j];
                let dot_j = normal_j[0] * candidate[0] + normal_j[1] * candidate[1];
                if dot_j < offset_j - 1e-6 {
                    feasible = false;
                    break;
                }
            }
            if feasible {
                let dx = candidate[0] - pref_vel[0];
                let dz = candidate[1] - pref_vel[1];
                let dist_sq = dx * dx + dz * dz;
                if dist_sq < best_dist_sq {
                    best_dist_sq = dist_sq;
                    best_vel = candidate;
                }
            }
        }
        best_vel
    }
    pub fn set_dt(&mut self, dt: f32) {
        self.dt = dt;
    }
    pub fn set_neighbor_radius(&mut self, radius: f32) {
        self.neighbor_radius = radius;
    }
    pub fn set_avoidance_weight(&mut self, weight: f32) {
        self.avoidance_weight = weight.clamp(0.0, 1.0);
    }
    pub fn positions(&self) -> Vec<[f32; 2]> {
        self.agents.iter().map(|a| a.state.pos).collect()
    }
}

impl Default for CrowdManager {
    fn default() -> Self {
        Self::new()
    }
}

/// GOAP 状态：黑板中键值对的集合，表示世界状态.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GoapState {
    values: HashMap<String, bool>,
}

impl GoapState {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set(&mut self, key: &str, value: bool) {
        self.values.insert(key.to_string(), value);
    }
    pub fn get(&self, key: &str) -> Option<bool> {
        self.values.get(key).copied()
    }
    pub fn satisfies(&self, conditions: &[(String, bool)]) -> bool {
        conditions.iter().all(|(k, v)| self.values.get(k) == Some(v))
    }
    pub fn iter(&self) -> impl Iterator<Item = (&String, &bool)> {
        self.values.iter()
    }
}

/// GOAP 动作.
pub struct GoapAction {
    pub name: String,
    pub preconditions: Vec<(String, bool)>,
    pub effects: Vec<(String, bool)>,
    pub cost: f32,
    pub is_available: Box<dyn Fn(&GoapState) -> bool + Send + Sync>,
    pub execute: Box<dyn Fn(&GoapState) -> GoapState + Send + Sync>,
}

impl GoapAction {
    pub fn new(
        name: &str,
        preconditions: Vec<(String, bool)>,
        effects: Vec<(String, bool)>,
        cost: f32,
        is_available: impl Fn(&GoapState) -> bool + Send + Sync + 'static,
        execute: impl Fn(&GoapState) -> GoapState + Send + Sync + 'static,
    ) -> Self {
        Self {
            name: name.to_string(),
            preconditions,
            effects,
            cost,
            is_available: Box::new(is_available),
            execute: Box::new(execute),
        }
    }
    pub fn can_execute(&self, state: &GoapState) -> bool {
        (self.is_available)(state) && state.satisfies(&self.preconditions)
    }
    pub fn apply(&self, state: &GoapState) -> GoapState {
        (self.execute)(state)
    }
}

/// GOAP 规划器.
pub struct GoapPlanner {
    actions: Vec<GoapAction>,
}

impl GoapPlanner {
    pub fn new() -> Self {
        Self { actions: Vec::new() }
    }
    pub fn add_action(&mut self, action: GoapAction) {
        self.actions.push(action);
    }
    pub fn plan(
        &self,
        current_state: &GoapState,
        goal: &[(String, bool)],
    ) -> Option<Vec<String>> {
        #[derive(Clone)]
        struct Node {
            state: GoapState,
            g_cost: f32,
            actions: Vec<String>,
        }
        let mut open: Vec<Node> = Vec::new();
        let mut closed: Vec<GoapState> = Vec::new();
        open.push(Node {
            state: current_state.clone(),
            g_cost: 0.0,
            actions: Vec::new(),
        });
        while !open.is_empty() {
            let best_idx = open
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    let ha = self.heuristic(&a.state, goal);
                    let hb = self.heuristic(&b.state, goal);
                    (a.g_cost + ha)
                        .partial_cmp(&(b.g_cost + hb))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(i, _)| i)
                .unwrap();
            let current = open.remove(best_idx);
            if current.state.satisfies(goal) {
                return Some(current.actions);
            }
            if closed.iter().any(|s| s.values == current.state.values) {
                continue;
            }
            closed.push(current.state.clone());
            for action in &self.actions {
                if !action.can_execute(&current.state) {
                    continue;
                }
                let new_state = action.apply(&current.state);
                if closed.iter().any(|s| s.values == new_state.values) {
                    continue;
                }
                open.push(Node {
                    state: new_state,
                    g_cost: current.g_cost + action.cost,
                    actions: {
                        let mut acts = current.actions.clone();
                        acts.push(action.name.clone());
                        acts
                    },
                });
            }
        }
        None
    }
    fn heuristic(&self, state: &GoapState, goal: &[(String, bool)]) -> f32 {
        goal.iter()
            .filter(|(k, v)| state.get(k) != Some(*v))
            .count() as f32
    }
}

/// GOAP 代理.
pub struct GoapAgent {
    pub state: GoapState,
    pub planner: GoapPlanner,
    pub current_goal: Option<Vec<(String, bool)>>,
    pub current_plan: Vec<String>,
    pub current_action_idx: usize,
}

impl GoapAgent {
    pub fn new() -> Self {
        Self {
            state: GoapState::new(),
            planner: GoapPlanner::new(),
            current_goal: None,
            current_plan: Vec::new(),
            current_action_idx: 0,
        }
    }
    pub fn set_state(&mut self, state: GoapState) {
        self.state = state;
    }
    pub fn set_goal(&mut self, goal: Vec<(String, bool)>) {
        self.current_goal = Some(goal);
        self.current_plan = Vec::new();
        self.current_action_idx = 0;
    }
    pub fn tick(&mut self) -> Option<&str> {
        if self.current_plan.is_empty() || self.current_action_idx >= self.current_plan.len() {
            if let Some(goal) = &self.current_goal {
                self.current_plan = self.planner.plan(&self.state, goal)?;
                self.current_action_idx = 0;
            } else {
                return None;
            }
        }
        self.current_plan.get(self.current_action_idx).map(|a| a.as_str())
    }
    pub fn complete_current_action(&mut self) {
        self.current_action_idx += 1;
    }
    pub fn state(&self) -> &GoapState {
        &self.state
    }
    pub fn state_mut(&mut self) -> &mut GoapState {
        &mut self.state
    }
}

impl Default for GoapAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod goap_tests {
    use super::*;
    #[test]
    fn goap_simple_plan() {
        let mut planner = GoapPlanner::new();
        planner.add_action(GoapAction::new(
            "pickup_item",
            vec![("has_item".to_string(), false)],
            vec![("has_item".to_string(), true)],
            1.0,
            |_| true,
            |state| {
                let mut new = state.clone();
                new.set("has_item", true);
                new
            },
        ));
        planner.add_action(GoapAction::new(
            "use_item",
            vec![("has_item".to_string(), true)],
            vec![("item_used".to_string(), true)],
            1.0,
            |_| true,
            |state| {
                let mut new = state.clone();
                new.set("item_used", true);
                new
            },
        ));
        let mut state = GoapState::new();
        state.set("has_item", false);
        state.set("item_used", false);
        let goal = vec![("item_used".to_string(), true)];
        let plan = planner.plan(&state, &goal);
        assert!(plan.is_some());
        let plan = plan.unwrap();
        assert_eq!(plan, vec!["pickup_item", "use_item"]);
    }
    #[test]
    fn goap_no_plan_when_impossible() {
        let mut planner = GoapPlanner::new();
        planner.add_action(GoapAction::new(
            "pickup_item",
            vec![("has_item".to_string(), false)],
            vec![("has_item".to_string(), true)],
            1.0,
            |_| true,
            |state| {
                let mut new = state.clone();
                new.set("has_item", true);
                new
            },
        ));
        let mut state = GoapState::new();
        state.set("has_item", false);
        let goal = vec![("item_used".to_string(), true)];
        let plan = planner.plan(&state, &goal);
        assert!(plan.is_none());
    }
    #[test]
    fn goap_agent_tick() {
        let mut agent = GoapAgent::new();
        let mut planner = GoapPlanner::new();
        planner.add_action(GoapAction::new(
            "pickup_item",
            vec![("has_item".to_string(), false)],
            vec![("has_item".to_string(), true)],
            1.0,
            |_| true,
            |state| {
                let mut new = state.clone();
                new.set("has_item", true);
                new
            },
        ));
        agent.planner = planner;
        agent.state_mut().set("has_item", false);
        agent.set_goal(vec![("has_item".to_string(), true)]);
        let action = agent.tick();
        assert_eq!(action, Some("pickup_item"));
        agent.state_mut().set("has_item", true);
        agent.complete_current_action();
        let action = agent.tick();
        assert_eq!(action, None);
    }
    #[test]
    fn goap_action_availability() {
        let mut planner = GoapPlanner::new();
        planner.add_action(GoapAction::new(
            "use_weapon",
            vec![("has_weapon".to_string(), true)],
            vec![("enemy_dead".to_string(), true)],
            1.0,
            |state| state.get("has_weapon").unwrap_or(false),
            |state| {
                let mut new = state.clone();
                new.set("enemy_dead", true);
                new
            },
        ));
        let mut state = GoapState::new();
        state.set("has_weapon", false);
        let plan = planner.plan(&state, &vec![("enemy_dead".to_string(), true)]);
        assert!(plan.is_none());
        state.set("has_weapon", true);
        let plan = planner.plan(&state, &vec![("enemy_dead".to_string(), true)]);
        assert!(plan.is_some());
    }
}
