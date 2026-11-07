//! Deus Ex Native Runtime
//!
//! Core runtime systems: UObject, AActor, Reflection, GC, Property System, Latent Execution

pub mod uobject;
pub mod actor;
pub mod class;
pub mod property;
pub mod gc;
pub mod latent;
pub mod world;
pub mod name;
pub mod array;
pub mod delegate;
pub mod replication;

pub use uobject::*;
pub use actor::*;
pub use class::*;
pub use property::*;
pub use gc::*;
pub use latent::*;
pub use world::*;
pub use name::*;
pub use array::*;
pub use delegate::*;
pub use replication::*;

use std::sync::Arc;
use parking_lot::RwLock;

/// Global runtime state
pub struct Runtime {
    pub gc: GarbageCollector,
    pub class_registry: ClassRegistry,
    pub name_table: NameTable,
    pub world: Option<World>,
}

impl Runtime {
    pub fn new() -> Self {
        Self {
            gc: GarbageCollector::new(),
            class_registry: ClassRegistry::new(),
            name_table: NameTable::new(),
            world: None,
        }
    }
    
    pub fn initialize(&mut self) {
        // Register core classes
        self.register_core_classes();
    }
    
    fn register_core_classes(&mut self) {
        // UObject
        let uobject_class = ClassBuilder::new("Object")
            .flags(ClassFlags::NATIVE | ClassFlags::INTRINSIC)
            .build();
        self.class_registry.register(uobject_class);
        
        // AActor
        let actor_class = ClassBuilder::new("Actor")
            .parent("Object")
            .flags(ClassFlags::NATIVE | ClassFlags::INTRINSIC)
            .build();
        self.class_registry.register(actor_class);
    }
    
    pub fn create_world(&mut self) -> &mut World {
        let world = World::new();
        self.world = Some(world);
        self.world.as_mut().unwrap()
    }
    
    pub fn tick(&mut self, delta_time: f32) {
        // Run latent actions
        self.process_latent_actions(delta_time);
        
        // Tick world
        if let Some(world) = &mut self.world {
            world.tick(delta_time);
        }
        
        // GC cycle
        self.gc.collect();
    }
    
    fn process_latent_actions(&mut self, delta_time: f32) {
        // Process latent action manager
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

/// Global runtime instance (thread-local for game thread)
thread_local! {
    static RUNTIME: std::cell::RefCell<Option<Runtime>> = const { std::cell::RefCell::new(None) };
}

pub fn with_runtime<F, R>(f: F) -> R
where
    F: FnOnce(&mut Runtime) -> R,
{
    RUNTIME.with(|rt| {
        let mut rt = rt.borrow_mut();
        if rt.is_none() {
            *rt = Some(Runtime::new());
        }
        f(rt.as_mut().unwrap())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_new() {
        let rt = Runtime::new();
        assert!(rt.world.is_none());
    }

    #[test]
    fn test_runtime_initialize() {
        let mut rt = Runtime::new();
        rt.initialize();
        // Should have at least Object and Actor classes registered
        assert!(rt.class_registry.find(Name::new("Object")).is_some());
        assert!(rt.class_registry.find(Name::new("Actor")).is_some());
    }

    #[test]
    fn test_object_creation() {
        let mut rt = Runtime::new();
        rt.initialize();

        // Create an object through runtime
        let class = rt.class_registry.find(Name::new("Object")).unwrap();
        let obj = UObject::new(Name::new("TestObject"), class.clone());
        assert!(obj.id().0 > 0);
    }

    #[test]
    fn test_class_registry() {
        let mut rt = Runtime::new();
        rt.initialize();

        assert!(rt.class_registry.find(Name::new("Object")).is_some());
        assert!(rt.class_registry.find(Name::new("Actor")).is_some());
        assert!(rt.class_registry.find(Name::new("NonExistent")).is_none());
    }

    #[test]
    fn test_gc_collect() {
        let mut rt = Runtime::new();
        rt.initialize();
        let object_class = rt.class_registry.find(Name::new("Object")).unwrap();

        // Create some objects
        with_runtime(|_rt| {
            for i in 0..5 {
                let _obj = UObject::new(
                    Name::new(&format!("Obj{}", i)),
                    object_class.clone(),
                );
                // Object will be dropped at end of loop iteration
            }
        });

        // Force GC collection
        rt.gc.collect();
        // Should not panic - GC should handle gracefully
    }

    #[test]
    fn test_name_table() {
        let mut rt = Runtime::new();

        let name1 = rt.name_table.add("TestName");
        let name2 = rt.name_table.add("TestName");
        let name3 = rt.name_table.add("OtherName");

        // Same string should return same name id
        assert_eq!(name1, name2);
        // Different strings should return different name ids
        assert_ne!(name1, name3);
    }
}
