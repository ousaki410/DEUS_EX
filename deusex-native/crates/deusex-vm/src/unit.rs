//! Loaded package plus the lookup tables the VM needs.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use deusex_bytecode::{parse_package_functions, Function};
use deusex_package::{
    function_parameters, read_property_info, UPropertyInfo, UnrealPackage,
};
use glam::Vec3;

use crate::error::VmError;
use crate::value::{StructValue, Value};

/// A package prepared for script execution.
pub struct PackageUnit {
    pub package: Arc<UnrealPackage>,
    pub functions: Vec<Arc<Function>>,
    /// Function export reference -> index into [`Self::functions`].
    pub function_by_ref: HashMap<i32, usize>,
    /// Lower-case script path (`commandlet.main`) -> function index.
    pub function_by_path: HashMap<String, usize>,
    /// Lower-case short name (`main`) -> all functions with that name.
    pub functions_by_short_name: HashMap<String, Vec<usize>>,
    /// Native index -> full script path (`index 231 -> Object.Log`).
    pub native_names: HashMap<u32, String>,
    /// Export reference -> parsed property metadata.
    pub properties: HashMap<i32, UPropertyInfo>,
    /// Function export reference -> parameter chain in declaration order.
    pub parameters: HashMap<i32, Vec<UPropertyInfo>>,
    /// Diagnostics collected while building lookup tables.
    pub warnings: Vec<String>,
    paths: Vec<String>,
}

impl std::fmt::Debug for PackageUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PackageUnit")
            .field("functions", &self.functions.len())
            .field("properties", &self.properties.len())
            .field("natives", &self.native_names.len())
            .field("warnings", &self.warnings.len())
            .finish_non_exhaustive()
    }
}

impl PackageUnit {
    /// Load and prepare a `.u` package from disk.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, VmError> {
        let package = UnrealPackage::open(path)?;
        Self::from_package(package)
    }

    /// Prepare an already-parsed package.
    pub fn from_package(package: UnrealPackage) -> Result<Self, VmError> {
        let package = Arc::new(package);
        let parsed = parse_package_functions(&package)?;
        let functions: Vec<Arc<Function>> = parsed.into_iter().map(Arc::new).collect();

        let mut function_by_ref = HashMap::with_capacity(functions.len());
        let mut function_by_path = HashMap::with_capacity(functions.len());
        let mut functions_by_short_name: HashMap<String, Vec<usize>> = HashMap::new();
        let mut native_names: HashMap<u32, String> = HashMap::new();
        let mut paths = Vec::with_capacity(functions.len());
        let mut warnings = Vec::new();

        for (index, function) in functions.iter().enumerate() {
            function_by_ref.insert(function.export_ref, index);

            let path = package
                .object_path(function.export_ref)
                .unwrap_or_else(|| format!("{}.{}", function.outer_name, function.name));
            paths.push(path.clone());
            let key = path.to_ascii_lowercase();
            function_by_path.insert(key, index);

            if let Some(full_path) = package.full_object_path(function.export_ref) {
                function_by_path
                    .entry(full_path.to_ascii_lowercase())
                    .or_insert(index);
            }

            functions_by_short_name
                .entry(function.name.to_ascii_lowercase())
                .or_default()
                .push(index);

            if function.native_index > 0 {
                let native_index = function.native_index as u32;
                match native_names.get(&native_index) {
                    Some(existing) if !existing.eq_ignore_ascii_case(&path) => {
                        warnings.push(format!(
                            "native index {native_index} declared by both `{existing}` and `{path}`"
                        ));
                    }
                    None => {
                        native_names.insert(native_index, path.clone());
                    }
                    _ => {}
                }
            }
        }

        // Parse every property export once; runtime variable references are
        // object references that resolve back into this table.
        let mut properties = HashMap::new();
        for (reference, _) in package.exports().iter() {
            let Some(class_name) = package.class_name(reference) else {
                continue;
            };
            if !class_name.ends_with("Property") {
                continue;
            }
            match read_property_info(&package, reference) {
                Ok(info) => {
                    properties.insert(reference, info);
                }
                Err(error) => warnings.push(format!(
                    "failed to read property export {reference}: {error}"
                )),
            }
        }

        let mut parameters = HashMap::new();
        for function in &functions {
            match function_parameters(&package, function.children_ref) {
                Ok(list) => {
                    parameters.insert(function.export_ref, list);
                }
                Err(error) => warnings.push(format!(
                    "failed to read parameters of `{}`: {error}",
                    function.name
                )),
            }
        }

        Ok(Self {
            package,
            functions,
            function_by_ref,
            function_by_path,
            functions_by_short_name,
            native_names,
            properties,
            parameters,
            warnings,
            paths,
        })
    }

    pub fn function_index_by_ref(&self, reference: i32) -> Option<usize> {
        self.function_by_ref.get(&reference).copied()
    }

    /// Resolve a script path (`Core.Commandlet.Main`, `Commandlet.Main`) to a
    /// function index. Short names are accepted when unambiguous.
    pub fn find_function(&self, path: &str) -> Option<usize> {
        let key = path.to_ascii_lowercase();
        if let Some(index) = self.function_by_path.get(&key) {
            return Some(*index);
        }

        // Drop leading package components: `Core.Commandlet.Main` ->
        // `Commandlet.Main`.
        let parts: Vec<&str> = key.split('.').collect();
        for start in 1..parts.len() {
            let candidate = parts[start..].join(".");
            if let Some(index) = self.function_by_path.get(&candidate) {
                return Some(*index);
            }
        }

        let short = parts.last().copied()?;
        match self.functions_by_short_name.get(short) {
            Some(matches) if matches.len() == 1 => Some(matches[0]),
            _ => None,
        }
    }

    pub fn function_path(&self, index: usize) -> &str {
        self.paths.get(index).map(String::as_str).unwrap_or("")
    }

    pub fn parameters_of(&self, function_export: i32) -> &[UPropertyInfo] {
        self.parameters
            .get(&function_export)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn property(&self, reference: i32) -> Option<&UPropertyInfo> {
        self.properties.get(&reference)
    }

    pub fn native_name(&self, index: u32) -> Option<&str> {
        self.native_names.get(&index).map(String::as_str)
    }

    /// Default value for a variable reference, based on its property type.
    pub fn default_value(&self, reference: i32) -> Value {
        let Some(property) = self.property(reference) else {
            return Value::None;
        };
        match property.class_name.as_str() {
            "StructProperty" => {
                let type_name = self
                    .package
                    .object_name(property.type_ref)
                    .unwrap_or("")
                    .to_string();
                match type_name.to_ascii_lowercase().as_str() {
                    "vector" => Value::Vector(Vec3::ZERO),
                    "rotator" => Value::Rotator([0, 0, 0]),
                    _ => Value::Struct(StructValue {
                        type_name,
                        fields: Default::default(),
                    }),
                }
            }
            class_name => Value::default_for_property(class_name),
        }
    }

    /// Resolve an instance variable reference to a dotted property path.
    pub fn property_path(&self, reference: i32) -> Option<String> {
        self.package.object_path(reference)
    }
}
