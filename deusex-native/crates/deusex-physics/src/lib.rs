//! deusex-physics - 简单的 3D 刚体物理引擎.
//!
//! 提供：
//! - `PhysicsWorld`: 物理世界，管理所有刚体并进行步进模拟
//! - `RigidBody`: 刚体（位置、速度、质量、惯性、碰撞体）
//! - `Collider`: 碰撞体形状（球体、AABB、平面）
//! - `Collision`: 碰撞检测结果（法线、穿深、接触点）
//! - `CharacterController`: 角色控制器（用于玩家移动、跳跃、碰撞滑动）
//!
//! 使用 `glam` 作为 3D 数学库。当前实现基于欧拉积分和冲量碰撞响应，
//! 适合 Deus Ex 这类第三人称/第一人称游戏的基础物理需求。

use std::collections::HashMap;

use glam::Vec3;
use tracing::{debug, info};

/// 刚体唯一标识符。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BodyId(usize);

impl BodyId {
    pub fn null() -> Self {
        Self(usize::MAX)
    }
    pub fn is_null(&self) -> bool {
        self.0 == usize::MAX
    }
}

/// 碰撞体形状。
#[derive(Debug, Clone, Copy)]
pub enum ColliderShape {
    /// 球体碰撞体。
    Sphere { radius: f32 },
    /// 轴对齐包围盒（AABB）。
    Aabb { min: Vec3, max: Vec3 },
    /// 无限平面（用于地面、墙壁）。
    Plane { normal: Vec3, distance: f32 },
}

/// 碰撞检测结果。
#[derive(Debug, Clone, Copy)]
pub struct Collision {
    /// 参与碰撞的两个刚体。
    pub body_a: BodyId,
    pub body_b: BodyId,
    /// 从 body_b 指向 body_a 的碰撞法线（单位向量）。
    pub normal: Vec3,
    /// 穿深（两体重叠深度，单位：世界单位）。
    pub penetration: f32,
    /// 接触点（世界空间）。
    pub point: Vec3,
}

/// 物理约束类型。
#[derive(Debug, Clone, Copy)]
pub enum ConstraintType {
    /// 弹簧约束：两个刚体之间的胡克定律弹簧连接。
    /// `stiffness` 弹簧刚度，`damping` 阻尼系数，`rest_length` 自然长度。
    Spring { stiffness: f32, damping: f32, rest_length: f32 },
    /// 距离约束：两个锚点之间的固定距离（不可伸缩的杆）。
    /// `distance` 目标距离。
    Distance { distance: f32 },
    /// 点对点关节（铰链）：两个刚体的指定点保持重合。
    /// `anchor_a` / `anchor_b` 是各自刚体局部空间的锚点偏移。
    PointToPoint { anchor_a: Vec3, anchor_b: Vec3 },
}

/// 物理约束实例，连接两个刚体。
#[derive(Debug, Clone, Copy)]
pub struct Constraint {
    pub id: ConstraintId,
    pub body_a: BodyId,
    pub body_b: BodyId,
    pub ctype: ConstraintType,
    /// 是否启用。
    pub enabled: bool,
}

/// 物理约束唯一标识符。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConstraintId(usize);

impl ConstraintId {
    pub fn null() -> Self {
        Self(usize::MAX)
    }
    pub fn is_null(&self) -> bool {
        self.0 == usize::MAX
    }
}

/// 射线检测结果。
#[derive(Debug, Clone, Copy)]
pub struct RaycastHit {
    /// 被击中的刚体 ID。
    pub body_id: BodyId,
    /// 射线到击中点的距离。
    pub distance: f32,
    /// 击中点的世界空间位置。
    pub point: Vec3,
    /// 击中面的法线（单位向量）。
    pub normal: Vec3,
}

/// 刚体物理对象。
#[derive(Debug, Clone)]
pub struct RigidBody {
    pub id: BodyId,
    /// 世界空间位置。
    pub position: Vec3,
    /// 线速度（单位/秒）。
    pub velocity: Vec3,
    /// 合力/加速度（每帧累加，步进后清零）。
    pub force_accum: Vec3,
    /// 质量（kg）。
    pub mass: f32,
    /// 质量的倒数（静态物体为 0）。
    pub inverse_mass: f32,
    /// 弹性系数（0 = 完全非弹性，1 = 完全弹性）。
    pub restitution: f32,
    /// 摩擦系数（0 = 无摩擦，1 = 高摩擦）。
    pub friction: f32,
    /// 碰撞体形状。
    pub collider: ColliderShape,
    /// 是否为静态物体（不受力影响，不参与积分）。
    pub is_static: bool,
    /// 是否启用（禁用的刚体不参与碰撞检测和积分）。
    pub enabled: bool,
}

impl RigidBody {
    /// 创建一个动态球体刚体。
    pub fn sphere(
        id: BodyId,
        position: Vec3,
        radius: f32,
        mass: f32,
    ) -> Self {
        let inverse_mass = if mass > 0.0 { 1.0 / mass } else { 0.0 };
        Self {
            id,
            position,
            velocity: Vec3::ZERO,
            force_accum: Vec3::ZERO,
            mass,
            inverse_mass,
            restitution: 0.5,
            friction: 0.3,
            collider: ColliderShape::Sphere { radius },
            is_static: mass <= 0.0,
            enabled: true,
        }
    }

    /// 创建一个动态 AABB 刚体。
    pub fn aabb(
        id: BodyId,
        center: Vec3,
        half_extents: Vec3,
        mass: f32,
    ) -> Self {
        let inverse_mass = if mass > 0.0 { 1.0 / mass } else { 0.0 };
        Self {
            id,
            position: center,
            velocity: Vec3::ZERO,
            force_accum: Vec3::ZERO,
            mass,
            inverse_mass,
            restitution: 0.5,
            friction: 0.3,
            collider: ColliderShape::Aabb {
                min: center - half_extents,
                max: center + half_extents,
            },
            is_static: mass <= 0.0,
            enabled: true,
        }
    }

