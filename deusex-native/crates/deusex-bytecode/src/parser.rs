//! Function extraction and bytecode decoding.
//!
//! A `UFunction` object is serialized as:
//! ```text
//! [stack header if RF_HasStack]
//! tagged properties (terminated by None)
//! UStruct fields:
//!   super_ref, next_ref, script_text_ref, children_ref,
//!   friendly_name, line, text_pos, script_size, script_bytes
//! UFunction fields:
//!   native_index u16, precedence u8, function_flags u32
//!   [replication_offset u16 if FUNC_Net]
//! ```
//!
//! `script_size` counts *normalized* bytes: compact object/name references
//! occupy four bytes in the normalized stream. The raw compact stream is
//! shorter. Jump and case offsets use the normalized coordinate system.

use deusex_package::{
    skip_tagged_properties, ArchiveReader, PackageError, UnrealPackage,
};

use crate::function::{Expr, ExprKind, Function};
use crate::instructions::Token;

#[derive(Debug, thiserror::Error)]
pub enum BytecodeError {
    #[error(transparent)]
    Package(#[from] PackageError),
    #[error("export {0} is not a Function")]
    NotFunction(i32),
    #[error("unknown bytecode token 0x{token:02X} at normalized offset {norm_offset} (raw 0x{raw_offset:X})")]
    UnknownToken {
        token: u8,
        norm_offset: u32,
        raw_offset: usize,
    },
    #[error("unexpected end of script at raw offset 0x{raw_offset:X} (normalized offset {norm_offset})")]
    UnexpectedEnd {
        raw_offset: usize,
        norm_offset: u32,
    },
    #[error("script size mismatch for {function}: expected {expected} normalized bytes, consumed {actual}")]
    ScriptSizeMismatch {
        function: String,
        expected: u32,
        actual: u32,
    },
    #[error("nesting too deep at normalized offset {0}")]
    NestingTooDeep(u32),
}

type Result<T> = std::result::Result<T, BytecodeError>;

struct ScriptReader<'a> {
    data: &'a [u8],
    raw: usize,
    norm: u32,
    limit: u32,
}

impl<'a> ScriptReader<'a> {
    fn new(data: &'a [u8], limit: u32) -> Self {
        Self {
            data,
            raw: 0,
            norm: 0,
            limit,
        }
    }

    fn read_u8_raw(&mut self) -> Result<u8> {
        if self.raw >= self.data.len() {
            return Err(BytecodeError::UnexpectedEnd {
                raw_offset: self.raw,
                norm_offset: self.norm,
            });
        }
        let value = self.data[self.raw];
        self.raw += 1;
        self.norm += 1;
        Ok(value)
    }

    fn read_u16_raw(&mut self) -> Result<u16> {
        if self.raw + 2 > self.data.len() {
            return Err(BytecodeError::UnexpectedEnd {
                raw_offset: self.raw,
                norm_offset: self.norm,
            });
        }
        let value = u16::from_le_bytes([self.data[self.raw], self.data[self.raw + 1]]);
        self.raw += 2;
        self.norm += 2;
        Ok(value)
    }

    fn read_u32_raw(&mut self) -> Result<u32> {
        if self.raw + 4 > self.data.len() {
            return Err(BytecodeError::UnexpectedEnd {
                raw_offset: self.raw,
                norm_offset: self.norm,
            });
        }
        let value = u32::from_le_bytes([
            self.data[self.raw],
            self.data[self.raw + 1],
            self.data[self.raw + 2],
            self.data[self.raw + 3],
        ]);
        self.raw += 4;
        self.norm += 4;
        Ok(value)
    }

