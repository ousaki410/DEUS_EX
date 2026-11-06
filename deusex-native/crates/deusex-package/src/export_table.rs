//! Export table parsing.
//!
//! `FObjectExport` for Deus Ex / UE1:
//! ```text
//! compact class_ref   (object reference)
//! compact super_ref   (object reference; usually None in Deus Ex)
//! i32     outer       (object reference)
//! compact object_name (name index)
//! u32     object_flags
//! compact serial_size
//! compact serial_offset  (only present when serial_size > 0)
//! ```

use crate::archive::ArchiveReader;
use crate::summary::PackageSummary;
use crate::PackageError;

// Object flags observed in Deus Ex packages.
pub const RF_TRANSACTIONAL: u32 = 0x0000_0001;
pub const RF_PUBLIC: u32 = 0x0000_0004;
pub const RF_TRANSIENT: u32 = 0x0000_4000;
pub const RF_LOAD_FOR_CLIENT: u32 = 0x0001_0000;
pub const RF_LOAD_FOR_SERVER: u32 = 0x0002_0000;
pub const RF_LOAD_FOR_EDIT: u32 = 0x0004_0000;
pub const RF_STANDALONE: u32 = 0x0008_0000;
pub const RF_NOT_FOR_CLIENT: u32 = 0x0010_0000;
pub const RF_NOT_FOR_SERVER: u32 = 0x0020_0000;
pub const RF_NOT_FOR_EDIT: u32 = 0x0040_0000;
pub const RF_HAS_STACK: u32 = 0x0200_0000;
pub const RF_NATIVE: u32 = 0x0400_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectExport {
    pub class_ref: i32,
    pub super_ref: i32,
    pub outer: i32,
    pub object_name: i32,
    pub object_flags: u32,
    pub serial_size: i32,
    pub serial_offset: i32,
}

impl ObjectExport {
    pub fn has_stack(&self) -> bool {
        (self.object_flags & RF_HAS_STACK) != 0
    }

    pub fn has_data(&self) -> bool {
        self.serial_size > 0
    }
}

#[derive(Debug, Clone, Default)]
pub struct ExportTable {
    entries: Vec<ObjectExport>,
}

impl ExportTable {
    pub fn read(
        reader: &mut ArchiveReader<'_>,
        summary: &PackageSummary,
    ) -> Result<Self, PackageError> {
        reader.seek(summary.export_offset as usize)?;
        let mut entries = Vec::with_capacity(summary.export_count as usize);
        for _ in 0..summary.export_count {
            let class_ref = reader.compact_index()?;
            let super_ref = reader.compact_index()?;
            let outer = reader.i32()?;
            let object_name = reader.compact_index()?;
            let object_flags = reader.u32()?;
            let serial_size = reader.compact_index()?;
            let serial_offset = if serial_size > 0 {
                reader.compact_index()?
            } else {
                0
            };
            entries.push(ObjectExport {
                class_ref,
                super_ref,
                outer,
                object_name,
                object_flags,
                serial_size,
                serial_offset,
            });
        }
        Ok(Self { entries })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Export references are positive and 1-based.
    pub fn get(&self, reference: i32) -> Option<&ObjectExport> {
        if reference <= 0 {
            return None;
        }
        self.entries.get((reference - 1) as usize)
    }

    pub fn iter(&self) -> impl Iterator<Item = (i32, &ObjectExport)> {
        self.entries
            .iter()
            .enumerate()
            .map(|(index, entry)| (index as i32 + 1, entry))
    }

    pub fn as_slice(&self) -> &[ObjectExport] {
        &self.entries
    }
}