    /// 创建一个静态平面（地面/墙壁）。
    pub fn plane(
        id: BodyId,
        normal: Vec3,
        distance: f32,
    ) -> Self {
        Self {
            id,
            position: normal * distance,
            velocity: Vec3::ZERO,
            force_accum: Vec3::ZERO,
            mass: 0.0,
            inverse_mass: 0.0,
            restitution: 0.5,
            friction: 0.3,
            collider: ColliderShape::Plane { normal, distance },
            is_static: true,
            enabled: true,
        }
    }

    /// 返回碰撞体的世界空间 AABB（用于广阶段剔除）。
    pub fn world_aabb(&self) -> (Vec3, Vec3) {
        match self.collider {
            ColliderShape::Sphere { radius } => {
                (self.position - Vec3::splat(radius), self.position + Vec3::splat(radius))
            }
            ColliderShape::Aabb { min, max } => (min, max),
            ColliderShape::Plane { .. } => {
                // 平面无穷大，返回一个足够大的盒子。
                let big = Vec3::splat(1e6);
                (self.position - big, self.position + big)
            }
        }
    }

    /// 应用一个力（世界空间）。
    pub fn apply_force(&mut self, force: Vec3) {
        if self.is_static || !self.enabled {
            return;
        }
        self.force_accum += force;
    }

    /// 应用一个冲量（改变线速度）。
    pub fn apply_impulse(&mut self, impulse: Vec3) {
        if self.is_static || !self.enabled {
            return;
        }
        self.velocity += impulse * self.inverse_mass;
    }

    /// 设置线速度。
    pub fn set_velocity(&mut self, velocity: Vec3) {
        self.velocity = velocity;
    }

    /// 返回是否为静态物体。
    pub fn is_static(&self) -> bool {
        self.is_static
    }
}

/// 碰撞检测结果集（单帧所有碰撞对）。
#[derive(Debug, Default, Clone)]
pub struct CollisionManifold {
    pub collisions: Vec<Collision>,
}

/// 物理世界配置。
#[derive(Debug, Clone)]
pub struct PhysicsConfig {
    /// 重力加速度（默认 -9.81 Y 轴）。
    pub gravity: Vec3,
    /// 固定时间步长（秒，默认 1/60）。
    pub fixed_dt: f32,
    /// 子步数（用于提高碰撞响应稳定性）。
    pub sub_steps: u32,
    /// 最大速度剪裁（防止隧道效应）。
    pub max_velocity: f32,
    /// 休眠速度阈值（低于此速度的物体可进入休眠）。
    pub sleep_threshold: f32,
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            gravity: Vec3::new(0.0, -9.81, 0.0),
            fixed_dt: 1.0 / 60.0,
            sub_steps: 4,
            max_velocity: 100.0,
            sleep_threshold: 0.01,
        }
    }
}

/// 物理世界：管理所有刚体，执行碰撞检测与响应，进行步进模拟。
pub struct PhysicsWorld {
    config: PhysicsConfig,
    bodies: HashMap<BodyId, RigidBody>,
    constraints: Vec<Constraint>,
    next_body_id: usize,
    next_constraint_id: usize,
    /// 上一帧的碰撞结果（供角色控制器等使用）。
    last_manifold: CollisionManifold,
}

impl PhysicsWorld {
    /// 创建新物理世界。
    pub fn new(config: PhysicsConfig) -> Self {
        info!(
            "physics: created world (gravity={:?}, dt={}, sub_steps={})",
            config.gravity, config.fixed_dt, config.sub_steps
        );
        Self {
            config,
            bodies: HashMap::new(),
            constraints: Vec::new(),
            next_body_id: 0,
            next_constraint_id: 0,
            last_manifold: CollisionManifold::default(),
        }
    }

    /// 使用默认配置创建物理世界。
    pub fn default() -> Self {
        Self::new(PhysicsConfig::default())
    }

    /// 添加一个刚体到世界，返回其 `BodyId`。
    pub fn add_body(&mut self, body: RigidBody) -> BodyId {
        let id = if body.id.is_null() {
            let id = BodyId(self.next_body_id);
            self.next_body_id += 1;
            id
        } else {
            body.id
        };
        self.bodies.insert(id, body);
        debug!("physics: added body {:?}", id);
        id
    }

    /// 添加一个物理约束到世界，返回其 `ConstraintId`。
    pub fn add_constraint(&mut self, constraint: Constraint) -> ConstraintId {
        let id = if constraint.id.is_null() {
            let id = ConstraintId(self.next_constraint_id);
            self.next_constraint_id += 1;
            id
        } else {
            constraint.id
        };
        self.constraints.push(Constraint { id, ..constraint });
        debug!("physics: added constraint {:?}", id);
        id
    }

    /// 移除一个约束。
    pub fn remove_constraint(&mut self, id: ConstraintId) -> Option<Constraint> {
        self.constraints.iter().position(|c| c.id == id).and_then(|i| {
            Some(self.constraints.remove(i))
        })
    }

    /// 返回约束数量。
    pub fn constraint_count(&self) -> usize {
        self.constraints.len()
    }

