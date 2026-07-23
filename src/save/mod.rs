pub mod chunked_lz4;
pub mod cursor;
pub mod facts;
pub mod inventory;
pub mod parser;
pub mod value;

use anyhow::{bail, Context, Result};
use std::path::Path;

pub use parser::{Node, NodeKind};
pub use value::Value;

/// A loaded savegame: the decompressed payload plus the parsed variable tree.
///
/// All offsets stored in nodes/values are absolute offsets into `data`
/// (which includes the zeroed container-header prefix, matching the
/// offset convention used by the game's own tables).
pub struct SaveFile {
    /// Decompressed payload, prefixed by `header_size` bytes reserved for the
    /// container header (contents unused while decompressed).
    pub data: Vec<u8>,
    pub header_size: usize,
    /// Format version ints following the SAV3 magic.
    pub versions: [i32; 3],
    /// CName string table (1-based indices in the file).
    pub names: Vec<String>,
    /// Top-level parsed variables, hierarchically structured.
    pub roots: Vec<Node>,
}

impl SaveFile {
    pub fn load(path: &Path) -> Result<SaveFile> {
        let raw = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        Self::from_compressed(&raw)
    }

    pub fn from_compressed(raw: &[u8]) -> Result<SaveFile> {
        let container = chunked_lz4::decompress(raw)?;
        Self::from_decompressed(container.data, container.header_size)
    }

    pub fn from_decompressed(data: Vec<u8>, header_size: usize) -> Result<SaveFile> {
        let mut cur = cursor::Cursor::new(&data);

        // Header
        cur.seek(header_size)?;
        let magic = cur.read_str(4)?;
        if magic != "SAV3" {
            bail!("bad payload magic {magic:?}, expected \"SAV3\"");
        }
        let versions = [cur.read_i32()?, cur.read_i32()?, cur.read_i32()?];

        // Footer: variable table offset + "SE"
        cur.seek(data.len() - 6)?;
        let variable_table_offset = cur.read_i32()? as usize;
        let se = cur.read_str(2)?;
        if se != "SE" {
            bail!("bad footer magic {se:?}, expected \"SE\"");
        }

        // String table footer sits 10 bytes before the variable table.
        let string_table_footer = variable_table_offset
            .checked_sub(10)
            .context("variable table offset too small")?;
        cur.seek(string_table_footer)?;
        let nm_offset = cur.read_i32()? as usize;
        let rb_offset = cur.read_i32()? as usize;

        // NM section wraps the MANU name table.
        cur.seek(nm_offset)?;
        if cur.read_str(2)? != "NM" {
            bail!("bad NM section magic at {nm_offset:#x}");
        }
        let manu_offset = cur.pos();
        let names = parser::parse_manu(&mut cur, manu_offset)?;

        // RB section (unknown purpose; validated only).
        cur.seek(rb_offset)?;
        if cur.read_str(2)? != "RB" {
            bail!("bad RB section magic at {rb_offset:#x}");
        }

        // Variable table
        cur.seek(variable_table_offset)?;
        let entry_count = cur.read_i32()? as usize;
        let mut entries = Vec::with_capacity(entry_count);
        for _ in 0..entry_count {
            let offset = cur.read_i32()? as usize;
            let size = cur.read_i32()? as usize;
            entries.push(parser::TableEntry { offset, size });
        }
        entries.sort_by_key(|e| e.offset);

        let roots = parser::parse_variables(&data, &entries, &names);

        Ok(SaveFile {
            data,
            header_size,
            versions,
            names,
            roots,
        })
    }

    /// Apply an in-place scalar patch. `bytes` must exactly match the
    /// original encoded width of the value being replaced.
    pub fn patch(&mut self, offset: usize, bytes: &[u8]) -> Result<()> {
        let end = offset + bytes.len();
        if offset < self.header_size || end > self.data.len() {
            bail!("patch range {offset:#x}..{end:#x} out of bounds");
        }
        self.data[offset..end].copy_from_slice(bytes);
        Ok(())
    }

    /// Recompress and write the save to `path`.
    pub fn save(&self, path: &Path) -> Result<()> {
        let raw = chunked_lz4::compress(&self.data, self.header_size)?;
        std::fs::write(path, raw).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }
}
