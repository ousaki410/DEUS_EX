//! Class System - UClass, reflection, inheritance
//!
//! Reference: UE1 `Core/Object/Class.h`, `Class.cpp`

use std::sync::{Arc, Weak};
use parking_lot::RwLock;
use ahash::AHashMap;
use super::*;
use crate::name::Name;
use crate::property::{Property, PropertyFlags};

bitflags::bitflags! {
    pub struct ClassFlags: u32 {
        const ABSTRACT = 0x00000001;
        const COMPILED = 0x00000002;
        const CONFIG = 0x00000004;
        const TRANSIENT = 0x00000008;
        const PARSED = 0x00000010;
        const MATCH_SERIALIZED = 0x00000020;
        const PROJECTILE = 0x00000040;
        const NATIVE = 0x00000080;
        const NO_EXPORT = 0x00000100;
        const NOT_PLACEABLE = 0x00000200;
        const PER_OBJECT_CONFIG = 0x00000400;
        const CHILD_OF_NATIVE = 0x00000800;
        const INTRINSIC = 0x00010000;
        const DEPRECATED = 0x00020000;
        const HIDDEN = 0x00040000;
        const DEPRECATED_IN_SDK = 0x00080000;
    }
}

bitflags::bitflags! {
    pub struct FunctionFlags: u32 {
        const FINAL = 0x00000001;
        const DEFINED = 0x00000002;
        const ITERATOR = 0x00000004;
        const LATENT = 0x00000008;
        const PRE_OPERATOR = 0x00000010;
        const POST_OPERATOR = 0x00000020;
        const SINGULAR = 0x00000040;
        const NET = 0x00000080;
        const NET_RELIABLE = 0x00000100;
        const NET_REQUEST = 0x00000200;
        const NET_EXEC = 0x00000400;
        const STATIC = 0x00000800;
        const INVOKER = 0x00001000;
        const NO_EXPORT = 0x00002000;
        const NATIVE = 0x00004000;
        const OPERATOR = 0x00008000;
        const EVENT = 0x00010000;
    }
}

pub struct Class {
    name: Name,
    super_class: Option<Weak<Class>>,
    flags: ClassFlags,
    properties: Vec<Property>,
    functions: AHashMap<Name, Function>,
    defaults: AHashMap<Name, PropertyValue>,
    constructor: Option<fn() -> UObject>,
    instance_size: usize,
    property_offsets: AHashMap<Name, usize>,
}

impl std::fmt::Debug for Class {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Class")
            .field("name", &self.name)
            .field("flags", &self.flags)
            .field("properties", &self.properties.len())
            .field("functions", &self.functions.len())
            .field("instance_size", &self.instance_size)
            .finish_non_exhaustive()
    }
}

impl Class {
    pub fn name(&self) -> Name { self.name }
    pub fn super_class(&self) -> Option<Arc<Class>> { self.super_class.as_ref().and_then(|w| w.upgrade()) }
    pub fn flags(&self) -> ClassFlags { self.flags }
    pub fn properties(&self) -> &[Property] { &self.properties }
    pub fn functions(&self) -> &AHashMap<Name, Function> { &self.functions }
    pub fn defaults(&self) -> &AHashMap<Name, PropertyValue> { &self.defaults }
    pub fn instance_size(&self) -> usize { self.instance_size }
    pub fn find_property(&self, name: Name) -> Option<&Property> { self.properties.iter().find(|p| p.name() == name) }
    pub fn find_function(&self, name: Name) -> Option<&Function> { self.functions.get(&name) }
    pub fn is_child_of(&self, other: &Class) -> bool {
        if self.name() == other.name() {
            return true;
        }
        let mut current = self.super_class();
        while let Some(c) = current {
            if c.name() == other.name() { return true; }
            current = c.super_class();
        }
        false
    }
    pub fn all_properties(&self) -> Vec<Property> {
        let mut props: Vec<Property> = self.properties.clone();
        let mut current = self.super_class();
        while let Some(c) = current {
            props.extend(c.properties.iter().cloned());
            current = c.super_class();
        }
        props.reverse(); props
    }
}