    /// 射线检测：从 `origin` 沿 `direction` 发射射线，返回最近命中的刚体。
    ///
    /// `max_distance` 限制射线最大长度。返回 `None` 表示未击中任何物体。
    pub fn raycast(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
    ) -> Option<RaycastHit> {
        let dir = direction.normalize();
        let mut closest_hit: Option<RaycastHit> = None;
        let mut closest_dist = max_distance;

        for (body_id, body) in &self.bodies {
            if !body.enabled {
                continue;
            }

            let hit = match body.collider {
                ColliderShape::Sphere { radius } => {
                    Self::raycast_sphere(origin, dir, body.position, radius)
                }
                ColliderShape::Aabb { min, max } => {
                    Self::raycast_aabb(origin, dir, min, max)
                }
                ColliderShape::Plane { normal, distance } => {
                    Self::raycast_plane(origin, dir, normal, distance)
                }
            };

            if let Some(hit_dist) = hit {
                if hit_dist < closest_dist {
                    let point = origin + dir * hit_dist;
                    let normal = match body.collider {
                        ColliderShape::Sphere { .. } => (point - body.position).normalize(),
                        ColliderShape::Aabb { .. } => {
                            // 简单返回射线方向的反向作为近似法线
                            -dir
                        }
                        ColliderShape::Plane { normal, .. } => normal,
                    };
                    closest_dist = hit_dist;
                    closest_hit = Some(RaycastHit {
                        body_id: *body_id,
                        distance: hit_dist,
                        point,
                        normal,
                    });
                }
            }
        }

        closest_hit
    }

    /// 射线-球体交点测试。
    fn raycast_sphere(
        origin: Vec3,
        dir: Vec3,
        center: Vec3,
        radius: f32,
    ) -> Option<f32> {
        let m = origin - center;
        let b = m.dot(dir);
        let c = m.dot(m) - radius * radius;

        if c > 0.0 && b > 0.0 {
            return None;
        }

        let discriminant = b * b - c;
        if discriminant < 0.0 {
            return None;
        }

        let t = -b - discriminant.sqrt();
        if t < 0.0 {
            return None;
        }

        Some(t)
    }

    /// 射线-AABB 交点测试（slab 方法）。
    fn raycast_aabb(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
        let mut tmin: f32 = 0.0;
        let mut tmax: f32 = f32::INFINITY;

        for i in 0..3 {
            let inv_d = if dir[i] != 0.0 { 1.0 / dir[i] } else { f32::INFINITY };
            let mut t0 = (min[i] - origin[i]) * inv_d;
            let mut t1 = (max[i] - origin[i]) * inv_d;
            if inv_d < 0.0 {
                std::mem::swap(&mut t0, &mut t1);
            }
            tmin = tmin.max(t0);
            tmax = tmax.min(t1);
            if tmax < tmin {
                return None;
            }
        }

        if tmin < 0.0 {
            if tmax < 0.0 {
                return None;
            }
            Some(tmax)
        } else {
            Some(tmin)
        }
    }

    /// 射线-平面交点测试。
    fn raycast_plane(
        origin: Vec3,
        dir: Vec3,
        plane_normal: Vec3,
        plane_distance: f32,
    ) -> Option<f32> {
        let denom = plane_normal.dot(dir);
        if denom.abs() < 1e-6 {
            return None;
        }

        let t = (plane_distance - plane_normal.dot(origin)) / denom;
        if t < 0.0 {
            return None;
        }

        Some(t)
    }

    /// 移除一个刚体。
    pub fn remove_body(&mut self, id: BodyId) -> Option<RigidBody> {
        self.bodies.remove(&id)
    }

    /// 获取刚体的不可变引用。
    pub fn body(&self, id: BodyId) -> Option<&RigidBody> {
        self.bodies.get(&id)
    }

    /// 获取刚体的可变引用。
    pub fn body_mut(&mut self, id: BodyId) -> Option<&mut RigidBody> {
        self.bodies.get_mut(&id)
    }

    /// 返回所有刚体的引用迭代器。
    pub fn bodies(&self) -> impl Iterator<Item = (&BodyId, &RigidBody)> {
        self.bodies.iter()
    }

    /// 返回刚体数量。
    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    /// 设置重力。
    pub fn set_gravity(&mut self, gravity: Vec3) {
        self.config.gravity = gravity;
    }

    /// 返回当前重力。
    pub fn gravity(&self) -> Vec3 {
        self.config.gravity
    }

    /// 执行一步物理模拟（固定步长）。
    ///
    /// 通常在游戏主循环的固定 tick 中调用。
    pub fn step(&mut self) {
        let dt = self.config.fixed_dt / self.config.sub_steps as f32;

        for _ in 0..self.config.sub_steps {
            // 1. 应用重力并积分速度/位置。
            self.integrate(dt);

            // 2. 碰撞检测（广阶段 + 窄阶段）。
            let manifold = self.detect_collisions();

            // 3. 碰撞响应（冲量解算 + 位置校正）。
            self.resolve_collisions(&manifold, dt);

            // 4. 约束求解（弹簧、距离、点对点关节等）。
            self.resolve_constraints(dt);

            // 5. 保存本帧碰撞结果（供角色控制器等使用）。
            self.last_manifold = manifold;
        }
    }

    /// 返回上一帧的碰撞流形。
    pub fn last_manifold(&self) -> &CollisionManifold {
        &self.last_manifold
    }

    /// 集成：应用重力、力累加器，更新速度和位置。
    fn integrate(&mut self, dt: f32) {
        let gravity = self.config.gravity;
        let _inv_dt = 1.0 / dt.max(1e-6);

        for body in self.bodies.values_mut() {
            if body.is_static || !body.enabled {
                continue;
            }

            // 应用重力。
            body.velocity += gravity * dt;

            // 应用累加的力（F = ma -> a = F / m）。
            body.velocity += body.force_accum * body.inverse_mass * dt;

            // 速度剪裁（防止隧道效应）。
            let speed = body.velocity.length();
            if speed > self.config.max_velocity {
                body.velocity = body.velocity.normalize() * self.config.max_velocity;
            }

            // 欧拉积分位置。
            body.position += body.velocity * dt;

            // 清空力累加器。
            body.force_accum = Vec3::ZERO;
        }
    }

