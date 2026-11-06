//! UObject - Base class for all engine objects
//!
//! Reference: UE1 `Core/Object/Object.h`, `Object.cpp`

use std::any::Any;
use std::sync::{Arc, Weak};
use std::cell::UnsafeCell;
use parking_lot::RwLock;
use ahash::AHashMap;
use smallvec::SmallVec;
use super::*;
use crate::class::{Class, ClassFlags};
use crate::name::Name;
use crate::property::Property;

/// Unique object identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObjectId(pub u64);

static NEXT_OBJECT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn generate_object_id() -> ObjectId {
    ObjectId(NEXT_OBJECT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
}

/// Object flags (match UE1 EObjectFlags)
bitflags::bitflags! {
    pub struct ObjectFlags: u32 {
        const TRANSACTIONAL = 0x00000001;
        const PUBLIC = 0x00000004;
        const TEMPORARY = 0x00000008;
        const SAVE_GAME = 0x00000010;
        const NO_IMPORT = 0x00000020;
        const NEED_LOAD = 0x00000040;
        const PER_OBJECT_CONFIG = 0x00000080;
        const NEED_POST_LOAD = 0x00000100;
        const NET = 0x00000200;
        const EDITOR_ONLY = 0x00000400;
        const DESTROYED = 0x00000800;
        const GC_ROOT = 0x00001000;
        const MARK = 0x00002000;
        const NATIVE = 0x00004000;
        const NO_EXPORT = 0x00008000;
        const STANDALONE = 0x00010000;
    }
}

/// Internal object data
struct ObjectInner {
    id: ObjectId,
    name: Name,
    class: Weak<Class>,
    outer: Option<Weak<RwLock<ObjectInner>>>,
    flags: ObjectFlags,
    properties: AHashMap<Name, PropertyValue>,
    // For generated code: vtable pointer to Rust trait impl
    vtable: Option<*const ()>,
}

/// Smart pointer to UObject (like TObjectPtr in UE)
#[derive(Clone)]
pub struct UObject {
    inner: Arc<RwLock<ObjectInner>>,
}

impl std::fmt::Debug for UObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = self.inner.read();
        f.debug_struct("UObject")
            .field("id", &inner.id)
            .field("name", &inner.name)
            .field("flags", &inner.flags)
            .finish_non_exhaustive()
    }
}

impl UObject {
    /// Create a new object
    pub fn new(name: impl Into<Name>, class: Arc<Class>) -> Self {
        let inner = ObjectInner {
            id: generate_object_id(),
            name: name.into(),
            class: Arc::downgrade(&class),
            outer: None,
            flags: ObjectFlags::empty(),
            properties: AHashMap::new(),
            vtable: None,
        };
        Self { inner: Arc::new(RwLock::new(inner)) }
    }
    
    /// Create with outer
    pub fn new_outer(name: impl Into<Name>, class: Arc<Class>, outer: &UObject) -> Self {
        let mut obj = Self::new(name, class);
        obj.set_outer(outer);
        obj
    }
    
    pub fn id(&self) -> ObjectId {
        self.inner.read().id
    }
    
    pub fn name(&self) -> Name {
        self.inner.read().name
    }
    
    pub fn set_name(&self, name: impl Into<Name>) {
        self.inner.write().name = name.into();
    }
    
    pub fn class(&self) -> Arc<Class> {
        self.inner.read().class.upgrade().expect("Class should exist")
    }
    
    pub fn outer(&self) -> Option<UObject> {
        let outer = self.inner.read().outer.clone()?;
        outer.upgrade().map(|inner| UObject { inner })
    }
    
    pub fn set_outer(&self, outer: &UObject) {
        self.inner.write().outer = Some(Arc::downgrade(&outer.inner));
    }
    
    pub fn flags(&self) -> ObjectFlags {
        self.inner.read().flags
    }
    
    pub fn set_flags(&self, flags: ObjectFlags) {
        self.inner.write().flags |= flags;
    }
    
    pub fn clear_flags(&self, flags: ObjectFlags) {
        self.inner.write().flags -= flags;
    }
    
    pub fn has_flags(&self, flags: ObjectFlags) -> bool {
        self.inner.read().flags.contains(flags)
    }
    
    /// Get property value
    pub fn get_property(&self, name: impl Into<Name>) -> Option<PropertyValue> {
        self.inner.read().properties.get(&name.into()).cloned()
    }
    
