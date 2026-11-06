//! Package file summary / header parsing.
//!
//! Reference: UE1 `FPackageFileSummary` as serialized by Deus Ex.
//!
//! Layout (version 68):
//! ```text
//! u32 tag
//! u16 file_version
//! u16 licensee_mode
//! u32 package_flags
//! u32 name_count
//! u32 name_offset
//! u32 export_count
//! u32 export_offset
//! u32 import_count
//! u32 import_offset
//! u8  guid[16]
//! u32 generation_count
//! { u32 export_count, u32 name_count } generations[]
//! ```
//!
//! Note that name/export/import counts come *before* their offsets; this
//! differs from later Unreal Engine versions.

use crate::archive::ArchiveReader;
use crate::PackageError;

pub const PACKAGE_MAGIC: u32 = 0x9E2A83C1;
pub const DEUS_EX_FILE_VERSION: u16 = 68;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenerationInfo {
    pub export_count: u32,
    pub name_count: u32,
}

#[derive(Debug, Clone)]
pub struct PackageSummary {
    pub tag: u32,
    pub file_version: u16,
    pub licensee_mode: u16,
    pub package_flags: u32,
    pub name_count: u32,
    pub name_offset: u32,
    pub export_count: u32,
    pub export_offset: u32,
    pub import_count: u32,
    pub import_offset: u32,
    pub guid: Option<[u8; 16]>,
    pub generations: Vec<GenerationInfo>,
    /// Offset immediately after the header (start of the name table).
    pub header_end: usize,
}

impl PackageSummary {
    pub fn read(reader: &mut ArchiveReader<'_>) -> Result<Self, PackageError> {
        let tag = reader.u32()?;
        if tag != PACKAGE_MAGIC {
            return Err(PackageError::InvalidMagic(tag));
        }

        let file_version = reader.u16()?;
        let licensee_mode = reader.u16()?;
        let package_flags = reader.u32()?;
        let name_count = reader.u32()?;
        let name_offset = reader.u32()?;
        let export_count = reader.u32()?;
        let export_offset = reader.u32()?;
        let import_count = reader.u32()?;
        let import_offset = reader.u32()?;

        let mut guid = None;
        let mut generations = Vec::new();
        if reader.remaining() >= 20 {
            let mut g = [0u8; 16];
            g.copy_from_slice(reader.read_bytes(16)?);
            guid = Some(g);
            let generation_count = reader.u32()?;
            if generation_count <= 1024 {
                for _ in 0..generation_count {
                    generations.push(GenerationInfo {
                        export_count: reader.u32()?,
                        name_count: reader.u32()?,
                    });
                }
            }
        }

        Ok(Self {
            tag,
            file_version,
            licensee_mode,
            package_flags,
            name_count,
            name_offset,
            export_count,
            export_offset,
            import_count,
            import_offset,
            guid,
            generations,
            header_end: reader.pos(),
        })
    }
}
