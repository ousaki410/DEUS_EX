//! Property System - Type-safe property definitions and serialization
//!
//! Reference: UE1 `Core/Object/Property.h`, `Property.cpp`

use std::fmt;
use super::*;
use crate::name::Name;

/// Property flags (match UE1 EPropertyFlags)
bitflags::bitflags! {
    pub struct PropertyFlags: u32 {
        const EDIT = 0x00000001;
        const CONST = 0x00000002;
        const INPUT = 0x00000004;
        const EXPORT = 0x00000008;
        const OPTIONAL = 0x00000010;
        const NET = 0x00000020;
        const EDIT_CONSTANT = 0x00000040;
        const GLOBAL = 0x00000080;
        const NATIVE = 0x00000100;
        const TRANSIENT = 0x00000200;
        const CONFIG = 0x00000400;
        const LOCALIZED = 0x00000800;
        const TRAVEL = 0x00001000;
        const EDIT_INLINE = 0x00002000;
        const NEW_INSTANCE = 0x00004000;
        const COMPONENT = 0x00008000;
        const OUT_PARM = 0x00010000;
        const SKIP_PARM = 0x00020000;
        const RETURN_PARM = 0x00040000;
        const COERCE_PARM = 0x00080000;
        const DEPRECATED = 0x00100000;
    }
}

/// Base property trait
pub trait PropertyBase {
    fn name(&self) -> Name;
    fn flags(&self) -> PropertyFlags;
    fn offset(&self) -> usize;
    fn size(&self) -> usize;
    fn array_dim(&self) -> u32;
    fn property_type(&self) -> PropertyType;
    fn owner_class(&self) -> Option<&Class>;
}

/// Property type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyType {
    Bool,
    Byte,
    Int,
    UInt,
    Float,
    Double,
    String,
    Name,
    Object,
    Class,
    Delegate,
    Interface,
    Struct,
    Array,
    Map,
    Set,
    Enum,
    Vector,
    Rotator,
    // ...
}

/// Concrete property
#[derive(Debug, Clone)]
pub struct Property {
    name: Name,
    flags: PropertyFlags,
    offset: usize,
    size: usize,
    array_dim: u32,
    prop_type: PropertyType,
    // Type-specific data
    inner: PropertyInner,
}

#[derive(Debug, Clone)]
pub enum PropertyInner {
    Simple,
    Enum { enum_name: Name },
    Struct { struct_class: Arc<Class> },
    Class { meta_class: Arc<Class> },
    Object { property_class: Arc<Class> },
    Interface { interface_class: Arc<Class> },
    Array { inner: Box<Property> },
    Delegate { signature_name: Name },
    // For generated code
    Custom { type_id: std::any::TypeId },
}

impl Property {
    pub fn new(name: impl Into<Name>, prop_type: PropertyType, offset: usize, size: usize) -> Self {
        Self {
            name: name.into(),
            flags: PropertyFlags::empty(),
            offset,
            size,
            array_dim: 1,
            prop_type,
            inner: PropertyInner::Simple,
        }
    }
    
    pub fn with_flags(mut self, flags: PropertyFlags) -> Self {
        self.flags = flags;
        self
    }
    
    pub fn with_array_dim(mut self, dim: u32) -> Self {
        self.array_dim = dim;
        self
    }
    
    pub fn name(&self) -> Name { self.name }
    pub fn flags(&self) -> PropertyFlags { self.flags }
    pub fn offset(&self) -> usize { self.offset }
    pub fn size(&self) -> usize { self.size }
    pub fn array_dim(&self) -> u32 { self.array_dim }
    pub fn property_type(&self) -> PropertyType { self.prop_type }
    
    pub fn is_net(&self) -> bool { self.flags.contains(PropertyFlags::NET) }
    pub fn is_transient(&self) -> bool { self.flags.contains(PropertyFlags::TRANSIENT) }
    pub fn is_config(&self) -> bool { self.flags.contains(PropertyFlags::CONFIG) }
    pub fn is_edit(&self) -> bool { self.flags.contains(PropertyFlags::EDIT) }
    
    /// Get property value from object instance
    pub fn get_value(&self, obj: &UObject) -> PropertyValue {
        // In generated code, this would use offset directly
        obj.get_property(self.name).unwrap_or(PropertyValue::None)
    }
    
    /// Set property value on object instance
    pub fn set_value(&self, obj: &UObject, value: PropertyValue) {
        obj.set_property(self.name, value);
    }
}