    fn read_f32_raw(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.read_u32_raw()?))
    }

    fn read_compact_index(&mut self) -> Result<i32> {
        let start_norm = self.norm;
        let first = self.read_u8_raw()?;
        let negative = (first & 0x80) != 0;
        let mut value = u32::from(first & 0x3f);
        if (first & 0x40) != 0 {
            let mut shift = 6u32;
            loop {
                let byte = self.read_u8_raw()?;
                value |= u32::from(byte & 0x7f) << shift;
                shift += 7;
                if (byte & 0x80) == 0 || shift >= 32 {
                    break;
                }
            }
        }
        // Object/name references occupy exactly four bytes once normalized,
        // regardless of how many compact bytes they used on disk.
        self.norm = start_norm.saturating_add(4);
        let value = value as i32;
        Ok(if negative { -value } else { value })
    }

    fn read_object_ref(&mut self) -> Result<i32> {
        self.read_compact_index()
    }

    fn read_name_ref(&mut self) -> Result<i32> {
        self.read_compact_index()
    }

    fn parse_expr(&mut self, depth: usize) -> Result<Expr> {
        if depth > 128 {
            return Err(BytecodeError::NestingTooDeep(self.norm));
        }
        if self.norm >= self.limit {
            return Err(BytecodeError::UnexpectedEnd {
                raw_offset: self.raw,
                norm_offset: self.norm,
            });
        }

        let start_norm = self.norm;
        let start_raw = self.raw;
        let token = Token::new(self.read_u8_raw()?);

        let kind = if token.is_conversion() {
            let value = self.parse_expr(depth + 1)?;
            ExprKind::Conversion {
                opcode: token.0,
                value: Box::new(value),
            }
        } else if token.is_native() {
            let params = self.parse_params(depth + 1)?;
            ExprKind::Native {
                index: token.0,
                params,
            }
        } else if token.is_extended_native() {
            // `EX_ExtendedNative` tokens (0x60..0x6F) store the high nibble of
            // the native index in the opcode and the low byte afterwards,
            // e.g. `0x61 0x30` -> native index 0x130 (Actor.AllActors).
            let low = u16::from(self.read_u8_raw()?);
            let index = (u16::from(token.0 & 0x0F) << 8) | low;
            let params = self.parse_params(depth + 1)?;
            ExprKind::ExtendedNative { index, params }
        } else {
            match token {
                Token::LOCAL_VARIABLE => ExprKind::LocalVariable(self.read_object_ref()?),
                Token::INSTANCE_VARIABLE => ExprKind::InstanceVariable(self.read_object_ref()?),
                Token::DEFAULT_VARIABLE => ExprKind::DefaultVariable(self.read_object_ref()?),
                Token::RETURN => ExprKind::Return(Box::new(self.parse_expr(depth + 1)?)),
                Token::SWITCH => {
                    let byte = self.read_u8_raw()?;
                    let value = self.parse_expr(depth + 1)?;
                    ExprKind::Switch(byte, Box::new(value))
                }
                Token::JUMP => ExprKind::Jump(self.read_u16_raw()?),
                Token::JUMP_IF_NOT => {
                    let offset = self.read_u16_raw()?;
                    let condition = self.parse_expr(depth + 1)?;
                    ExprKind::JumpIfNot {
                        offset,
                        condition: Box::new(condition),
                    }
                }
                Token::STOP => ExprKind::Stop,
                Token::ASSERT => {
                    let offset = self.read_u16_raw()?;
                    let condition = self.parse_expr(depth + 1)?;
                    ExprKind::Assert {
                        offset,
                        condition: Box::new(condition),
                    }
                }
                Token::CASE => {
                    let offset = self.read_u16_raw()?;
                    let value = if offset != 0xffff {
                        Some(Box::new(self.parse_expr(depth + 1)?))
                    } else {
                        None
                    };
                    ExprKind::Case { offset, value }
                }
                Token::NOTHING => ExprKind::Nothing,
                Token::LABEL_TABLE => {
                    let mut labels = Vec::new();
                    loop {
                        let name = self.read_name_ref()?;
                        let offset = self.read_u32_raw()?;
                        labels.push((name, offset));
                        if name == 0 {
                            break;
                        }
                    }
                    ExprKind::LabelTable(labels)
                }
                Token::GOTO_LABEL => ExprKind::GotoLabel(Box::new(self.parse_expr(depth + 1)?)),
                Token::EAT_STRING => ExprKind::EatString(Box::new(self.parse_expr(depth + 1)?)),
                Token::LET => {
                    let target = self.parse_expr(depth + 1)?;
                    let value = self.parse_expr(depth + 1)?;
                    ExprKind::Let {
                        target: Box::new(target),
                        value: Box::new(value),
                    }
                }
                Token::DYN_ARRAY_ELEMENT => {
                    let array = self.parse_expr(depth + 1)?;
                    let index = self.parse_expr(depth + 1)?;
                    ExprKind::DynArrayElement {
                        array: Box::new(array),
                        index: Box::new(index),
                    }
                }
                Token::NEW => {
                    let mut items = Vec::with_capacity(4);
                    for _ in 0..4 {
                        items.push(self.parse_expr(depth + 1)?);
                    }
                    ExprKind::New(items)
                }
                Token::CLASS_CONTEXT => {
                    let context = self.parse_expr(depth + 1)?;
                    let offset = self.read_u16_raw()?;
                    let byte = self.read_u8_raw()?;
                    let value = self.parse_expr(depth + 1)?;
                    ExprKind::ClassContext {
                        context: Box::new(context),
                        offset,
                        byte,
                        value: Box::new(value),
                    }
                }
                Token::META_CAST => {
                    let class = self.read_object_ref()?;
                    let value = self.parse_expr(depth + 1)?;
                    ExprKind::MetaCast {
                        class,
                        value: Box::new(value),
                    }
                }
                Token::LET_BOOL => {
                    let target = self.parse_expr(depth + 1)?;
                    let value = self.parse_expr(depth + 1)?;
                    ExprKind::LetBool {
                        target: Box::new(target),
                        value: Box::new(value),
                    }
                }
                Token::UNKNOWN_15 => ExprKind::Unknown15,
                Token::END_FUNCTION_PARMS => ExprKind::EndFunctionParms,
                Token::SELF_ => ExprKind::SelfObject,
                Token::SKIP => {
                    let offset = self.read_u16_raw()?;
                    let value = self.parse_expr(depth + 1)?;
                    ExprKind::Skip {
                        offset,
                        value: Box::new(value),
                    }
                }
                Token::CONTEXT => {
                    let context = self.parse_expr(depth + 1)?;
                    let offset = self.read_u16_raw()?;
                    let byte = self.read_u8_raw()?;
                    let value = self.parse_expr(depth + 1)?;
                    ExprKind::Context {
                        context: Box::new(context),
                        offset,
                        byte,
                        value: Box::new(value),
                    }
                }
                Token::ARRAY_ELEMENT => {
                    let array = self.parse_expr(depth + 1)?;
                    let index = self.parse_expr(depth + 1)?;
                    ExprKind::ArrayElement {
                        array: Box::new(array),
                        index: Box::new(index),
                    }
                }
                Token::VIRTUAL_FUNCTION => {
                    let name = self.read_name_ref()?;
                    let params = self.parse_params(depth + 1)?;
                    ExprKind::VirtualFunction { name, params }
                }
                Token::FINAL_FUNCTION => {
                    let function = self.read_object_ref()?;
                    let params = self.parse_params(depth + 1)?;
                    ExprKind::FinalFunction { function, params }
                }
                Token::INT_CONST => ExprKind::IntConst(self.read_u32_raw()? as i32),
                Token::FLOAT_CONST => ExprKind::FloatConst(self.read_f32_raw()?),
                Token::STRING_CONST => {
                    let mut bytes = Vec::new();
                    loop {
                        let byte = self.read_u8_raw()?;
                        if byte == 0 {
                            break;
                        }
                        bytes.push(byte);
                    }
                    ExprKind::StringConst(
                        bytes.iter().map(|&b| b as char).collect::<String>(),
                    )
                }
                Token::OBJECT_CONST => ExprKind::ObjectConst(self.read_object_ref()?),
                Token::NAME_CONST => ExprKind::NameConst(self.read_name_ref()?),
                Token::ROTATION_CONST => ExprKind::RotationConst([
                    self.read_f32_raw()?,
                    self.read_f32_raw()?,
                    self.read_f32_raw()?,
                ]),
                Token::VECTOR_CONST => ExprKind::VectorConst([
                    self.read_f32_raw()?,
                    self.read_f32_raw()?,
                    self.read_f32_raw()?,
                ]),
                Token::BYTE_CONST => ExprKind::ByteConst(self.read_u8_raw()?),
                Token::INT_ZERO => ExprKind::IntZero,
                Token::INT_ONE => ExprKind::IntOne,
                Token::TRUE => ExprKind::True,
                Token::FALSE => ExprKind::False,
                Token::NATIVE_PARM => ExprKind::NativeParm(self.read_object_ref()?),
                Token::NO_OBJECT => ExprKind::NoObject,
                Token::UNKNOWN_2B => {
                    let byte = self.read_u8_raw()?;
                    let value = self.parse_expr(depth + 1)?;
                    ExprKind::Unknown2B {
                        byte,
                        value: Box::new(value),
                    }
                }
                Token::INT_CONST_BYTE => ExprKind::IntConstByte(self.read_u8_raw()?),
                Token::BOOL_VARIABLE => {
                    ExprKind::BoolVariable(Box::new(self.parse_expr(depth + 1)?))
                }
                Token::DYNAMIC_CAST => {
                    let class = self.read_object_ref()?;
                    let value = self.parse_expr(depth + 1)?;
                    ExprKind::DynamicCast {
                        class,
                        value: Box::new(value),
                    }
                }
                Token::ITERATOR => {
                    let value = self.parse_expr(depth + 1)?;
                    let offset = self.read_u16_raw()?;
                    ExprKind::Iterator {
                        value: Box::new(value),
                        offset,
                    }
                }
                Token::ITERATOR_POP => ExprKind::IteratorPop,
                Token::ITERATOR_NEXT => ExprKind::IteratorNext,
                Token::STRUCT_CMP_EQ => {
                    let struct_ref = self.read_object_ref()?;
                    let left = self.parse_expr(depth + 1)?;
                    let right = self.parse_expr(depth + 1)?;
                    ExprKind::StructCmpEq {
                        struct_ref,
                        left: Box::new(left),
                        right: Box::new(right),
                    }
                }
                Token::STRUCT_CMP_NE => {
                    let struct_ref = self.read_object_ref()?;
                    let left = self.parse_expr(depth + 1)?;
                    let right = self.parse_expr(depth + 1)?;
                    ExprKind::StructCmpNe {
                        struct_ref,
                        left: Box::new(left),
                        right: Box::new(right),
                    }
                }
                Token::UNICODE_STRING_CONST => {
                    let mut units = Vec::new();
                    loop {
                        let unit = self.read_u16_raw()?;
                        if unit == 0 {
                            break;
                        }
                        units.push(unit);
                    }
                    ExprKind::UnicodeStringConst(String::from_utf16_lossy(&units))
                }
                Token::STRUCT_MEMBER => {
                    let struct_ref = self.read_object_ref()?;
                    let value = self.parse_expr(depth + 1)?;
                    ExprKind::StructMember {
                        struct_ref,
                        value: Box::new(value),
                    }
                }
                Token::GLOBAL_FUNCTION => {
                    let name = self.read_name_ref()?;
                    let params = self.parse_params(depth + 1)?;
                    ExprKind::GlobalFunction { name, params }
                }
                other => {
                    return Err(BytecodeError::UnknownToken {
                        token: other.0,
                        norm_offset: start_norm,
                        raw_offset: start_raw,
                    });
                }
            }
        };

        if self.norm > self.limit {
            return Err(BytecodeError::ScriptSizeMismatch {
                function: "<expr>".to_string(),
                expected: self.limit,
                actual: self.norm,
            });
        }

        Ok(Expr {
            offset: start_norm,
            size: self.norm - start_norm,
            raw_offset: start_raw,
            raw_size: self.raw - start_raw,
            kind,
        })
    }

    fn parse_params(&mut self, depth: usize) -> Result<Vec<Expr>> {
        let mut params = Vec::new();
        loop {
            let expr = self.parse_expr(depth)?;
            if matches!(expr.kind, ExprKind::EndFunctionParms) {
                return Ok(params);
            }
            params.push(expr);
        }
    }
}

