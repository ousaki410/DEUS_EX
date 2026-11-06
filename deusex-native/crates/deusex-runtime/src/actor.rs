//! AActor - Base class for all placeable objects
//!
//! Reference: UE1 `Engine/Actor.h`, `Actor.cpp`

use std::sync::Arc;
use super::*;
use crate::class::{Class, ClassFlags};
use crate::name::Name;

/// Actor - any object that can be placed in a level
pub struct AActor {
    base: UObject,
    // Transform
    location: glam::Vec3,
    rotation: glam::Vec3, // Pitch, Yaw, Roll (degrees)
    // Components
    root_component: Option<UObject>,
    // Physics
    velocity: glam::Vec3,
    // State
    state_name: Name,
    // Network
    remote_role: NetRole,
    // Lifecycle
    b_destroyed: bool,
}

/// Network role
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetRole {
    None = 0,
    SimulatedProxy = 1,
    AutonomousProxy = 2,
    Authority = 3,
    MAX = 4,
}

impl AActor {
    pub fn new(name: impl Into<Name>, class: Arc<Class>) -> Self {
        Self {
            base: UObject::new(name, class),
            location: glam::Vec3::ZERO,
            rotation: glam::Vec3::ZERO,
            root_component: None,
            velocity: glam::Vec3::ZERO,
            state_name: Name::new(""),
            remote_role: NetRole::None,
            b_destroyed: false,
        }
    }
    
    pub fn as_uobject(&self) -> &UObject { &self.base }
    pub fn as_uobject_mut(&mut self) -> &mut UObject { &mut self.base }
    
    // Transform
    pub fn location(&self) -> glam::Vec3 { self.location }
    pub fn set_location(&mut self, loc: glam::Vec3) { self.location = loc; }
    
    pub fn rotation(&self) -> glam::Vec3 { self.rotation }
    pub fn set_rotation(&mut self, rot: glam::Vec3) { self.rotation = rot; }
    
    // Velocity
    pub fn velocity(&self) -> glam::Vec3 { self.velocity }
    pub fn set_velocity(&mut self, vel: glam::Vec3) { self.velocity = vel; }
    
    // State
    pub fn state_name(&self) -> Name { self.state_name }
    pub fn set_state(&mut self, name: Name) { self.state_name = name; }
    
    // Network
    pub fn remote_role(&self) -> NetRole { self.remote_role }
    pub fn set_remote_role(&mut self, role: NetRole) { self.remote_role = role; }
    
    // Lifecycle
    pub fn is_destroyed(&self) -> bool { self.b_destroyed }
    pub fn destroy(&mut self) { self.b_destroyed = true; self.base.destroy(); }
    
    /// Called when actor is spawned
    pub fn begin_play(&mut self) {}
    
    /// Called every frame
    pub fn tick(&mut self, delta_time: f32) {}
    
    /// Called when actor is destroyed
    pub fn end_play(&mut self) {}
    
    /// Called when actor takes damage
    pub fn take_damage(&mut self, damage: f32, event: &DamageEvent) -> f32 { damage }
    
    /// Get actor's world
    pub fn world(&self) -> Option<&World> {
        // Would traverse outer chain to find World
        None
    }
    
    /// Get distance to another actor
    pub fn distance_to(&self, other: &AActor) -> f32 {
        (self.location - other.location).length()
    }
    
    /// Check if actor is visible from another actor's perspective
    pub fn is_visible_from(&self, _viewer: &AActor) -> bool { true }
}

/// Damage event info
#[derive(Debug, Clone)]
pub struct DamageEvent {
    pub damage_type: Option<UObject>,
    pub hit_location: glam::Vec3,
    pub hit_normal: glam::Vec3,
    pub instigator: Option<UObject>,
    pub damage_causer: Option<UObject>,
}

/// Component base class
pub struct UActorComponent {
    base: UObject,
    owner: Option<UObject>,
    b_wants_initialize: bool,
    b_auto_activate: bool,
    b_is_active: bool,
}

impl UActorComponent {
    pub fn new(name: impl Into<Name>, class: Arc<Class>) -> Self {
        Self {
            base: UObject::new(name, class),
            owner: None,
            b_wants_initialize: true,
            b_auto_activate: true,
            b_is_active: false,
        }
    }
    
    pub fn activate(&mut self) { self.b_is_active = true; }
    pub fn deactivate(&mut self) { self.b_is_active = false; }
    pub fn is_active(&self) -> bool { self.b_is_active }
    
    pub fn initialize_component(&mut self) {}
    pub fn uninitialize_component(&mut self) {}
}

/// Primitive component (has physics/collision)
pub struct UPrimitiveComponent {
    base: UActorComponent,
    // Collision
    collision_enabled: bool,
    collision_response: CollisionResponse,
    // Physics
    mass: f32,
    b_simulate_physics: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionResponse {
    Ignore,
    Overlap,
    Block,
}

impl UPrimitiveComponent {
    pub fn new(name: impl Into<Name>, class: Arc<Class>) -> Self {
        Self {
            base: UActorComponent::new(name, class),
            collision_enabled: true,
            collision_response: CollisionResponse::Block,
            mass: 100.0,
            b_simulate_physics: false,
        }
    }
    
    pub fn set_collision_enabled(&mut self, enabled: bool) { self.collision_enabled = enabled; }
    pub fn set_collision_response(&mut self, response: CollisionResponse) { self.collision_response = response; }
    pub fn set_mass(&mut self, mass: f32) { self.mass = mass; }
    pub fn set_simulate_physics(&mut self, simulate: bool) { self.b_simulate_physics = simulate; }
}