    /// 碰撞检测：返回所有重叠的刚体对。
    fn detect_collisions(&self) -> CollisionManifold {
        let mut manifold = CollisionManifold::default();
        let ids: Vec<BodyId> = self.bodies.keys().copied().collect();

        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                let a_id = ids[i];
                let b_id = ids[j];
                let a = match self.bodies.get(&a_id) {
                    Some(b) if b.enabled => b,
                    _ => continue,
                };
                let b = match self.bodies.get(&b_id) {
                    Some(b) if b.enabled => b,
                    _ => continue,
                };

                // 广阶段：AABB 剔除。
                let (a_min, a_max) = a.world_aabb();
                let (b_min, b_max) = b.world_aabb();
                if a_max.x < b_min.x || a_min.x > b_max.x
                    || a_max.y < b_min.y || a_min.y > b_max.y
                    || a_max.z < b_min.z || a_min.z > b_max.z
                {
                    continue;
                }

                // 窄阶段：具体形状碰撞检测。
                if let Some(collision) = Self::narrow_phase(a, b) {
                    manifold.collisions.push(collision);
                }
            }
        }

        manifold
    }

    /// 窄阶段碰撞检测。
    fn narrow_phase(a: &RigidBody, b: &RigidBody) -> Option<Collision> {
        match (&a.collider, &b.collider) {
            (ColliderShape::Sphere { radius: ra }, ColliderShape::Sphere { radius: rb }) => {
                Self::sphere_sphere(a, b, *ra, *rb)
            }
            (ColliderShape::Aabb { min: a_min, max: a_max }, ColliderShape::Aabb { min: b_min, max: b_max }) => {
                Self::aabb_aabb(a, b, *a_min, *a_max, *b_min, *b_max)
            }
            (ColliderShape::Sphere { radius }, ColliderShape::Aabb { min, max }) => {
                Self::sphere_aabb(a, b, *radius, *min, *max)
            }
            (ColliderShape::Aabb { min, max }, ColliderShape::Sphere { radius }) => {
                let c = Self::sphere_aabb(b, a, *radius, *min, *max)?;
                Some(Collision {
                    body_a: c.body_b,
                    body_b: c.body_a,
                    normal: -c.normal,
                    penetration: c.penetration,
                    point: c.point,
                })
            }
            (ColliderShape::Sphere { radius }, ColliderShape::Plane { normal, distance }) => {
                Self::sphere_plane(a, b, *radius, *normal, *distance)
            }
            (ColliderShape::Plane { normal, distance }, ColliderShape::Sphere { radius }) => {
                let c = Self::sphere_plane(b, a, *radius, *normal, *distance)?;
                Some(Collision {
                    body_a: c.body_b,
                    body_b: c.body_a,
                    normal: -c.normal,
                    penetration: c.penetration,
                    point: c.point,
                })
            }
            (ColliderShape::Aabb { .. }, ColliderShape::Plane { normal, distance }) => {
                Self::aabb_plane(a, b, *normal, *distance)
            }
            (ColliderShape::Plane { normal, distance }, ColliderShape::Aabb { .. }) => {
                let c = Self::aabb_plane(b, a, *normal, *distance)?;
                Some(Collision {
                    body_a: c.body_b,
                    body_b: c.body_a,
                    normal: -c.normal,
                    penetration: c.penetration,
                    point: c.point,
                })
            }
            (ColliderShape::Plane { .. }, ColliderShape::Plane { .. }) => None,
        }
    }

    /// 球体-球体碰撞。
    fn sphere_sphere(
        a: &RigidBody,
        b: &RigidBody,
        ra: f32,
        rb: f32,
    ) -> Option<Collision> {
        let delta = a.position - b.position;
        let dist_sq = delta.length_squared();
        let radius_sum = ra + rb;
        if dist_sq >= radius_sum * radius_sum || dist_sq < 1e-12 {
            return None;
        }
        let dist = dist_sq.sqrt();
        let normal = delta / dist;
        let penetration = radius_sum - dist;
        let point = b.position + normal * rb;
        Some(Collision {
            body_a: a.id,
            body_b: b.id,
            normal,
            penetration,
            point,
        })
    }

    /// AABB-AABB 碰撞（使用最小平移向量）。
    fn aabb_aabb(
        a: &RigidBody,
        b: &RigidBody,
        a_min: Vec3,
        a_max: Vec3,
        b_min: Vec3,
        b_max: Vec3,
    ) -> Option<Collision> {
        let overlap_x = a_max.x.min(b_max.x) - a_min.x.max(b_min.x);
        let overlap_y = a_max.y.min(b_max.y) - a_min.y.max(b_min.y);
        let overlap_z = a_max.z.min(b_max.z) - a_min.z.max(b_min.z);

        if overlap_x <= 0.0 || overlap_y <= 0.0 || overlap_z <= 0.0 {
            return None;
        }

        // 找到最小穿深轴作为法线。
        let (normal, penetration) = if overlap_x <= overlap_y && overlap_x <= overlap_z {
            let dir = if a.position.x < b.position.x { -Vec3::X } else { Vec3::X };
            (dir, overlap_x)
        } else if overlap_y <= overlap_z {
            let dir = if a.position.y < b.position.y { -Vec3::Y } else { Vec3::Y };
            (dir, overlap_y)
        } else {
            let dir = if a.position.z < b.position.z { -Vec3::Z } else { Vec3::Z };
            (dir, overlap_z)
        };

        let point = a.position + normal * (penetration * 0.5);
        Some(Collision {
            body_a: a.id,
            body_b: b.id,
            normal,
            penetration,
            point,
        })
    }

    /// 球体-AABB 碰撞。
    fn sphere_aabb(
        a: &RigidBody,
        b: &RigidBody,
        radius: f32,
        b_min: Vec3,
        b_max: Vec3,
    ) -> Option<Collision> {
        // 找到 AABB 上距离球心最近的点。
        let closest = a.position.clamp(b_min, b_max);
        let delta = a.position - closest;
        let dist_sq = delta.length_squared();

        if dist_sq >= radius * radius || dist_sq < 1e-12 {
            return None;
        }
        let dist = dist_sq.sqrt();
        let normal = delta / dist;
        let penetration = radius - dist;
        let point = closest;
        Some(Collision {
            body_a: a.id,
            body_b: b.id,
            normal,
            penetration,
            point,
        })
    }

    /// 球体-平面碰撞。
    fn sphere_plane(
        a: &RigidBody,
        _b: &RigidBody,
        radius: f32,
        plane_normal: Vec3,
        plane_distance: f32,
    ) -> Option<Collision> {
        let signed_dist = plane_normal.dot(a.position) - plane_distance;
        if signed_dist > radius {
            return None;
        }
        let normal = plane_normal;
        let penetration = radius - signed_dist;
        let point = a.position - normal * signed_dist;
        Some(Collision {
            body_a: a.id,
            body_b: _b.id,
            normal,
            penetration,
            point,
        })
    }

    /// AABB-平面碰撞（简化：检查 AABB 中心到平面的距离）。
    fn aabb_plane(
        a: &RigidBody,
        b: &RigidBody,
        plane_normal: Vec3,
        plane_distance: f32,
    ) -> Option<Collision> {
        match a.collider {
            ColliderShape::Aabb { min, max } => {
                // 计算 AABB 到平面的最小有符号距离。
                let center = (min + max) * 0.5;
                let extents = (max - min) * 0.5;
                let r = extents.x * plane_normal.x.abs()
                    + extents.y * plane_normal.y.abs()
                    + extents.z * plane_normal.z.abs();
                let signed_dist = plane_normal.dot(center) - plane_distance;
                if signed_dist > r {
                    return None;
                }
                let normal = plane_normal;
                let penetration = r - signed_dist;
                let point = center - normal * signed_dist;
                Some(Collision {
                    body_a: a.id,
                    body_b: b.id,
                    normal,
                    penetration,
                    point,
                })
            }
            _ => None,
        }
    }

    /// 碰撞响应：对每个碰撞对应用冲量并进行位置校正。
    fn resolve_collisions(&mut self, manifold: &CollisionManifold, _dt: f32) {
        const POSITION_CORRECTION: f32 = 0.3; // 位置校正比例（防止下陷）。
        const PENETRATION_SLOP: f32 = 0.01;    // 允许的微小穿深。

        for collision in &manifold.collisions {
            let a_id = collision.body_a;
            let b_id = collision.body_b;

            let a_static = self.bodies.get(&a_id).map_or(true, |b| b.is_static);
            let b_static = self.bodies.get(&b_id).map_or(true, |b| b.is_static);

            if a_static && b_static {
                continue;
            }

            let a_inv_mass = self
                .bodies
                .get(&a_id)
                .map(|b| b.inverse_mass)
                .unwrap_or(0.0);
            let b_inv_mass = self
                .bodies
                .get(&b_id)
                .map(|b| b.inverse_mass)
                .unwrap_or(0.0);
            let total_inv_mass = a_inv_mass + b_inv_mass;
            if total_inv_mass <= 0.0 {
                continue;
            }

            let restitution = self
                .bodies
                .get(&a_id)
                .map(|b| b.restitution)
                .unwrap_or(0.5)
                .min(self.bodies.get(&b_id).map(|b| b.restitution).unwrap_or(0.5));
            let friction = self
                .bodies
                .get(&a_id)
                .map(|b| b.friction)
                .unwrap_or(0.3)
                .min(self.bodies.get(&b_id).map(|b| b.friction).unwrap_or(0.3));

            // 相对速度。
            let vel_a = self.bodies.get(&a_id).map(|b| b.velocity).unwrap_or(Vec3::ZERO);
            let vel_b = self.bodies.get(&b_id).map(|b| b.velocity).unwrap_or(Vec3::ZERO);
            let relative_vel = vel_a - vel_b;

            // 法线方向的分离速度。
            let sep_vel = relative_vel.dot(collision.normal);

            // 只处理接近的碰撞（正向分离不处理）。
            if sep_vel > 0.0 {
                continue;
            }

            // 法向冲量标量。
            let j = -(1.0 + restitution) * sep_vel / total_inv_mass;
            let impulse = collision.normal * j;

            // 应用冲量。
            if !a_static {
                if let Some(body) = self.bodies.get_mut(&a_id) {
                    body.velocity += impulse * a_inv_mass;
                }
            }
            if !b_static {
                if let Some(body) = self.bodies.get_mut(&b_id) {
                    body.velocity -= impulse * b_inv_mass;
                }
            }

            // 摩擦（切向冲量）。
            let tangent_vel = relative_vel - collision.normal * sep_vel;
            let tangent_speed = tangent_vel.length();
            if tangent_speed > 1e-6 {
                let tangent_dir = tangent_vel / tangent_speed;
                let jt = -tangent_speed / total_inv_mass;
                let max_friction = friction * j.abs();
                let jt_clamped = jt.clamp(-max_friction, max_friction);
                let friction_impulse = tangent_dir * jt_clamped;
                if !a_static {
                    if let Some(body) = self.bodies.get_mut(&a_id) {
                        body.velocity += friction_impulse * a_inv_mass;
                    }
                }
                if !b_static {
                    if let Some(body) = self.bodies.get_mut(&b_id) {
                        body.velocity -= friction_impulse * b_inv_mass;
                    }
                }
            }

            // 位置校正（防止下陷/穿模）。
            let penetration_correction = ((collision.penetration - PENETRATION_SLOP)
                .max(0.0)
                / total_inv_mass)
                * POSITION_CORRECTION;
            let correction = collision.normal * penetration_correction;
            if !a_static {
                if let Some(body) = self.bodies.get_mut(&a_id) {
                    body.position += correction * a_inv_mass;
                }
            }
            if !b_static {
                if let Some(body) = self.bodies.get_mut(&b_id) {
                    body.position -= correction * b_inv_mass;
                }
            }
        }
    }

    /// 约束求解：对每个启用的约施加修正力/位置校正。
    ///
    /// 使用位置基础动力学（PBD）风格的迭代求解，多次迭代以提高稳定性。
    fn resolve_constraints(&mut self, dt: f32) {
        const ITERATIONS: i32 = 8;

        for _ in 0..ITERATIONS {
            for constraint in &self.constraints {
                if !constraint.enabled {
                    continue;
                }

                let body_a_id = constraint.body_a;
                let body_b_id = constraint.body_b;

                // 先读取两个身体的状态（不可变借用，自动释放）。
                let (pos_a, vel_a, inv_mass_a, static_a) = {
                    let b = self.bodies.get(&body_a_id).unwrap();
                    (b.position, b.velocity, b.inverse_mass, b.is_static)
                };
                let (pos_b, vel_b, inv_mass_b, static_b) = {
                    let b = self.bodies.get(&body_b_id).unwrap();
                    (b.position, b.velocity, b.inverse_mass, b.is_static)
                };

                let total_inv_mass = inv_mass_a + inv_mass_b;
                if total_inv_mass <= 0.0 || (static_a && static_b) {
                    continue;
                }

                match constraint.ctype {
                    ConstraintType::Spring {
                        stiffness,
                        damping,
                        rest_length,
                    } => {
                        let delta = pos_a - pos_b;
                        let dist = delta.length();
                        if dist < 1e-6 {
                            continue;
                        }
                        let normal = delta / dist;

                        // 胡克定律：F = -k * (dist - rest_length) * normal
                        let extension = dist - rest_length;
                        let spring_force = -stiffness * extension * normal;

                        // 阻尼：F_d = -damping * relative_velocity
                        let rel_vel = vel_a - vel_b;
                        let damping_force = -damping * rel_vel.dot(normal) * normal;

                        let total_force = spring_force + damping_force;

                        // 应用力（等效于加速度 * dt）
                        if !static_a {
                            if let Some(body) = self.bodies.get_mut(&body_a_id) {
                                body.velocity += total_force * inv_mass_a * dt;
                            }
                        }
                        if !static_b {
                            if let Some(body) = self.bodies.get_mut(&body_b_id) {
                                body.velocity -= total_force * inv_mass_b * dt;
                            }
                        }
                    }
                    ConstraintType::Distance { distance } => {
                        let delta = pos_a - pos_b;
                        let dist = delta.length();
                        if dist < 1e-6 {
                            continue;
                        }
                        let normal = delta / dist;

                        // 距离误差
                        let error = dist - distance;
                        let correction = error * 0.5; // 平均分配修正
                        let offset = normal * correction;

                        if !static_a {
                            if let Some(body) = self.bodies.get_mut(&body_a_id) {
                                body.position -= offset * inv_mass_a / total_inv_mass;
                            }
                        }
                        if !static_b {
                            if let Some(body) = self.bodies.get_mut(&body_b_id) {
                                body.position += offset * inv_mass_b / total_inv_mass;
                            }
                        }

                        // 沿约束法线消除相对速度（防止震荡）
                        let rel_vel = vel_a - vel_b;
                        let vel_along_normal = rel_vel.dot(normal);
                        if vel_along_normal > 0.0 {
                            let impulse = -vel_along_normal / total_inv_mass;
                            if !static_a {
                                if let Some(body) = self.bodies.get_mut(&body_a_id) {
                                    body.velocity += normal * impulse * inv_mass_a;
                                }
                            }
                            if !static_b {
                                if let Some(body) = self.bodies.get_mut(&body_b_id) {
                                    body.velocity -= normal * impulse * inv_mass_b;
                                }
                            }
                        }
                    }
                    ConstraintType::PointToPoint {
                        anchor_a,
                        anchor_b,
                    } => {
                        // 简化：使用刚体当前位置作为世界空间锚点（忽略旋转）
                        let world_a = pos_a + anchor_a;
                        let world_b = pos_b + anchor_b;
                        let delta = world_a - world_b;
                        let dist = delta.length();
                        if dist < 1e-6 {
                            continue;
                        }
                        let normal = delta / dist;

                        // 位置校正：将两个锚点拉到一起
                        let correction = dist * 0.5;
                        let offset = normal * correction;

                        if !static_a {
                            if let Some(body) = self.bodies.get_mut(&body_a_id) {
                                body.position -= offset * inv_mass_a / total_inv_mass;
                            }
                        }
                        if !static_b {
                            if let Some(body) = self.bodies.get_mut(&body_b_id) {
                                body.position += offset * inv_mass_b / total_inv_mass;
                            }
                        }

                        // 速度校正：消除沿法线的相对速度
                        let rel_vel = vel_a - vel_b;
                        let vel_along_normal = rel_vel.dot(normal);
                        if vel_along_normal > 0.0 {
                            let impulse = -vel_along_normal / total_inv_mass;
                            if !static_a {
                                if let Some(body) = self.bodies.get_mut(&body_a_id) {
                                    body.velocity += normal * impulse * inv_mass_a;
                                }
                            }
                            if !static_b {
                                if let Some(body) = self.bodies.get_mut(&body_b_id) {
                                    body.velocity -= normal * impulse * inv_mass_b;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self::new(PhysicsConfig::default())
    }
}

/// 角色控制器：用于玩家/NPC 的胶囊碰撞体和运动控制.
///
/// 提供：
/// - 行走/奔跑移动（WASD 方向）
/// - 跳跃
/// - 重力
/// - 与静态几何的碰撞滑动
/// - 地面检测
pub struct CharacterController {
    /// 关联的刚体 ID。
    pub body_id: BodyId,
    /// 行走速度（单位/秒）。
    pub walk_speed: f32,
    /// 奔跑速度（单位/秒）。
    pub run_speed: f32,
    /// 跳跃初速度（单位/秒，向上）。
    pub jump_speed: f32,
    /// 重力缩放系数（1.0 = 使用世界重力）。
    pub gravity_scale: f32,
    /// 当前是否接触地面。
    pub grounded: bool,
    /// 碰撞体半径。
    pub radius: f32,
    /// 碰撞体高度。
    pub height: f32,
    /// 上一帧的移动方向输入（归一化）。
    pub move_input: Vec3,
    /// 是否按下跳跃。
    pub jump_pressed: bool,
}

impl CharacterController {
    /// 创建一个角色控制器。
    pub fn new(
        body_id: BodyId,
        radius: f32,
        height: f32,
    ) -> Self {
        Self {
            body_id,
            walk_speed: 3.0,
            run_speed: 6.0,
            jump_speed: 6.0,
            gravity_scale: 1.0,
            grounded: false,
            radius,
            height,
            move_input: Vec3::ZERO,
            jump_pressed: false,
        }
    }

    /// 设置移动方向输入（来自 InputManager，x=左右，z=前后，y=0）。
    pub fn set_move_input(&mut self, direction: Vec3) {
        self.move_input = direction;
    }

    /// 设置跳跃输入。
    pub fn set_jump(&mut self, pressed: bool) {
        self.jump_pressed = pressed;
    }

    /// 执行一帧角色控制逻辑。
    ///
    /// 应在物理世界 `step()` 之后调用，使用上一帧的碰撞结果判断地面状态。
    pub fn update(
        &mut self,
        world: &mut PhysicsWorld,
        dt: f32,
    ) -> Vec3 {
        let Some(body) = world.body(self.body_id) else {
            return Vec3::ZERO;
        };

        // 检查地面状态：从上一帧碰撞流形中查找与本体的下方碰撞。
        let mut grounded = false;
        for collision in &world.last_manifold().collisions {
            if collision.body_a == self.body_id || collision.body_b == self.body_id {
                // 如果法线指向上方（碰撞体在下方），视为接触地面。
                if collision.normal.y > 0.7 {
                    grounded = true;
                }
            }
        }
        self.grounded = grounded;

        // 计算期望速度。
        let speed = if self.move_input.length_squared() > 1.0 {
            self.move_input.normalize()
        } else {
            self.move_input
        };
        let move_speed = if self.move_input.length_squared() > 0.01 {
            // TODO: 检测 Shift 键切换奔跑
            self.walk_speed
        } else {
            0.0
        };
        let desired_velocity = speed * move_speed;

        // 应用移动加速度（简单线性插值，避免突变）。
        let accel = 20.0;
        let target_h = Vec3::new(desired_velocity.x, body.velocity.y, desired_velocity.z);
        let new_velocity = body.velocity.lerp(target_h, accel * dt.clamp(0.0, 1.0));

        // 跳跃。
        let mut jump_vel = new_velocity.y;
        if self.jump_pressed && self.grounded {
            jump_vel = self.jump_speed;
            self.jump_pressed = false;
        }

        // 应用重力。
        let gravity = world.gravity() * self.gravity_scale * dt;
        let final_velocity = Vec3::new(new_velocity.x, jump_vel + gravity.y, new_velocity.z);

        // 更新刚体速度。
        if let Some(body_mut) = world.body_mut(self.body_id) {
            body_mut.set_velocity(final_velocity);
        }

        final_velocity
    }

    /// 返回角色控制器的世界空间 AABB（基于关联刚体位置）。
    pub fn world_aabb(&self, world: &PhysicsWorld) -> Option<(Vec3, Vec3)> {
        let body = world.body(self.body_id)?;
        let half_h = self.height * 0.5;
        let min = body.position - Vec3::new(self.radius, half_h, self.radius);
        let max = body.position + Vec3::new(self.radius, half_h, self.radius);
        Some((min, max))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sphere_sphere_collision() {
        let mut world = PhysicsWorld::default();
        let a = RigidBody::sphere(BodyId(0), Vec3::ZERO, 1.0, 1.0);
        let b = RigidBody::sphere(BodyId(1), Vec3::new(1.5, 0.0, 0.0), 1.0, 1.0);
        world.add_body(a);
        world.add_body(b);

        let manifold = world.detect_collisions();
        assert!(!manifold.collisions.is_empty());
        let c = &manifold.collisions[0];
        assert_eq!(c.body_a, BodyId(0));
        assert_eq!(c.body_b, BodyId(1));
        assert!((c.normal - Vec3::new(-1.0, 0.0, 0.0)).length() < 0.01);
        assert!((c.penetration - 0.5).abs() < 0.01);
    }

    #[test]
    fn no_collision_when_separated() {
        let mut world = PhysicsWorld::default();
        let a = RigidBody::sphere(BodyId(0), Vec3::ZERO, 1.0, 1.0);
        let b = RigidBody::sphere(BodyId(1), Vec3::new(3.0, 0.0, 0.0), 1.0, 1.0);
        world.add_body(a);
        world.add_body(b);

        let manifold = world.detect_collisions();
        assert!(manifold.collisions.is_empty());
    }

    #[test]
    fn static_body_not_affected() {
        let mut world = PhysicsWorld::default();
        let floor = RigidBody::plane(BodyId(0), Vec3::Y, 0.0);
        let ball = RigidBody::sphere(BodyId(1), Vec3::new(0.0, 10.0, 0.0), 1.0, 1.0);
        world.add_body(floor);
        world.add_body(ball);

        let floor_before = world.body(BodyId(0)).unwrap().position;
        world.step();
        let floor_after = world.body(BodyId(0)).unwrap().position;
        assert_eq!(floor_before, floor_after);

        // 球体应该下落。
        let ball_after = world.body(BodyId(1)).unwrap().position;
        assert!(ball_after.y < 10.0);
    }

    #[test]
    fn add_and_remove_body() {
        let mut world = PhysicsWorld::default();
        let body = RigidBody::sphere(BodyId::null(), Vec3::ZERO, 1.0, 1.0);
        let id = world.add_body(body);
        assert!(!id.is_null());
        assert_eq!(world.body_count(), 1);

        let removed = world.remove_body(id);
        assert!(removed.is_some());
        assert_eq!(world.body_count(), 0);
        assert!(world.body(id).is_none());
    }

    #[test]
    fn character_controller_grounded() {
        let mut world = PhysicsWorld::default();
        let floor = RigidBody::plane(BodyId(0), Vec3::Y, 0.0);
        let body = RigidBody::sphere(BodyId(1), Vec3::new(0.0, 1.0, 0.0), 0.5, 1.0);
        world.add_body(floor);
        world.add_body(body);

        let mut cc = CharacterController::new(BodyId(1), 0.5, 1.8);
        world.step();
        cc.update(&mut world, 1.0 / 60.0);

        // 球体位于 y=1.0，半径 0.5，底部在 y=0.5，与平面 y=0 不重叠，不应 grounded。
        assert!(!cc.grounded);
    }

    #[test]
    fn spring_constraint_applies_force() {
        let mut world = PhysicsWorld::default();
        // 两个动态球体，用弹簧连接
        let a = RigidBody::sphere(BodyId(0), Vec3::new(-2.0, 0.0, 0.0), 0.5, 1.0);
        let b = RigidBody::sphere(BodyId(1), Vec3::new(2.0, 0.0, 0.0), 0.5, 1.0);
        world.add_body(a);
        world.add_body(b);

        let spring = Constraint {
            id: ConstraintId::null(),
            body_a: BodyId(0),
            body_b: BodyId(1),
            ctype: ConstraintType::Spring {
                stiffness: 10.0,
                damping: 1.0,
                rest_length: 1.0,
            },
            enabled: true,
        };
        world.add_constraint(spring);

        let vel_before_a = world.body(BodyId(0)).unwrap().velocity;
        world.step();
        let vel_after_a = world.body(BodyId(0)).unwrap().velocity;

        // 距离为 4.0 > rest_length 1.0，弹簧应该拉向彼此，A 应该向右加速
        assert!(vel_after_a.x > vel_before_a.x);
    }

    #[test]
    fn distance_constraint_keeps_distance() {
        let mut world = PhysicsWorld::default();
        let a = RigidBody::sphere(BodyId(0), Vec3::ZERO, 0.5, 1.0);
        let b = RigidBody::sphere(BodyId(1), Vec3::new(5.0, 0.0, 0.0), 0.5, 1.0);
        world.add_body(a);
        world.add_body(b);

        let dist = Constraint {
            id: ConstraintId::null(),
            body_a: BodyId(0),
            body_b: BodyId(1),
            ctype: ConstraintType::Distance { distance: 2.0 },
            enabled: true,
        };
        world.add_constraint(dist);

        // 多步模拟，两个身体应该被拉近到距离 2.0
        for _ in 0..10 {
            world.step();
        }

        let pos_a = world.body(BodyId(0)).unwrap().position;
        let pos_b = world.body(BodyId(1)).unwrap().position;
        let dist_after = (pos_a - pos_b).length();
        // 距离应该接近 2.0（允许一定误差）
        assert!((dist_after - 2.0).abs() < 1.0);
    }

    #[test]
    fn constraint_enable_disable() {
        let mut world = PhysicsWorld::default();
        let a = RigidBody::sphere(BodyId(0), Vec3::ZERO, 0.5, 1.0);
        let b = RigidBody::sphere(BodyId(1), Vec3::new(5.0, 0.0, 0.0), 0.5, 1.0);
        world.add_body(a);
        world.add_body(b);

        let dist = Constraint {
            id: ConstraintId::null(),
            body_a: BodyId(0),
            body_b: BodyId(1),
            ctype: ConstraintType::Distance { distance: 2.0 },
            enabled: true,
        };
        let cid = world.add_constraint(dist);

        // 禁用约束
        if let Some(c) = world.constraints.iter_mut().find(|c| c.id == cid) {
            c.enabled = false;
        }

        // 多步模拟，两个身体应该自由下落/分离（不受距离约束限制）
        for _ in 0..10 {
            world.step();
        }

        // 验证约束已禁用
        assert!(!world.constraints.iter().find(|c| c.id == cid).unwrap().enabled);
    }

    #[test]
    fn add_remove_constraint() {
        let mut world = PhysicsWorld::default();
        let a = RigidBody::sphere(BodyId(0), Vec3::ZERO, 0.5, 1.0);
        let b = RigidBody::sphere(BodyId(1), Vec3::new(2.0, 0.0, 0.0), 0.5, 1.0);
        world.add_body(a);
        world.add_body(b);

        let spring = Constraint {
            id: ConstraintId::null(),
            body_a: BodyId(0),
            body_b: BodyId(1),
            ctype: ConstraintType::Spring {
                stiffness: 10.0,
                damping: 1.0,
                rest_length: 1.0,
            },
            enabled: true,
        };
        let cid = world.add_constraint(spring);
        assert_eq!(world.constraint_count(), 1);

        let removed = world.remove_constraint(cid);
        assert!(removed.is_some());
        assert_eq!(world.constraint_count(), 0);
    }
}
