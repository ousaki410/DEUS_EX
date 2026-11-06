//! `UProperty` object deserialization.
//!
//! In UE1/Deus Ex packages a `UProperty` export stores, after its tagged
//! property list:
//!
//! ```text
//! compact index   reserved (always 0 in Deus Ex packages)
//! compact index   next property in the chain (0 terminates)
//! i32             array dimension
//! u32             property flags
//! compact index   category (name index / object reference)
//! i16             replication offset (only when CPF_Net is set)
//! ...             type-specific payload
//! ```
//!
//! The layout matches OpenDE's `package_dump.cc` field reader. The reserved
//! field is parsed but not interpreted; omitting it shifts every following
//! field and produced nonsense dimensions in our probes.

use std::collections::HashSet;

use crate::archive::ArchiveReader;
use crate::serialization::skip_tagged_properties;
use crate::{PackageError, UnrealPackage};

/// Confirmed UE1 property flags used by the interpreter.
pub mod property_flags {
    /// Parameter of a function (`CPF_Parm`).
    pub const PARM: u32 = 0x0000_0080;
    /// Parameter is passed by reference (`CPF_OutParm`).
    pub const OUT_PARM: u32 = 0x0000_0100;
    /// Parameter is skipped in the signature (`CPF_SkipParm`).
    pub const SKIP_PARM: u32 = 0x0000_0200;
    /// Parameter represents the return value (`CPF_ReturnParm`).
    pub const RETURN_PARM: u32 = 0x0000_0400;
    /// Parameter is automatically converted (`CPF_CoerceParm`).
    pub const COERCE_PARM: u32 = 0x0000_0800;
    /// Property is replicated (`CPF_Net`).
    pub const NET: u32 = 0x0000_0020;
}

/// Deserialized metadata for one `UProperty` export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UPropertyInfo {
    /// Export reference of the property object.
    pub reference: i32,
    /// Property name (e.g. `Parms`).
    pub name: String,
    /// UClass name (e.g. `StrProperty`, `IntProperty`).
    pub class_name: String,
    /// Next property in the declaration chain; `0` terminates.
    pub next: i32,
    /// Fixed array dimension (`1` for scalars).
    pub array_dim: i32,
    /// Raw `CPF_*` flags.
    pub flags: u32,
    /// Category name/reference.
    pub category_ref: i32,
    /// Type payload: struct/object/byte/array element reference.
    pub type_ref: i32,
    /// Metaclass reference for `ClassProperty`.
    pub meta_ref: i32,
    /// Element count for `FixedArrayProperty`.
    pub fixed_count: i32,
    /// Replication offset when `CPF_Net` is set.
    pub replication_offset: Option<i16>,
}

impl UPropertyInfo {
    pub fn is_parm(&self) -> bool {
        self.flags & property_flags::PARM != 0
    }

    pub fn is_out_parm(&self) -> bool {
        self.flags & property_flags::OUT_PARM != 0
    }

    pub fn is_return_parm(&self) -> bool {
        self.flags & property_flags::RETURN_PARM != 0
    }

    pub fn is_coerce_parm(&self) -> bool {
        self.flags & property_flags::COERCE_PARM != 0
    }

    /// `true` when this property is a function parameter that is not the
    /// implicit return value slot.
    pub fn is_input_parm(&self) -> bool {
        self.is_parm() && !self.is_return_parm()
    }
}

/// Skip the optional stack header that precedes some export payloads.
fn skip_stack_header(reader: &mut ArchiveReader<'_>) -> Result<(), PackageError> {
    let node = reader.compact_index()?;
    let _state = reader.compact_index()?;
    let _probe = reader.u64()?;
    let _latent = reader.i32()?;
    if node != 0 {
        let _offset = reader.compact_index()?;
    }
    Ok(())
}

/// Read one `UProperty` export.
pub fn read_property_info(
    package: &UnrealPackage,
    reference: i32,
) -> Result<UPropertyInfo, PackageError> {
    let export = package
        .exports()
        .get(reference)
        .ok_or(PackageError::InvalidExportReference(reference))?;
    let class_name = package.class_name(reference).unwrap_or("").to_string();
    let name = package
        .object_name(reference)
        .unwrap_or("<unnamed>")
        .to_string();

    let mut reader = ArchiveReader::new(package.data(), export.serial_offset as usize)?;
    if export.has_stack() {
        skip_stack_header(&mut reader)?;
    }
    if export.class_ref != 0 {
        skip_tagged_properties(&mut reader, package.names())?;
    }

    // Reserved field; always zero in the packages we inspected.
    let _reserved = reader.compact_index()?;
    let next = reader.compact_index()?;
    let array_dim = reader.i32()?;
    let flags = reader.u32()?;
    let category_ref = reader.compact_index()?;
    let replication_offset = if flags & property_flags::NET != 0 {
        Some(reader.i16()?)
    } else {
        None
    };

    let mut type_ref = 0;
    let mut meta_ref = 0;
    let mut fixed_count = 0;
    match class_name.as_str() {
        "ByteProperty" | "ObjectProperty" | "StructProperty" => {
            type_ref = reader.compact_index()?;
        }
        "ClassProperty" => {
            type_ref = reader.compact_index()?;
            meta_ref = reader.compact_index()?;
        }
        "ArrayProperty" => {
            type_ref = reader.compact_index()?;
        }
        "FixedArrayProperty" => {
            type_ref = reader.compact_index()?;
            fixed_count = reader.i32()?;
        }
        _ => {}
    }

    Ok(UPropertyInfo {
        reference,
        name,
        class_name,
        next,
        array_dim,
        flags,
        category_ref,
        type_ref,
        meta_ref,
        fixed_count,
        replication_offset,
    })
}

/// Follow a property chain starting at `reference` until `next` is `0`.
///
/// Cycles are broken defensively; malformed packages should not hang the
/// loader.
pub fn property_chain(
    package: &UnrealPackage,
    reference: i32,
) -> Result<Vec<UPropertyInfo>, PackageError> {
    let mut chain = Vec::new();
    let mut seen = HashSet::new();
    let mut current = reference;
    while current > 0 && seen.insert(current) {
        let info = read_property_info(package, current)?;
        current = info.next;
        chain.push(info);
    }
    Ok(chain)
}

/// Read the parameter list of a function from its `Children` reference.
pub fn function_parameters(
    package: &UnrealPackage,
    children_ref: i32,
) -> Result<Vec<UPropertyInfo>, PackageError> {
    Ok(property_chain(package, children_ref)?
        .into_iter()
        .filter(UPropertyInfo::is_parm)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_helpers() {
        let mut info = UPropertyInfo {
            reference: 1,
            name: "A".into(),
            class_name: "IntProperty".into(),
            next: 0,
            array_dim: 1,
            flags: property_flags::PARM | property_flags::OUT_PARM,
            category_ref: 0,
            type_ref: 0,
            meta_ref: 0,
            fixed_count: 0,
            replication_offset: None,
        };
        assert!(info.is_parm());
        assert!(info.is_out_parm());
        assert!(!info.is_return_parm());
        assert!(info.is_input_parm());

        info.flags = property_flags::PARM | property_flags::RETURN_PARM;
        assert!(!info.is_input_parm());
        assert!(info.is_return_parm());
    }
}
