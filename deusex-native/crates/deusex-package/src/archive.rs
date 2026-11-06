//! Low-level reader for Unreal Engine 1 package archives.
//!
//! Deus Ex packages use a compact index encoding for name/import/export
//! references and for bytecode object references. See OpenDE's
//! `engine/reader.cc` for the reference implementation this matches.

use crate::PackageError;

/// A cursor over a package byte slice with UE1-aware primitive reads.
pub struct ArchiveReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> ArchiveReader<'a> {
    pub fn new(data: &'a [u8], pos: usize) -> Result<Self, PackageError> {
        if pos > data.len() {
            return Err(PackageError::UnexpectedEof {
                offset: pos,
                needed: 0,
                available: data.len().saturating_sub(pos.min(data.len())),
            });
        }
        Ok(Self { data, pos })
    }

    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn seek(&mut self, pos: usize) -> Result<(), PackageError> {
        if pos > self.data.len() {
            return Err(PackageError::UnexpectedEof {
                offset: pos,
                needed: 0,
                available: self.data.len(),
            });
        }
        self.pos = pos;
        Ok(())
    }

    fn require(&self, count: usize) -> Result<(), PackageError> {
        if self.remaining() < count {
            return Err(PackageError::UnexpectedEof {
                offset: self.pos,
                needed: count,
                available: self.remaining(),
            });
        }
        Ok(())
    }

    pub fn read_bytes(&mut self, count: usize) -> Result<&'a [u8], PackageError> {
        self.require(count)?;
        let start = self.pos;
        self.pos += count;
        Ok(&self.data[start..start + count])
    }

    pub fn skip(&mut self, count: usize) -> Result<(), PackageError> {
        self.require(count)?;
        self.pos += count;
        Ok(())
    }

    pub fn u8(&mut self) -> Result<u8, PackageError> {
        Ok(self.read_bytes(1)?[0])
    }

    pub fn i8(&mut self) -> Result<i8, PackageError> {
        Ok(self.u8()? as i8)
    }

    pub fn u16(&mut self) -> Result<u16, PackageError> {
        let b = self.read_bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn i16(&mut self) -> Result<i16, PackageError> {
        Ok(self.u16()? as i16)
    }

    pub fn u32(&mut self) -> Result<u32, PackageError> {
        let b = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn i32(&mut self) -> Result<i32, PackageError> {
        Ok(self.u32()? as i32)
    }

    pub fn u64(&mut self) -> Result<u64, PackageError> {
        let b = self.read_bytes(8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    pub fn f32(&mut self) -> Result<f32, PackageError> {
        Ok(f32::from_bits(self.u32()?))
    }

    /// Read a UE1 compact index.
    ///
    /// The first byte contains a sign bit (0x80), a continuation bit (0x40),
    /// and six value bits. Subsequent bytes hold seven value bits each with
    /// 0x80 as the continuation bit.
    pub fn compact_index(&mut self) -> Result<i32, PackageError> {
        let first = self.u8()?;
        let negative = (first & 0x80) != 0;
        let mut value = u32::from(first & 0x3f);
        if (first & 0x40) != 0 {
            let mut shift = 6u32;
            loop {
                let byte = self.u8()?;
                value |= u32::from(byte & 0x7f) << shift;
                shift += 7;
                if (byte & 0x80) == 0 || shift >= 32 {
                    break;
                }
            }
        }
        let value = value as i32;
        Ok(if negative { -value } else { value })
    }

    /// Read a UE1 FString (version 64+ uses a compact length prefix).
    pub fn fstring(&mut self, file_version: u16) -> Result<String, PackageError> {
        if file_version >= 64 {
            let len = self.compact_index()?;
            if len <= 0 {
                return Ok(String::new());
            }
            let bytes = self.read_bytes(len as usize)?;
            Ok(latin1_string(bytes))
        } else {
            let mut bytes = Vec::new();
            loop {
                let byte = self.u8()?;
                if byte == 0 {
                    break;
                }
                bytes.push(byte);
            }
            Ok(latin1_string(&bytes))
        }
    }

    /// Read a compact name/object index (identical encoding to `compact_index`).
    pub fn name_index(&mut self) -> Result<i32, PackageError> {
        self.compact_index()
    }
}

fn latin1_string(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    bytes[..end].iter().map(|&b| b as char).collect()
}

/// Read a property array index. This uses a different variable-length
/// encoding from the normal compact index:
///
/// * `0xxxxxxx`  -> 7-bit value
/// * `1xxxxxxx`  -> 15-bit value in two bytes
/// * `11xxxxxx`  -> 30-bit value in four bytes
pub fn read_array_index(reader: &mut ArchiveReader<'_>) -> Result<i32, PackageError> {
    let first = reader.u8()?;
    if (first & 0xc0) == 0xc0 {
        let b2 = reader.u8()?;
        let b3 = reader.u8()?;
        let b4 = reader.u8()?;
        Ok((i32::from(first & 0x3f) << 24)
            | (i32::from(b2) << 16)
            | (i32::from(b3) << 8)
            | i32::from(b4))
    } else if (first & 0x80) != 0 {
        let b2 = reader.u8()?;
        Ok((i32::from(first & 0x7f) << 8) | i32::from(b2))
    } else {
        Ok(i32::from(first))
    }
}

/// Compute the raw byte size implied by a property tag's size code.
pub fn property_tag_size(reader: &mut ArchiveReader<'_>, info: u8) -> Result<usize, PackageError> {
    Ok(match (info >> 4) & 0x07 {
        0 => 1,
        1 => 2,
        2 => 4,
        3 => 12,
        4 => 16,
        5 => usize::from(reader.u8()?),
        6 => usize::from(reader.i16()? as u16),
        _ => reader.u32()? as usize,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_index_examples() {
        let mut r = ArchiveReader::new(&[0x00], 0).unwrap();
        assert_eq!(r.compact_index().unwrap(), 0);

        // 0x1748, the export offset of Core.Commandlet.Main.
        let mut r = ArchiveReader::new(&[0x48, 0x5d], 0).unwrap();
        assert_eq!(r.compact_index().unwrap(), 0x1748);

        let mut r = ArchiveReader::new(&[0x81], 0).unwrap();
        assert_eq!(r.compact_index().unwrap(), -1);

        let mut r = ArchiveReader::new(&[0x40, 0x01], 0).unwrap();
        assert_eq!(r.compact_index().unwrap(), 64);
    }

    #[test]
    fn fstring_uses_compact_length() {
        let data = [0x06, b'H', b'e', b'l', b'l', b'o', 0];
        let mut r = ArchiveReader::new(&data, 0).unwrap();
        assert_eq!(r.fstring(68).unwrap(), "Hello");
    }
}