/// Decode a raw function script into an expression list.
pub fn decode_script(
    raw: &[u8],
    normalized_size: u32,
) -> Result<(Vec<Expr>, usize, u32)> {
    let mut reader = ScriptReader::new(raw, normalized_size);
    let mut body = Vec::new();
    while reader.norm < normalized_size {
        body.push(reader.parse_expr(0)?);
    }
    if reader.norm != normalized_size {
        return Err(BytecodeError::ScriptSizeMismatch {
            function: "<script>".to_string(),
            expected: normalized_size,
            actual: reader.norm,
        });
    }
    Ok((body, reader.raw, reader.norm))
}

/// Read one function export from a package.
pub fn read_function(package: &UnrealPackage, reference: i32) -> Result<Function> {
    let export = package
        .exports()
        .get(reference)
        .ok_or(PackageError::InvalidExportReference(reference))?;
    if !package
        .class_name(reference)
        .map(|name| name.eq_ignore_ascii_case("Function"))
        .unwrap_or(false)
    {
        return Err(BytecodeError::NotFunction(reference));
    }

    let export_end = export.serial_offset as usize + export.serial_size as usize;
    let mut reader = ArchiveReader::new(package.data(), export.serial_offset as usize)?;

    if export.has_stack() {
        skip_stack_header(&mut reader)?;
    }
    if export.class_ref != 0 {
        skip_tagged_properties(&mut reader, package.names())?;
    }

    let super_ref = reader.compact_index()?;
    let next_ref = reader.compact_index()?;
    let script_text_ref = reader.compact_index()?;
    let children_ref = reader.compact_index()?;
    let friendly_name_index = reader.compact_index()?;
    let friendly_name = package
        .name(friendly_name_index)
        .unwrap_or("<invalid>")
        .to_string();
    let line = reader.u32()?;
    let text_pos = reader.u32()?;
    let script_size = reader.u32()?;

    let script_offset = reader.pos();
    if script_offset > export_end || script_offset > package.data().len() {
        return Err(BytecodeError::UnexpectedEnd {
            raw_offset: script_offset,
            norm_offset: 0,
        });
    }
    let available = &package.data()[script_offset..export_end.min(package.data().len())];
    let (body, raw_size, norm_size) = decode_script(available, script_size)?;
    if norm_size != script_size {
        return Err(BytecodeError::ScriptSizeMismatch {
            function: package.object_name(reference).unwrap_or("?").to_string(),
            expected: script_size,
            actual: norm_size,
        });
    }
    reader.seek(script_offset + raw_size)?;

    let native_index = reader.i16()?;
    let precedence = reader.u8()?;
    let function_flags = reader.u32()?;
    let replication_offset = if (function_flags & crate::function::function_flags::NET) != 0 {
        Some(reader.i16()?)
    } else {
        None
    };

    if reader.pos() > export_end {
        return Err(BytecodeError::UnexpectedEnd {
            raw_offset: reader.pos(),
            norm_offset: norm_size,
        });
    }

    Ok(Function {
        export_ref: reference,
        name: package.object_name(reference).unwrap_or("?").to_string(),
        outer_ref: export.outer,
        outer_name: package.object_name(export.outer).unwrap_or("").to_string(),
        super_ref,
        next_ref,
        script_text_ref,
        children_ref,
        friendly_name,
        line,
        text_pos,
        script_size,
        script_offset,
        script_raw_size: raw_size,
        script: available[..raw_size].to_vec(),
        body,
        native_index,
        precedence,
        function_flags,
        replication_offset,
        export_end,
        reader_end: reader.pos(),
    })
}

fn skip_stack_header(reader: &mut ArchiveReader<'_>) -> std::result::Result<(), PackageError> {
    let _node = reader.compact_index()?;
    let _state = reader.compact_index()?;
    let _probe = reader.u64()?;
    let _latent = reader.i32()?;
    if _node != 0 {
        let _offset = reader.compact_index()?;
    }
    Ok(())
}

/// Parse every function in a package.
pub fn parse_package_functions(package: &UnrealPackage) -> Result<Vec<Function>> {
    let refs = package.function_exports();
    let mut functions = Vec::with_capacity(refs.len());
    for reference in refs {
        functions.push(read_function(package, reference)?);
    }
    Ok(functions)
}
