//! Network Replication System
//!
//! Reference: UE1 `Engine/Replication.cpp`, `Actor.cpp`

use std::collections::HashMap;
use std::sync::{Arc, Weak};
use parking_lot::RwLock;
use ahash::AHashMap;
use super::*;

/// Replication info for an actor
#[derive(Debug, Clone)]
pub struct ReplicationInfo {
    pub actor: UObject,
    pub net_id: u32,
    pub owner: Option<UObject>,
    pub connection: Option<NetConnection>,
    pub replicated_properties: Vec<ReplicatedProperty>,
    pub last_update_time: f64,
    pub update_frequency: f32,
    pub priority: f32,
}

/// Property that replicates
#[derive(Debug, Clone)]
pub struct ReplicatedProperty {
    pub property: Property,
    pub condition: ReplicationCondition,
    pub last_value: PropertyValue,
}

/// When to replicate
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplicationCondition {
    Always,
    InitialOnly,
    OwnerOnly,
    SkipOwner,
    Custom,
}

/// Network connection
#[derive(Debug, Clone)]
pub struct NetConnection {
    pub id: u32,
    pub address: String,
    pub is_server: bool,
    pub bandwidth: f32,
}

/// Replication manager
pub struct ReplicationManager {
    actors: RwLock<AHashMap<u32, ReplicationInfo>>,
    connections: RwLock<AHashMap<u32, NetConnection>>,
    next_net_id: std::sync::atomic::AtomicU32,
}

impl ReplicationManager {
    pub fn new() -> Self {
        Self {
            actors: RwLock::new(AHashMap::new()),
            connections: RwLock::new(AHashMap::new()),
            next_net_id: std::sync::atomic::AtomicU32::new(1),
        }
    }
    
    /// Register actor for replication
    pub fn register_actor(&self, actor: UObject) -> u32 {
        let net_id = self.next_net_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        // Find replicated properties from class
        let class = actor.class();
        let replicated_props = class.properties()
            .iter()
            .filter(|p| p.is_net())
            .map(|p| ReplicatedProperty {
                property: p.clone(),
                condition: ReplicationCondition::Always,
                last_value: PropertyValue::None,
            })
            .collect();
        
        let info = ReplicationInfo {
            actor,
            net_id,
            owner: None,
            connection: None,
            replicated_properties: replicated_props,
            last_update_time: 0.0,
            update_frequency: 10.0, // 10 Hz default
            priority: 1.0,
        };
        
        self.actors.write().insert(net_id, info);
        net_id
    }
    
    /// Unregister actor
    pub fn unregister_actor(&self, net_id: u32) {
        self.actors.write().remove(&net_id);
    }
    
    /// Update replication for all actors
    pub fn update(&self, delta_time: f32, current_time: f64) {
        let mut actors = self.actors.write();
        for (_, info) in actors.iter_mut() {
            let time_since_update = current_time - info.last_update_time;
            if time_since_update >= (1.0 / info.update_frequency as f64) {
                self.replicate_actor(info, current_time);
            }
        }
    }
    
    fn replicate_actor(&self, info: &mut ReplicationInfo, current_time: f64) {
        // Check each replicated property for changes
        for rep_prop in &mut info.replicated_properties {
            let current_value = info.actor.get_property(rep_prop.property.name());
            
            if let Some(current) = current_value {
                if !property_values_equal(&current, &rep_prop.last_value) {
                    // Property changed - replicate
                    rep_prop.last_value = current;
                    // In real implementation, would send to connections
                }
            }
        }
        
        info.last_update_time = current_time;
    }
    
    /// Add network connection
    pub fn add_connection(&self, conn: NetConnection) {
        self.connections.write().insert(conn.id, conn);
    }
    
    pub fn remove_connection(&self, id: u32) {
        self.connections.write().remove(&id);
    }
}

impl Default for ReplicationManager {
    fn default() -> Self { Self::new() }
}

fn property_values_equal(a: &PropertyValue, b: &PropertyValue) -> bool {
    match (a, b) {
        (PropertyValue::Bool(a), PropertyValue::Bool(b)) => a == b,
        (PropertyValue::Byte(a), PropertyValue::Byte(b)) => a == b,
        (PropertyValue::Int(a), PropertyValue::Int(b)) => a == b,
        (PropertyValue::UInt(a), PropertyValue::UInt(b)) => a == b,
        (PropertyValue::Float(a), PropertyValue::Float(b)) => (a - b).abs() < 0.0001,
        (PropertyValue::Double(a), PropertyValue::Double(b)) => (a - b).abs() < 0.0001,
        (PropertyValue::String(a), PropertyValue::String(b)) => a == b,
        (PropertyValue::Name(a), PropertyValue::Name(b)) => a == b,
        (PropertyValue::Object(a), PropertyValue::Object(b)) => a == b,
        _ => false,
    }
}

/// Replication graph for prioritization
pub struct ReplicationGraph {
    // Would implement priority-based replication
}
