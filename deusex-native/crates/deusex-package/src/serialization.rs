//! Tagged property serialization.
//!
//! Every non-`Class` object starts with a tagged property list, and
//! `UClass` objects end with their default-property list. Each tag is:
//!
//! ```text
//! compact name_index
//! u8      info       (low nibble: type, bits 4..6: size code, bit 7: bool value / array flag)
//! [compact struct_name]  when type == StructProperty
//! [size bytes]           when size code == 5
//! [u16 size]             when size code == 6
//! [u32 size]             when size code == 7
//! [array_index]          when type != Bool and the high bit is set
//! value bytes
//! ```
//!
//! The list terminates with the name `None`.

use crate::archive::{property_tag_size, read_array_index, ArchiveReader};
use crate::name_table::NameTable;
use crate::PackageError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PropertyType {
    Byte = 1,
    Int = 2,
    Bool = 3,
    Float = 4,
    Object = 5,
    Name = 6,
    String = 7,
    Class = 8,
    Array = 9,
    Struct = 10,
    Vector = 11,
    Rotator = 12,
    Str = 13,
    Map = 14,
    FixedArray = 15,
}

impl PropertyType {
    pub fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            1 => Self::Byte,
            2 => Self::Int,
            3 => Self::Bool,
            4 => Self::Float,
            5 => Self::Object,
            6 => Self::Name,
            7 => Self::String,
            8 => Self::Class,
            9 => Self::Array,
            10 => Self::Struct,
            11 => Self::Vector,
            12 => Self::Rotator,
            13 => Self::Str,
            14 => Self::Map,
            15 => Self::FixedArray,
            _ => return None,
        })
    }

    pub fn is_bool(self) -> bool {
        matches!(self, Self::Bool)
    }
}

#[derive(Debug, Clone)]
pub enum PropertyValue {
    Bool(bool),
    Byte(u8),
    Int(i32),
    Float(f32),
    ObjectRef(i32),
    NameIndex(i32),
    String(String),
    Vector([f32; 3]),
    Rotator([i32; 3]),
    Struct { name: String, raw: Vec<u8> },
    Raw(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct TaggedProperty {
    pub name: String,
    pub property_type: PropertyType,
    pub struct_name: Option<String>,
    pub array_index: i32,
    pub size: usize,
    pub value: PropertyValue,
}

/// Skip a tagged property list without interpreting values.
pub fn skip_tagged_properties(
    reader: &mut ArchiveReader<'_>,
    names: &NameTable,
) -> Result<(), PackageError> {
    loop {
        let name_index = reader.compact_index()?;
        let name = names.name(name_index).unwrap_or("");
        if name.eq_ignore_ascii_case("None") || name_index == 0 {
            return Ok(());
        }

        let info = reader.u8()?;
        let Some(property_type) = PropertyType::from_u8(info & 0x0f) else {
            return Err(PackageError::InvalidPropertyTag {
                name: name.to_string(),
                type_id: info & 0x0f,
                offset: reader.pos().saturating_sub(1),
            });
        };

        if property_type == PropertyType::Struct {
            reader.compact_index()?;
        }

        let mut size = property_tag_size(reader, info)?;
        if property_type != PropertyType::Bool && (info & 0x80) != 0 {
            read_array_index(reader)?;
        }
        if property_type == PropertyType::Bool {
            size = 0;
        }
        reader.skip(size)?;
    }
}

/// Read a tagged property list.
pub fn read_tagged_properties(
    reader: &mut ArchiveReader<'_>,
    names: &NameTable,
) -> Result<Vec<TaggedProperty>, PackageError> {
    let mut properties = Vec::new();
    loop {
        let tag_offset = reader.pos();
        let name_index = reader.compact_index()?;
        let name = names.name(name_index).unwrap_or("");
        if name.eq_ignore_ascii_case("None") || name_index == 0 {
            return Ok(properties);
        }

        let info = reader.u8()?;
        let Some(property_type) = PropertyType::from_u8(info & 0x0f) else {
            return Err(PackageError::InvalidPropertyTag {
                name: name.to_string(),
                type_id: info & 0x0f,
                offset: tag_offset,
            });
        };

        let struct_name = if property_type == PropertyType::Struct {
            let index = reader.compact_index()?;
            Some(names.name_or_none(index).to_string())
        } else {
            None
        };

        let mut size = property_tag_size(reader, info)?;
        let mut array_index = 0;
        if property_type != PropertyType::Bool && (info & 0x80) != 0 {
            array_index = read_array_index(reader)?;
        }

        let mut bool_value = false;
        if property_type == PropertyType::Bool {
            bool_value = (info & 0x80) != 0;
            size = 0;
        }

        let raw = reader.read_bytes(size)?.to_vec();
        let value = decode_property_value(
            property_type,
            struct_name.as_deref(),
            bool_value,
            &raw,
        )?;

        properties.push(TaggedProperty {
            name: name.to_string(),
            property_type,
            struct_name,
            array_index,
            size,
            value,
        });
    }
}

fn decode_property_value(
    property_type: PropertyType,
    struct_name: Option<&str>,
    bool_value: bool,
    raw: &[u8],
) -> Result<PropertyValue, PackageError> {
    let mut r = ArchiveReader::new(raw, 0)?;
    let value = match property_type {
        PropertyType::Bool => PropertyValue::Bool(bool_value),
        PropertyType::Byte => PropertyValue::Byte(r.u8()?),
        PropertyType::Int => PropertyValue::Int(r.i32()?),
        PropertyType::Float => PropertyValue::Float(r.f32()?),
        PropertyType::Object | PropertyType::Class => PropertyValue::ObjectRef(r.compact_index()?),
        PropertyType::Name => PropertyValue::NameIndex(r.compact_index()?),
        PropertyType::String | PropertyType::Str => PropertyValue::String(r.fstring(68)?),
        PropertyType::Vector => {
            let v = [r.f32()?, r.f32()?, r.f32()?];
            PropertyValue::Vector(v)
        }
        PropertyType::Rotator => {
            let v = [r.i32()?, r.i32()?, r.i32()?];
            PropertyValue::Rotator(v)
        }
        PropertyType::Struct => {
            let name = struct_name.unwrap_or("<struct>").to_string();
            match name.to_ascii_lowercase().as_str() {
                "vector" if raw.len() == 12 => {
                    let mut v = ArchiveReader::new(raw, 0)?;
                    PropertyValue::Vector([v.f32()?, v.f32()?, v.f32()?])
                }
                "rotator" if raw.len() == 12 => {
                    let mut v = ArchiveReader::new(raw, 0)?;
                    PropertyValue::Rotator([v.i32()?, v.i32()?, v.i32()?])
                }
                _ => PropertyValue::Struct {
                    name,
                    raw: raw.to_vec(),
                },
            }
        }
        PropertyType::Array | PropertyType::Map | PropertyType::FixedArray => {
            PropertyValue::Raw(raw.to_vec())
        }
    };
    Ok(value)
}
