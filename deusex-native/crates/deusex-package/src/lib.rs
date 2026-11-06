//! Unreal Engine 1 / Deus Ex package format parser.
//!
//! Parses `.u` (code/packages), `.utx` (textures), `.umx` (music),
//! `.uax` (audio), and `.dx` (maps) files.
//!
//! References:
//! - OpenDE `src/engine/package.cc`, `src/engine/reader.cc`
//! - UE1 `Core/Object/UnObj.cpp`, `Core/Object/UnClass.cpp`

use thiserror::Error;

pub mod archive;
pub mod export_table;
pub mod import_table;
pub mod name_table;
pub mod package;
pub mod property;
pub mod serialization;
pub mod summary;

pub use archive::{read_array_index, ArchiveReader};
pub use export_table::{
    ExportTable, ObjectExport, RF_HAS_STACK, RF_LOAD_FOR_CLIENT, RF_LOAD_FOR_EDIT,
    RF_LOAD_FOR_SERVER, RF_NATIVE, RF_NOT_FOR_CLIENT, RF_NOT_FOR_EDIT, RF_NOT_FOR_SERVER,
    RF_PUBLIC, RF_STANDALONE, RF_TRANSACTIONAL, RF_TRANSIENT,
};
pub use import_table::{ImportTable, ObjectImport};
pub use name_table::{NameEntry, NameTable};
pub use package::{inspect_package, UnrealPackage};
pub use property::{
    function_parameters, property_chain, read_property_info, property_flags, UPropertyInfo,
};
pub use serialization::{
    read_tagged_properties, skip_tagged_properties, PropertyType, PropertyValue, TaggedProperty,
};
pub use summary::{
    GenerationInfo, PackageSummary, DEUS_EX_FILE_VERSION, PACKAGE_MAGIC,
};

#[derive(Debug, Error)]
pub enum PackageError {
    #[error("invalid package magic: expected 0x9E2A83C1, got 0x{0:08X}")]
    InvalidMagic(u32),
    #[error("unsupported package version: {0}")]
    UnsupportedVersion(u16),
    #[error("unexpected end of package at offset 0x{offset:X}: needed {needed} bytes, {available} available")]
    UnexpectedEof {
        offset: usize,
        needed: usize,
        available: usize,
    },
    #[error("invalid name reference: {0}")]
    InvalidNameReference(i32),
    #[error("invalid import reference: {0}")]
    InvalidImportReference(i32),
    #[error("invalid export reference: {0}")]
    InvalidExportReference(i32),
    #[error("invalid object reference: {0}")]
    InvalidObjectReference(i32),
    #[error("invalid property tag '{name}' (type id {type_id}) at offset 0x{offset:X}")]
    InvalidPropertyTag {
        name: String,
        type_id: u8,
        offset: usize,
    },
    #[error("invalid export {reference}: {reason}")]
    InvalidExport {
        reference: i32,
        reason: &'static str,
    },
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
