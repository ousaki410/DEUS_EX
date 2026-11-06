//! Name System - FName equivalent
//!
//! Reference: UE1 `Core/Object/Name.h`, `Name.cpp`

use std::sync::OnceLock;
use parking_lot::RwLock;
use ahash::AHashMap;
use std::fmt;

/// Global name table
struct NameTableInner {
    names: Vec<String>,
    name_to_index: AHashMap<String, u32>,
}

fn global_name_table() -> &'static RwLock<NameTableInner> {
    static GLOBAL_NAME_TABLE: OnceLock<RwLock<NameTableInner>> = OnceLock::new();
    GLOBAL_NAME_TABLE.get_or_init(|| {
        let mut names = Vec::new();
        let mut name_to_index = AHashMap::new();
        names.push("none".to_string());
        name_to_index.insert("none".to_string(), 0);
        RwLock::new(NameTableInner {
            names,
            name_to_index,
        })
    })
}

/// FName - compact name representation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Name(pub u32);

impl Name {
    /// Create or get existing name
    pub fn new(name: impl AsRef<str>) -> Self {
        let name_str = name.as_ref().to_lowercase();
        let mut table = global_name_table().write();
        
        if let Some(&idx) = table.name_to_index.get(&name_str) {
            return Name(idx);
        }
        
        let idx = table.names.len() as u32;
        table.names.push(name_str.clone());
        table.name_to_index.insert(name_str, idx);
        Name(idx)
    }
    
    /// Get name string
    pub fn to_string(&self) -> String {
        let table = global_name_table().read();
        table.names.get(self.0 as usize).cloned().unwrap_or_else(|| format!("None_{}", self.0))
    }
    
    /// Get name as &str (requires table lock)
    pub fn as_str(&self) -> String {
        self.to_string()
    }
    
    pub fn is_none(&self) -> bool {
        self.0 == 0
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

impl From<&str> for Name {
    fn from(value: &str) -> Self {
        Name::new(value)
    }
}

impl From<String> for Name {
    fn from(value: String) -> Self {
        Name::new(value)
    }
}

/// Name table for package-local names
pub struct NameTable {
    names: Vec<String>,
    name_to_index: AHashMap<String, u32>,
}

impl NameTable {
    pub fn new() -> Self {
        Self {
            names: Vec::new(),
            name_to_index: AHashMap::new(),
        }
    }
    
    pub fn add(&mut self, name: impl AsRef<str>) -> Name {
        let name_str = name.as_ref().to_lowercase();
        if let Some(&idx) = self.name_to_index.get(&name_str) {
            return Name(idx);
        }
        let idx = self.names.len() as u32;
        self.names.push(name_str.clone());
        self.name_to_index.insert(name_str, idx);
        Name(idx)
    }
    
    pub fn get(&self, index: Name) -> Option<&str> {
        self.names.get(index.0 as usize).map(|s| s.as_str())
    }
    
    pub fn find(&self, name: &str) -> Option<Name> {
        self.name_to_index.get(&name.to_lowercase()).copied().map(Name)
    }
    
    pub fn len(&self) -> usize { self.names.len() }
}

impl Default for NameTable {
    fn default() -> Self { Self::new() }
}
