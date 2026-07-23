use anyhow::{bail, Context, Result};

use super::cursor::Cursor;
use super::SaveFile;

/// One value entry of a fact: value plus the game-time it was set and its
/// expiry time (-1 = never expires).
#[derive(Debug, Clone)]
pub struct FactEntry {
    /// Absolute offset of the i16 value, for in-place patching.
    pub value_off: usize,
    pub value: i16,
    pub time: f32,
    pub expiry: f32,
}

#[derive(Debug, Clone)]
pub struct Fact {
    pub name: String,
    pub expiring: u16,
    pub entries: Vec<FactEntry>,
}

/// Parse the SBDF facts database (the `facts` section of the save).
///
/// Layout: "SBDF", count:i32, then per fact: name (length-prefixed string,
/// high bit set on the length byte, optional 0x01 marker), expiring:u16,
/// entryCount:u16, then entryCount x { value:i16, time:f32, expiry:f32 }.
pub fn parse_facts(save: &SaveFile) -> Result<Vec<Fact>> {
    let sig = b"SBDF";
    let start = save
        .data
        .windows(4)
        .position(|w| w == sig)
        .context("no SBDF facts database found")?;

    let mut cur = Cursor::new(&save.data);
    cur.seek(start + 4)?;
    let count = cur.read_i32()? as usize;
    if count > 2_000_000 {
        bail!("implausible fact count {count}");
    }

    let mut facts = Vec::with_capacity(count);
    for _ in 0..count {
        let header = cur.read_u8()?;
        if header & 0x80 == 0 {
            // Unknown name encoding (seen in some old-gen saves): stop here
            // and return what parsed cleanly rather than failing outright.
            break;
        }
        if cur.peek(1) == [0x01] {
            cur.read_u8()?;
        }
        let len = (header & 0x7f) as usize;
        let name = cur.read_str(len)?;
        let expiring = cur.read_u16()?;
        let entry_count = cur.read_u16()? as usize;
        let mut entries = Vec::with_capacity(entry_count);
        for _ in 0..entry_count {
            let value_off = cur.pos();
            let value = cur.read_i16()?;
            let time = cur.read_f32()?;
            let expiry = cur.read_f32()?;
            entries.push(FactEntry {
                value_off,
                value,
                time,
                expiry,
            });
        }
        facts.push(Fact {
            name,
            expiring,
            entries,
        });
    }
    Ok(facts)
}
