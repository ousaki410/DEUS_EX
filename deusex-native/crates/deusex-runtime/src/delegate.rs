//! Delegate System - Multicast delegates
//!
//! Reference: UE1 `Core/Object/Delegate.h`

use std::collections::HashMap;
use parking_lot::RwLock;
use super::*;

/// Delegate signature
#[derive(Debug, Clone)]
pub struct DelegateSignature {
    pub name: Name,
    pub params: Vec<Property>,
    pub return_type: Option<Property>,
}

/// Delegate handle for removal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DelegateHandle(pub u64);

static NEXT_HANDLE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn new_handle() -> DelegateHandle {
    DelegateHandle(NEXT_HANDLE.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
}

/// Multicast delegate
pub struct MulticastDelegate {
    signature: DelegateSignature,
    callbacks: RwLock<HashMap<DelegateHandle, DelegateCallback>>,
}

/// Delegate callback storage
enum DelegateCallback {
    Function { obj: UObject, func_name: Name },
    Closure(Box<dyn Fn(&[PropertyValue]) -> PropertyValue + Send + Sync>),
}

impl MulticastDelegate {
    pub fn new(signature: DelegateSignature) -> Self {
        Self {
            signature,
            callbacks: RwLock::new(HashMap::new()),
        }
    }
    
    /// Add a function callback
    pub fn add(&self, obj: UObject, func_name: Name) -> DelegateHandle {
        let handle = new_handle();
        self.callbacks.write().insert(handle, DelegateCallback::Function { obj, func_name });
        handle
    }
    
    /// Add a closure callback
    pub fn add_closure<F>(&self, f: F) -> DelegateHandle
    where
        F: Fn(&[PropertyValue]) -> PropertyValue + Send + Sync + 'static,
    {
        let handle = new_handle();
        self.callbacks.write().insert(handle, DelegateCallback::Closure(Box::new(f)));
        handle
    }
    
    /// Remove a callback
    pub fn remove(&self, handle: DelegateHandle) -> bool {
        self.callbacks.write().remove(&handle).is_some()
    }
    
    /// Broadcast to all callbacks
    pub fn broadcast(&self, params: &[PropertyValue]) -> Vec<PropertyValue> {
        let mut results = Vec::new();
        let callbacks = self.callbacks.read();
        
        for callback in callbacks.values() {
            let result = match callback {
                DelegateCallback::Function { obj: _, func_name: _ } => {
                    // Call function on object
                    // In generated code, this would be a virtual call
                    PropertyValue::None
                }
                DelegateCallback::Closure(f) => {
                    f(params)
                }
            };
            results.push(result);
        }
        
        results
    }
    
    pub fn is_bound(&self) -> bool {
        !self.callbacks.read().is_empty()
    }
    
    pub fn num_bound(&self) -> usize {
        self.callbacks.read().len()
    }
    
    pub fn clear(&self) {
        self.callbacks.write().clear();
    }
}

/// A single-cast delegate value.
///
/// The expanded runtime supports binding callbacks; for now the stored value
/// only needs to be cloneable/debuggable so it can live inside a
/// `PropertyValue`.
#[derive(Debug, Clone)]
pub struct Delegate {
    pub signature: DelegateSignature,
    pub callback: Option<DelegateHandle>,
}

impl Delegate {
    pub fn new(signature: DelegateSignature) -> Self {
        Self {
            signature,
            callback: None,
        }
    }

    pub fn bind(&mut self, handle: DelegateHandle) {
        self.callback = Some(handle);
    }

    pub fn unbind(&mut self) {
        self.callback = None;
    }

    pub fn is_bound(&self) -> bool {
        self.callback.is_some()
    }
}

