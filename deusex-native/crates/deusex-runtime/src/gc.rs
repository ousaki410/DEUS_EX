//! Garbage Collector - Mark and Sweep
//!
//! Reference: UE1 `Core/Object/GarbageCollection.cpp`

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use parking_lot::RwLock;
use ahash::AHashMap;
use super::*;

/// GC reference for tracking
#[derive(Debug, Clone)]
struct GcRef {
    object: UObject,
    strong_count: usize,
    is_root: bool,
}

/// Garbage collector
pub struct GarbageCollector {
    objects: RwLock<AHashMap<ObjectId, GcRef>>,
    roots: RwLock<Vec<ObjectId>>,
    pending_destruction: RwLock<Vec<ObjectId>>,
    collection_count: u64,
}

impl GarbageCollector {
    pub fn new() -> Self {
        Self {
            objects: RwLock::new(AHashMap::new()),
            roots: RwLock::new(Vec::new()),
            pending_destruction: RwLock::new(Vec::new()),
            collection_count: 0,
        }
    }
    
    /// Track a new object
    pub fn track(&self, obj: &UObject) {
        let mut objects = self.objects.write();
        objects.entry(obj.id()).or_insert(GcRef {
            object: obj.clone(),
            strong_count: 1,
            is_root: false,
        });
    }
    
    /// Add GC root (prevents collection)
    pub fn add_root(&self, obj: &UObject) {
        let mut objects = self.objects.write();
        if let Some(ref_) = objects.get_mut(&obj.id()) {
            ref_.is_root = true;
        }
        self.roots.write().push(obj.id());
    }
    
    /// Remove GC root
    pub fn remove_root(&self, obj: &UObject) {
        let mut objects = self.objects.write();
        if let Some(ref_) = objects.get_mut(&obj.id()) {
            ref_.is_root = false;
        }
        self.roots.write().retain(|&id| id != obj.id());
    }
    
    /// Mark object as pending destruction
    pub fn destroy(&self, obj: &UObject) {
        self.pending_destruction.write().push(obj.id());
    }
    
    /// Run full GC cycle
    pub fn collect(&mut self) -> usize {
        self.collection_count += 1;
        
        // Process pending destruction
        let mut destroyed = 0;
        {
            let mut pending = self.pending_destruction.write();
            for id in pending.drain(..) {
                if let Some(ref_) = self.objects.write().remove(&id) {
                    // Call destructor
                    ref_.object.destroy();
                    destroyed += 1;
                }
            }
        }
        
        // Mark phase
        let mut marked = HashSet::new();
        let mut queue = VecDeque::new();
        
        // Add roots
        for root_id in self.roots.read().iter() {
            if let Some(ref_) = self.objects.read().get(root_id) {
                marked.insert(*root_id);
                queue.push_back(ref_.object.clone());
            }
        }
        
        // Trace references
        while let Some(obj) = queue.pop_front() {
            self.trace_object(&obj, &mut marked, &mut queue);
        }
        
        // Sweep phase
        let mut objects = self.objects.write();
        let mut to_remove = Vec::new();
        
        for (id, ref_) in objects.iter() {
            if !marked.contains(id) && !ref_.is_root {
                to_remove.push(*id);
            }
        }
        
        for id in to_remove {
            if let Some(ref_) = objects.remove(&id) {
                ref_.object.destroy();
                destroyed += 1;
            }
        }
        
        destroyed
    }
    
    fn trace_object(&self, obj: &UObject, marked: &mut HashSet<ObjectId>, queue: &mut VecDeque<UObject>) {
        // Trace outer
        if let Some(outer) = obj.outer() {
            if marked.insert(outer.id()) {
                queue.push_back(outer);
            }
        }
        
        // Trace properties that are objects
        // In real implementation, would iterate all properties
        // For now, simplified
    }
    
    pub fn stats(&self) -> GcStats {
        let objects = self.objects.read();
        GcStats {
            total_objects: objects.len(),
            roots: self.roots.read().len(),
            pending_destruction: self.pending_destruction.read().len(),
            collection_count: self.collection_count,
        }
    }
}

impl Default for GarbageCollector {
    fn default() -> Self { Self::new() }
}

#[derive(Debug, Clone)]
pub struct GcStats {
    pub total_objects: usize,
    pub roots: usize,
    pub pending_destruction: usize,
    pub collection_count: u64,
}
