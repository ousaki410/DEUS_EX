//! World / Level System
//!
//! Reference: UE1 `Engine/Level.h`, `Level.cpp`, `World.cpp`

use std::collections::HashMap;
use std::sync::{Arc, Weak};
use parking_lot::RwLock;
use ahash::AHashMap;
use super::*;

/// World (level) containing actors
pub struct World {
    name: Name,
    actors: RwLock<AHashMap<ObjectId, UObject>>,
    actor_list: RwLock<Vec<UObject>>,
    time_seconds: f64,
    delta_seconds: f32,
    tick_count: u64,
    // Latent action manager
    latent_manager: LatentActionManager,
    // Physics scene
    physics_scene: Option<PhysicsScene>,
    // Navigation
    navigation_system: Option<NavigationSystem>,
}

impl World {
    pub fn new() -> Self {
        Self {
            name: Name::new("World"),
            actors: RwLock::new(AHashMap::new()),
            actor_list: RwLock::new(Vec::new()),
            time_seconds: 0.0,
            delta_seconds: 0.0,
            tick_count: 0,
            latent_manager: LatentActionManager::new(),
            physics_scene: None,
            navigation_system: None,
        }
    }
    
    pub fn with_name(name: impl Into<Name>) -> Self {
        let mut w = Self::new();
        w.name = name.into();
        w
    }
    
    pub fn name(&self) -> Name { self.name }
    pub fn time_seconds(&self) -> f64 { self.time_seconds }
    pub fn delta_seconds(&self) -> f32 { self.delta_seconds }
    pub fn tick_count(&self) -> u64 { self.tick_count }
    
    /// Spawn an actor in the world
    pub fn spawn_actor(&self, actor: UObject) -> UObject {
        let id = actor.id();
        self.actors.write().insert(id, actor.clone());
        self.actor_list.write().push(actor.clone());
        actor
    }
    
    /// Destroy an actor
    pub fn destroy_actor(&self, actor: &UObject) {
        self.actors.write().remove(&actor.id());
        self.actor_list.write().retain(|a| a.id() != actor.id());
        actor.destroy();
    }
    
    /// Find actor by name
    pub fn find_actor(&self, name: Name) -> Option<UObject> {
        self.actor_list.read().iter()
            .find(|a| a.name() == name)
            .cloned()
    }
    
    /// Find all actors of a class
    pub fn find_actors_of_class(&self, class: &Class) -> Vec<UObject> {
        self.actor_list.read().iter()
            .filter(|a| a.class().is_child_of(class))
            .cloned()
            .collect()
    }
    
    /// Get all actors
    pub fn all_actors(&self) -> Vec<UObject> {
        self.actor_list.read().clone()
    }
    
    /// Tick the world
    pub fn tick(&mut self, delta_time: f32) {
        self.delta_seconds = delta_time;
        self.time_seconds += delta_time as f64;
        self.tick_count += 1;
        
        // Update latent actions
        let mut context = LatentContext {
            world: Weak::<World>::new(), // attached by the owning runtime when available
            owner: UObject::new("Temp", ClassBuilder::new("Object").build()),
            delta_time,
        };
        self.latent_manager.update(&mut context);
        
        // Tick all actors
        let actors = self.actor_list.read().clone();
        for actor in actors {
            if actor.is_valid() {
                // Call Tick event
                // In generated code, this would be a virtual call
            }
        }
        
        // Physics tick
        if let Some(physics) = &mut self.physics_scene {
            physics.tick(delta_time);
        }
    }
    
    pub fn latent_manager(&self) -> &LatentActionManager {
        &self.latent_manager
    }
}

impl Clone for World {
    fn clone(&self) -> Self {
        Self {
            name: self.name,
            actors: RwLock::new(AHashMap::new()),
            actor_list: RwLock::new(Vec::new()),
            time_seconds: self.time_seconds,
            delta_seconds: self.delta_seconds,
            tick_count: self.tick_count,
            latent_manager: LatentActionManager::new(),
            physics_scene: None,
            navigation_system: None,
        }
    }
}

impl Default for World {
    fn default() -> Self { Self::new() }
}

/// Physics scene (placeholder)
pub struct PhysicsScene {
    // Would integrate with physics engine
}

impl PhysicsScene {
    pub fn tick(&mut self, _delta_time: f32) {}
}

/// Navigation system (placeholder)
pub struct NavigationSystem {
    // Pathfinding, navmesh
}

impl NavigationSystem {
    pub fn find_path(&self, _start: glam::Vec3, _end: glam::Vec3) -> Vec<glam::Vec3> {
        Vec::new()
    }
}