#[derive(Debug, Clone)]
pub struct Function {
    name: Name,
    flags: FunctionFlags,
    return_property: Option<Property>,
    params: Vec<Property>,
    native_func: Option<NativeFunctionPtr>,
    script_index: Option<u32>,
    latent_info: Option<LatentFunctionInfo>,
}

type NativeFunctionPtr = fn(&UObject, &[PropertyValue]) -> PropertyValue;

#[derive(Debug, Clone)]
pub struct LatentFunctionInfo {
    pub callback_name: Name,
    pub uuid_property: Name,
    pub linkage: u32,
}

impl Function {
    pub fn name(&self) -> Name { self.name }
    pub fn flags(&self) -> FunctionFlags { self.flags }
    pub fn is_native(&self) -> bool { self.flags.contains(FunctionFlags::NATIVE) }
    pub fn is_latent(&self) -> bool { self.flags.contains(FunctionFlags::LATENT) }
    pub fn is_static(&self) -> bool { self.flags.contains(FunctionFlags::STATIC) }
    pub fn is_event(&self) -> bool { self.flags.contains(FunctionFlags::EVENT) }
    pub fn params(&self) -> &[Property] { &self.params }
    pub fn return_property(&self) -> Option<&Property> { self.return_property.as_ref() }
}

pub struct ClassBuilder {
    name: Name,
    super_class: Option<Arc<Class>>,
    flags: ClassFlags,
    properties: Vec<Property>,
    functions: Vec<Function>,
    defaults: AHashMap<Name, PropertyValue>,
    constructor: Option<fn() -> UObject>,
    instance_size: usize,
}

impl ClassBuilder {
    pub fn new(name: impl Into<Name>) -> Self { Self { name: name.into(), super_class: None, flags: ClassFlags::empty(), properties: Vec::new(), functions: Vec::new(), defaults: AHashMap::new(), constructor: None, instance_size: 0 } }
    pub fn parent(mut self, _name: impl Into<Name>) -> Self { self }
    pub fn flags(mut self, flags: ClassFlags) -> Self { self.flags = flags; self }
    pub fn property(mut self, prop: Property) -> Self { self.properties.push(prop); self }
    pub fn function(mut self, func: Function) -> Self { self.functions.push(func); self }
    pub fn default(mut self, name: impl Into<Name>, value: PropertyValue) -> Self { self.defaults.insert(name.into(), value); self }
    pub fn constructor(mut self, ctor: fn() -> UObject) -> Self { self.constructor = Some(ctor); self }
    pub fn instance_size(mut self, size: usize) -> Self { self.instance_size = size; self }
    pub fn build(self) -> Arc<Class> {
        let mut func_map = AHashMap::new(); for f in self.functions { func_map.insert(f.name(), f); }
        let mut prop_offsets = AHashMap::new(); let mut offset = 0; for prop in &self.properties { prop_offsets.insert(prop.name(), offset); offset += prop.size(); }
        Arc::new(Class { name: self.name, super_class: self.super_class.as_ref().map(Arc::downgrade), flags: self.flags, properties: self.properties, functions: func_map, defaults: self.defaults, constructor: self.constructor, instance_size: self.instance_size, property_offsets: prop_offsets })
    }
}

pub struct ClassRegistry { classes: RwLock<AHashMap<Name, Arc<Class>>> }
impl ClassRegistry {
    pub fn new() -> Self { Self { classes: RwLock::new(AHashMap::new()) } }
    pub fn register(&self, class: Arc<Class>) { self.classes.write().insert(class.name(), class); }
    pub fn find(&self, name: Name) -> Option<Arc<Class>> { self.classes.read().get(&name).cloned() }
    pub fn all(&self) -> Vec<Arc<Class>> { self.classes.read().values().cloned().collect() }
}
impl Default for ClassRegistry { fn default() -> Self { Self::new() } }
