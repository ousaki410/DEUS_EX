//! Name table parsing.
//!
//! Each entry is an FString followed by a 32-bit flags word. In Deus Ex
//! (file version 68) the FString has a compact length prefix. There is no
//! additional hash-chain field on disk.

use crate::archive::ArchiveReader;
use crate::summary::PackageSummary;
use crate::PackageError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameEntry {
    pub name: String,
    pub flags: u32,
}

#[derive(Debug, Clone, Default)]
pub struct NameTable {
    entries: Vec<NameEntry>,
}

impl NameTable {
    pub fn read(
        reader: &mut ArchiveReader<'_>,
        summary: &PackageSummary,
    ) -> Result<Self, PackageError> {
        reader.seek(summary.name_offset as usize)?;
        let mut entries = Vec::with_capacity(summary.name_count as usize);
        for _ in 0..summary.name_count {
            let name = reader.fstring(summary.file_version)?;
            let flags = reader.u32()?;
            entries.push(NameEntry { name, flags });
        }
        Ok(Self { entries })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Names are stored with index 0 == "None".
    pub fn get(&self, index: i32) -> Option<&NameEntry> {
        if index < 0 {
            return None;
        }
        self.entries.get(index as usize)
    }

    pub fn name(&self, index: i32) -> Option<&str> {
        self.get(index).map(|entry| entry.name.as_str())
    }

    pub fn name_or_none(&self, index: i32) -> &str {
        self.name(index).unwrap_or("<invalid-name>")
    }

    pub fn iter(&self) -> impl Iterator<Item = (usize, &NameEntry)> {
        self.entries.iter().enumerate()
    }

    pub fn find_index(&self, name: &str) -> Option<i32> {
        self.entries
            .iter()
            .position(|entry| entry.name.eq_ignore_ascii_case(name))
            .map(|index| index as i32)
    }

    pub fn none_index(&self) -> i32 {
        self.find_index("None").unwrap_or(0)
    }

    pub fn as_slice(&self) -> &[NameEntry] {
        &self.entries
    }
}
