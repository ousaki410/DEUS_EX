//! Import table parsing.
//!
//! `FObjectImport` for Deus Ex / UE1:
//! ```text
//! compact class_package (name index)
//! compact class_name    (name index)
//! i32     outer         (object reference)
//! compact object_name   (name index)
//! ```

use crate::archive::ArchiveReader;
use crate::summary::PackageSummary;
use crate::PackageError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectImport {
    pub class_package: i32,
    pub class_name: i32,
    pub outer: i32,
    pub object_name: i32,
}

#[derive(Debug, Clone, Default)]
pub struct ImportTable {
    entries: Vec<ObjectImport>,
}

impl ImportTable {
    pub fn read(
        reader: &mut ArchiveReader<'_>,
        summary: &PackageSummary,
    ) -> Result<Self, PackageError> {
        reader.seek(summary.import_offset as usize)?;
        let mut entries = Vec::with_capacity(summary.import_count as usize);
        for _ in 0..summary.import_count {
            entries.push(ObjectImport {
                class_package: reader.compact_index()?,
                class_name: reader.compact_index()?,
                outer: reader.i32()?,
                object_name: reader.compact_index()?,
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

    /// Import references are negative: -1 is the first import.
    pub fn get(&self, reference: i32) -> Option<&ObjectImport> {
        if reference >= 0 {
            return None;
        }
        self.entries.get((-reference - 1) as usize)
    }

    pub fn iter(&self) -> impl Iterator<Item = (i32, &ObjectImport)> {
        self.entries
            .iter()
            .enumerate()
            .map(|(index, entry)| (-(index as i32) - 1, entry))
    }

    pub fn as_slice(&self) -> &[ObjectImport] {
        &self.entries
    }
}
