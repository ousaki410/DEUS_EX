//! Complete package reader.
//!
//! This ties together the header, name table, import table and export table,
//! and provides random access to serialized object data.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::archive::ArchiveReader;
use crate::export_table::ExportTable;
use crate::import_table::ImportTable;
use crate::name_table::NameTable;
use crate::summary::PackageSummary;
use crate::PackageError;

#[derive(Debug)]
pub struct UnrealPackage {
    data: Vec<u8>,
    file_path: Option<PathBuf>,
    summary: PackageSummary,
    name_table: NameTable,
    import_table: ImportTable,
    export_table: ExportTable,
}

impl UnrealPackage {
    /// Open and parse a package file.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, PackageError> {
        let path = path.as_ref().to_path_buf();
        let mut file = File::open(&path)?;
        let mut package = Self::read(&mut file)?;
        package.file_path = Some(path);
        Ok(package)
    }

    /// Parse a package from any readable + seekable source.
    pub fn read<R: Read + Seek>(reader: &mut R) -> Result<Self, PackageError> {
        reader.seek(SeekFrom::Start(0))?;
        let mut data = Vec::new();
        reader.read_to_end(&mut data)?;
        Self::from_bytes(data)
    }

    pub fn from_bytes(data: Vec<u8>) -> Result<Self, PackageError> {
        let mut reader = ArchiveReader::new(&data, 0)?;
        let summary = PackageSummary::read(&mut reader)?;

        if summary.file_version < 60 {
            return Err(PackageError::UnsupportedVersion(summary.file_version));
        }

        let name_table = NameTable::read(&mut reader, &summary)?;
        let import_table = ImportTable::read(&mut reader, &summary)?;
        let export_table = ExportTable::read(&mut reader, &summary)?;

        for (reference, export) in export_table.iter() {
            if export.serial_size < 0 {
                return Err(PackageError::InvalidExport {
                    reference,
                    reason: "negative serialized size",
                });
            }
            if export.serial_size > 0 {
                let start = export.serial_offset as usize;
                let end = start
                    .checked_add(export.serial_size as usize)
                    .ok_or(PackageError::InvalidExport {
                        reference,
                        reason: "serialized range overflow",
                    })?;
                if end > data.len() {
                    return Err(PackageError::InvalidExport {
                        reference,
                        reason: "serialized range exceeds package size",
                    });
                }
            }
        }

        Ok(Self {
            data,
            file_path: None,
            summary,
            name_table,
            import_table,
            export_table,
        })
    }

    pub fn file_path(&self) -> Option<&Path> {
        self.file_path.as_deref()
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn summary(&self) -> &PackageSummary {
        &self.summary
    }

    pub fn names(&self) -> &NameTable {
        &self.name_table
    }

    pub fn imports(&self) -> &ImportTable {
        &self.import_table
    }

    pub fn exports(&self) -> &ExportTable {
        &self.export_table
    }

    pub fn name(&self, index: i32) -> Option<&str> {
        self.name_table.name(index)
    }

    /// Resolve an object reference to its object name.
    pub fn object_name(&self, reference: i32) -> Option<&str> {
        match reference {
            0 => Some("None"),
            r if r < 0 => {
                let import = self.import_table.get(r)?;
                self.name_table.name(import.object_name)
            }
            r => {
                let export = self.export_table.get(r)?;
                self.name_table.name(export.object_name)
            }
        }
    }

    /// Resolve an object reference to its class name.
    pub fn class_name(&self, reference: i32) -> Option<&str> {
        match reference {
            0 => Some("Class"),
            r if r < 0 => {
                let import = self.import_table.get(r)?;
                self.name_table.name(import.class_name)
            }
            r => {
                let export = self.export_table.get(r)?;
                if export.class_ref == 0 {
                    Some("Class")
                } else {
                    self.object_name(export.class_ref)
                }
            }
        }
    }

    /// Resolve the outer object of an import/export reference.
    pub fn outer_ref(&self, reference: i32) -> Option<i32> {
        match reference {
            0 => None,
            r if r < 0 => Some(self.import_table.get(r)?.outer),
            r => Some(self.export_table.get(r)?.outer),
        }
    }

    /// Build a dotted path such as `Core.Commandlet.Main`.
    pub fn object_path(&self, reference: i32) -> Option<String> {
        let mut parts = Vec::new();
        let mut current = reference;
        for _ in 0..64 {
            if current == 0 {
                break;
            }
            parts.push(self.object_name(current)?.to_string());
            current = self.outer_ref(current)?;
        }
        parts.reverse();
        Some(parts.join("."))
    }

    /// Full package-qualified path, including the package import name.
    pub fn full_object_path(&self, reference: i32) -> Option<String> {
        let mut path = self.object_path(reference)?;
        if let Some(export) = self.export_table.get(reference) {
            let _ = export;
            if let Some(package_import) = self.package_import_name() {
                path = format!("{package_import}.{path}");
            }
        }
        Some(path)
    }

    fn package_import_name(&self) -> Option<&str> {
        let (_, first) = self.import_table.iter().next()?;
        self.name_table.name(first.object_name)
    }

    /// Raw serialized data belonging to an export.
    pub fn export_data(&self, reference: i32) -> Result<&[u8], PackageError> {
        let export = self
            .export_table
            .get(reference)
            .ok_or(PackageError::InvalidExportReference(reference))?;
        if export.serial_size <= 0 {
            return Ok(&[]);
        }
        let start = export.serial_offset as usize;
        let end = start + export.serial_size as usize;
        Ok(&self.data[start..end])
    }

    /// Find an export by class and object name.
    pub fn find_export(&self, class_name: &str, object_name: &str) -> Option<i32> {
        for (reference, export) in self.export_table.iter() {
            let name = self.name_table.name(export.object_name).unwrap_or("");
            if !name.eq_ignore_ascii_case(object_name) {
                continue;
            }
            let class = self.class_name(reference).unwrap_or("");
            if class.eq_ignore_ascii_case(class_name) {
                return Some(reference);
            }
        }
        None
    }

    /// Find all exports of a given class.
    pub fn find_exports_by_class(&self, class_name: &str) -> Vec<i32> {
        self.export_table
            .iter()
            .filter_map(|(reference, _)| {
                let class = self.class_name(reference).unwrap_or("");
                class.eq_ignore_ascii_case(class_name).then_some(reference)
            })
            .collect()
    }

    /// All UFunction exports.
    pub fn function_exports(&self) -> Vec<i32> {
        self.find_exports_by_class("Function")
    }
}

/// Convenience function to open and print package info.
pub fn inspect_package<P: AsRef<Path>>(path: P) -> Result<(), PackageError> {
    let package = UnrealPackage::open(path)?;
    println!("Package: {}", package.file_path().map(|p| p.display().to_string()).unwrap_or_default());
    println!(
        "  tag={:#010X} version={} licensee={} flags={:#X}",
        package.summary().tag,
        package.summary().file_version,
        package.summary().licensee_mode,
        package.summary().package_flags
    );
    println!(
        "  names={} imports={} exports={}",
        package.names().len(),
        package.imports().len(),
        package.exports().len()
    );
    for (reference, export) in package.exports().iter().take(20) {
        println!(
            "  [{reference}] {:<32} class={:<16} flags={:#010X} size={} offset={:#X}",
            package.object_path(reference).unwrap_or_else(|| "?".into()),
            package.class_name(reference).unwrap_or("?"),
            export.object_flags,
            export.serial_size,
            export.serial_offset
        );
    }
    Ok(())
}