    /// Set property value
    pub fn set_property(&self, name: impl Into<Name>, value: PropertyValue) {
        self.inner.write().properties.insert(name.into(), value);
    }
    
    /// Get property with type safety
    pub fn get_typed_property<T: PropertyType>(&self, name: impl Into<Name>) -> Option<T> {
        self.get_property(name).and_then(|v| v.cast())
    }
    
    /// Set vtable for generated code
    pub fn set_vtable(&self, vtable: *const ()) {
        self.inner.write().vtable = Some(vtable);
    }
    
    pub fn vtable(&self) -> Option<*const ()> {
        self.inner.read().vtable
    }
    
    /// Check if object is valid (not destroyed)
    pub fn is_valid(&self) -> bool {
        !self.has_flags(ObjectFlags::DESTROYED)
    }
    
    /// Mark for destruction
    pub fn destroy(&self) {
        self.set_flags(ObjectFlags::DESTROYED);
    }
    
    /// Get full path name
    pub fn full_name(&self) -> String {
        let mut parts = Vec::new();
        let mut current = Some(self.clone());
        
        while let Some(obj) = current {
            parts.push(obj.name().to_string());
            current = obj.outer();
        }
        
        parts.reverse();
        parts.join(".")
    }
    
    /// Downcast to specific type (for generated code)
    pub fn cast<T: 'static>(&self) -> Option<T> {
        // In generated code, this would use the vtable
        None
    }
}

impl PartialEq for UObject {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Eq for UObject {}

impl std::hash::Hash for UObject {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id().0.hash(state);
    }
}

/// Property value storage (type-erased)
#[derive(Debug, Clone)]
pub enum PropertyValue {
    None,
    Bool(bool),
    Byte(u8),
    Int(i32),
    UInt(u32),
    Float(f32),
    Double(f64),
    String(String),
    Name(Name),
    Object(UObject),
    Class(Arc<Class>),
    Delegate(Delegate),
    Array(Vec<PropertyValue>),
    Struct(AHashMap<Name, PropertyValue>),
    Vector(glam::Vec3),
    Rotator(glam::Vec3),
    Raw(Vec<u8>),
}

impl PropertyValue {
    pub fn cast<T: PropertyType>(&self) -> Option<T> {
        T::from_property(self)
    }
    
    pub fn as_bool(&self) -> Option<bool> {
        match self { Self::Bool(b) => Some(*b), _ => None }
    }
    
    pub fn as_int(&self) -> Option<i32> {
        match self { Self::Int(i) => Some(*i), _ => None }
    }
    
    pub fn as_float(&self) -> Option<f32> {
        match self { Self::Float(f) => Some(*f), _ => None }
    }
    
    pub fn as_string(&self) -> Option<&String> {
        match self { Self::String(s) => Some(s), _ => None }
    }
    
    pub fn as_object(&self) -> Option<&UObject> {
        match self { Self::Object(o) => Some(o), _ => None }
    }
}

/// Trait for property types that can be stored/retrieved
pub trait PropertyType: Sized {
    fn from_property(value: &PropertyValue) -> Option<Self>;
    fn to_property(&self) -> PropertyValue;
}

impl PropertyType for bool {
    fn from_property(v: &PropertyValue) -> Option<Self> { v.as_bool() }
    fn to_property(&self) -> PropertyValue { PropertyValue::Bool(*self) }
}

impl PropertyType for i32 {
    fn from_property(v: &PropertyValue) -> Option<Self> { v.as_int() }
    fn to_property(&self) -> PropertyValue { PropertyValue::Int(*self) }
}

impl PropertyType for f32 {
    fn from_property(v: &PropertyValue) -> Option<Self> { v.as_float() }
    fn to_property(&self) -> PropertyValue { PropertyValue::Float(*self) }
}

impl PropertyType for String {
    fn from_property(v: &PropertyValue) -> Option<Self> { v.as_string().cloned() }
    fn to_property(&self) -> PropertyValue { PropertyValue::String(self.clone()) }
}

impl PropertyType for UObject {
    fn from_property(v: &PropertyValue) -> Option<Self> { v.as_object().cloned() }
    fn to_property(&self) -> PropertyValue { PropertyValue::Object(self.clone()) }
}
