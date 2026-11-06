//! Latent Action System - for state code and async operations
//!
//! Reference: UE1 `Core/Object/State.cpp`, `LatentActionManager.cpp`

use std::collections::HashMap;
use std::sync::{Arc, Weak};
use parking_lot::RwLock;
use ahash::AHashMap;
use super::*;

/// Latent action base
pub trait LatentAction: Send + Sync {
    /// Update the action, return true if completed
    fn update(&mut self, context: &mut LatentContext, delta_time: f32) -> bool;
    
    /// Get action UUID for linking
    fn uuid(&self) -> u32;
    
    /// Get callback name
    fn callback(&self) -> Option<Name> { None }
}

/// Context provided to latent actions
pub struct LatentContext {
    pub world: Weak<World>,
    pub owner: UObject,
    pub delta_time: f32,
}

/// Latent action manager
pub struct LatentActionManager {
    actions: RwLock<AHashMap<u32, Box<dyn LatentAction>>>,
    next_uuid: std::sync::atomic::AtomicU32,
}

impl LatentActionManager {
    pub fn new() -> Self {
        Self {
            actions: RwLock::new(AHashMap::new()),
            next_uuid: std::sync::atomic::AtomicU32::new(1),
        }
    }
    
    pub fn add_action(&self, mut action: Box<dyn LatentAction>) -> u32 {
        let uuid = self.next_uuid.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.actions.write().insert(uuid, action);
        uuid
    }
    
    pub fn remove_action(&self, uuid: u32) -> Option<Box<dyn LatentAction>> {
        self.actions.write().remove(&uuid)
    }
    
    pub fn update(&self, context: &mut LatentContext) {
        let mut completed = Vec::new();
        
        {
            let mut actions = self.actions.write();
            for (uuid, action) in actions.iter_mut() {
                if action.update(context, context.delta_time) {
                    completed.push(*uuid);
                }
            }
        }
        
        for uuid in completed {
            self.actions.write().remove(&uuid);
        }
    }
    
    pub fn generate_uuid(&self) -> u32 {
        self.next_uuid.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }
}

impl Default for LatentActionManager {
    fn default() -> Self { Self::new() }
}

/// Built-in latent actions

/// Sleep for specified time
pub struct SleepAction {
    uuid: u32,
    remaining_time: f32,
    callback: Option<Name>,
}

impl SleepAction {
    pub fn new(uuid: u32, time: f32, callback: Option<Name>) -> Self {
        Self { uuid, remaining_time: time, callback }
    }
}

impl LatentAction for SleepAction {
    fn update(&mut self, _context: &mut LatentContext, delta_time: f32) -> bool {
        self.remaining_time -= delta_time;
        self.remaining_time <= 0.0
    }
    
    fn uuid(&self) -> u32 { self.uuid }
    
    fn callback(&self) -> Option<Name> { self.callback }
}

/// Wait for another latent action to complete
pub struct WaitForAction {
    uuid: u32,
    target_uuid: u32,
    callback: Option<Name>,
}

impl WaitForAction {
    pub fn new(uuid: u32, target_uuid: u32, callback: Option<Name>) -> Self {
        Self { uuid, target_uuid, callback }
    }
}

impl LatentAction for WaitForAction {
    fn update(&mut self, context: &mut LatentContext, _delta_time: f32) -> bool {
        // Check if target action still exists
        // In real implementation, would check action manager
        // For now, always return false (wait forever)
        false
    }
    
    fn uuid(&self) -> u32 { self.uuid }
    
    fn callback(&self) -> Option<Name> { self.callback }
}
