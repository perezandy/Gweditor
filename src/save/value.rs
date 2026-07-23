use anyhow::{bail, Result};

use super::cursor::Cursor;

/// A decoded property value. Scalar variants carry the absolute offset of
/// their encoding so the UI can patch them in place.
#[derive(Debug, Clone)]
pub enum Value {
    Bool { off: usize, v: bool },
    U8 { off: usize, v: u8 },
    I8 { off: usize, v: i8 },
    U16 { off: usize, v: u16 },
    I16 { off: usize, v: i16 },
    U32 { off: usize, v: u32 },
    I32 { off: usize, v: i32 },
    U64 { off: usize, v: u64 },
    I64 { off: usize, v: i64 },
    F32 { off: usize, v: f32 },
    F64 { off: usize, v: f64 },
    /// Index into the CName table (1-based), resolved to its string.
    CName { off: usize, idx: u16, name: String },
    Str(String),
    Guid([u8; 16]),
    Enum { off: usize, a: u8, b: u8 },
    Array(Vec<Value>),
    Handle(Box<Value>),
    Soft(Box<Value>),
    /// Anything we do not decode; kept opaque and uneditable.
    Opaque { off: usize, len: usize },
}

impl Value {
    pub fn display(&self, max_opaque: usize) -> String {
        match self {
            Value::Bool { v, .. } => v.to_string(),
            Value::U8 { v, .. } => v.to_string(),
            Value::I8 { v, .. } => v.to_string(),
            Value::U16 { v, .. } => v.to_string(),
            Value::I16 { v, .. } => v.to_string(),
            Value::U32 { v, .. } => v.to_string(),
            Value::I32 { v, .. } => v.to_string(),
            Value::U64 { v, .. } => v.to_string(),
            Value::I64 { v, .. } => v.to_string(),
            Value::F32 { v, .. } => v.to_string(),
            Value::F64 { v, .. } => v.to_string(),
            Value::CName { name, .. } => name.clone(),
            Value::Str(s) => s.clone(),
            Value::Guid(g) => format!("{g:02x?}"),
            Value::Enum { a, b, .. } => format!("enum({a},{b})"),
            Value::Array(items) => format!("[{} items]", items.len()),
            Value::Handle(v) | Value::Soft(v) => v.display(max_opaque),
            Value::Opaque { len, .. } => {
                if *len <= max_opaque {
                    format!("<{len} bytes>")
                } else {
                    format!("<{len} bytes>")
                }
            }
        }
    }
}

/// Decode a value of CName type `type_name`, consuming from `cur` and
/// decrementing `size` (the token-byte budget), mirroring the reference
/// implementation from W3SavegameEditor.
pub fn read_value(
    cur: &mut Cursor,
    names: &[String],
    type_name: &str,
    size: &mut usize,
) -> Result<Value> {
    fn dec(size: &mut usize, n: usize) -> Result<()> {
        if *size < n {
            bail!("value overruns token budget ({} left, needs {n})", *size);
        }
        *size -= n;
        Ok(())
    }

    macro_rules! scalar {
        ($variant:ident, $read:ident, $n:expr) => {{
            let off = cur.pos();
            let v = cur.$read()?;
            dec(size, $n)?;
            Ok(Value::$variant { off, v })
        }};
    }

    match type_name {
        "Bool" => {
            let off = cur.pos();
            let v = cur.read_u8()? != 0;
            dec(size, 1)?;
            Ok(Value::Bool { off, v })
        }
        "Uint8" => scalar!(U8, read_u8, 1),
        "Int8" => scalar!(I8, read_i8, 1),
        "Uint16" => scalar!(U16, read_u16, 2),
        "Int16" => scalar!(I16, read_i16, 2),
        "Uint32" => scalar!(U32, read_u32, 4),
        "Int32" => scalar!(I32, read_i32, 4),
        "Uint64" => scalar!(U64, read_u64, 8),
        "Int64" => scalar!(I64, read_i64, 8),
        "Float" => scalar!(F32, read_f32, 4),
        "Double" => scalar!(F64, read_f64, 8),
        "CName" => {
            let off = cur.pos();
            let idx = cur.read_u16()?;
            dec(size, 2)?;
            let name = resolve_name(names, idx);
            Ok(Value::CName { off, idx, name })
        }
        "CGUID" => {
            let b = cur.take(16)?;
            dec(size, 16)?;
            Ok(Value::Guid(b.try_into().unwrap()))
        }
        "String" | "CEntityTemplate" => {
            let header = cur.read_u8()?;
            dec(size, 1)?;
            if header & 0x80 != 0 {
                // ASCII string; a stray 0x01 marker byte sometimes follows.
                if cur.peek(1) == [0x01] {
                    cur.read_u8()?;
                    dec(size, 1)?;
                }
                let len = (header & 0x7f) as usize;
                let s = cur.read_str(len)?;
                dec(size, len)?;
                Ok(Value::Str(s))
            } else {
                let off = cur.pos();
                let len = *size;
                cur.take(len)?;
                *size = 0;
                Ok(Value::Opaque { off, len })
            }
        }
        "StringAnsi" => {
            let len = cur.read_u8()? as usize;
            let s = cur.read_str(len)?;
            dec(size, 1 + len)?;
            Ok(Value::Str(s.trim_end_matches('\0').to_string()))
        }
        "eGwintFaction" | "EJournalStatus" | "EZoneName" | "EDifficultyMode" => {
            let off = cur.pos();
            let a = cur.read_u8()?;
            let b = cur.read_u8()?;
            dec(size, 2)?;
            Ok(Value::Enum { off, a, b })
        }
        "TagList" => {
            let header = cur.read_u8()?;
            dec(size, 1)?;
            let count = (header & 0x7f) as usize;
            let mut items = Vec::with_capacity(count);
            for _ in 0..count {
                let off = cur.pos();
                let idx = cur.read_u16()?;
                let name = resolve_name(names, idx);
                items.push(Value::CName { off, idx, name });
            }
            dec(size, count * 2)?;
            Ok(Value::Array(items))
        }
        _ => {
            if let Some(elem) = type_name.strip_prefix("array:2,0,") {
                let count = cur.read_i32()? as usize;
                dec(size, 4)?;
                let mut items = Vec::with_capacity(count);
                for _ in 0..count {
                    items.push(read_value(cur, names, elem, size)?);
                }
                Ok(Value::Array(items))
            } else if let Some(inner) = type_name.strip_prefix("handle:") {
                Ok(Value::Handle(Box::new(read_value(cur, names, inner, size)?)))
            } else if let Some(inner) = type_name.strip_prefix("soft:") {
                Ok(Value::Soft(Box::new(read_value(cur, names, inner, size)?)))
            } else {
                // Unknown structured type: keep the remaining token bytes opaque.
                let off = cur.pos();
                let len = *size;
                cur.take(len)?;
                *size = 0;
                Ok(Value::Opaque { off, len })
            }
        }
    }
}

pub fn resolve_name(names: &[String], idx: u16) -> String {
    if idx == 0 || idx as usize > names.len() {
        format!("<name#{idx}>")
    } else {
        names[idx as usize - 1].clone()
    }
}
