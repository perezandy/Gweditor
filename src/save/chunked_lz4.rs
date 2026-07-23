use anyhow::{bail, Context, Result};

use super::cursor::Cursor;

/// Decompressed chunk size the game uses when writing saves.
const CHUNK_SIZE: usize = 0x0010_0000;

pub struct Container {
    /// Header-region size; the decompressed payload starts at this offset.
    pub header_size: usize,
    /// `header_size` zero bytes followed by the decompressed payload, so that
    /// offsets inside the payload match the game's absolute-offset tables.
    pub data: Vec<u8>,
}

struct ChunkEntry {
    compressed_size: usize,
    decompressed_size: usize,
    end_offset: usize,
}

pub fn decompress(raw: &[u8]) -> Result<Container> {
    let mut cur = Cursor::new(raw);
    if cur.read_str(4)? != "SNFH" {
        bail!("not a Witcher 3 save: missing SNFH magic");
    }
    if cur.read_str(4)? != "FZLC" {
        bail!("unsupported container: missing FZLC (chunked LZ4) magic");
    }
    let chunk_count = cur.read_i32()? as usize;
    let header_size = cur.read_i32()? as usize;
    if header_size < 16 + chunk_count * 12 || header_size > raw.len() {
        bail!("implausible container header size {header_size:#x}");
    }

    let mut entries = Vec::with_capacity(chunk_count);
    for _ in 0..chunk_count {
        entries.push(ChunkEntry {
            compressed_size: cur.read_i32()? as usize,
            decompressed_size: cur.read_i32()? as usize,
            end_offset: cur.read_i32()? as usize,
        });
    }

    let total: usize = entries.iter().map(|e| e.decompressed_size).sum();
    let mut data = vec![0u8; header_size + total];

    cur.seek(header_size)?;
    let mut out_pos = header_size;
    for (i, e) in entries.iter().enumerate() {
        let comp = cur.take(e.compressed_size)?;
        let n = lz4_flex::block::decompress_into(comp, &mut data[out_pos..out_pos + e.decompressed_size])
            .with_context(|| format!("decompressing chunk {i}"))?;
        if n != e.decompressed_size {
            bail!(
                "chunk {i}: expected {} decompressed bytes, got {n}",
                e.decompressed_size
            );
        }
        if e.end_offset != 0 && cur.pos() != e.end_offset {
            bail!(
                "chunk {i}: stream position {:#x} does not match table end offset {:#x}",
                cur.pos(),
                e.end_offset
            );
        }
        out_pos += e.decompressed_size;
    }

    Ok(Container { header_size, data })
}

/// Rebuild the compressed container from a (possibly patched) decompressed
/// buffer. `header_size` must be the value the buffer was loaded with, since
/// every offset in the payload's internal tables is relative to it.
pub fn compress(data: &[u8], header_size: usize) -> Result<Vec<u8>> {
    let payload = &data[header_size..];
    let chunk_count = payload.len().div_ceil(CHUNK_SIZE);
    if 16 + chunk_count * 12 > header_size {
        bail!(
            "chunk table for {chunk_count} chunks does not fit in the {header_size}-byte header region"
        );
    }

    let mut out = vec![0u8; header_size];
    let mut entries: Vec<ChunkEntry> = Vec::with_capacity(chunk_count);
    for chunk in payload.chunks(CHUNK_SIZE) {
        let comp = lz4_flex::block::compress(chunk);
        out.extend_from_slice(&comp);
        entries.push(ChunkEntry {
            compressed_size: comp.len(),
            decompressed_size: chunk.len(),
            end_offset: out.len(),
        });
    }

    // Fill in the header region.
    out[0..4].copy_from_slice(b"SNFH");
    out[4..8].copy_from_slice(b"FZLC");
    out[8..12].copy_from_slice(&(chunk_count as i32).to_le_bytes());
    out[12..16].copy_from_slice(&(header_size as i32).to_le_bytes());
    let mut p = 16;
    for e in &entries {
        out[p..p + 4].copy_from_slice(&(e.compressed_size as i32).to_le_bytes());
        out[p + 4..p + 8].copy_from_slice(&(e.decompressed_size as i32).to_le_bytes());
        out[p + 8..p + 12].copy_from_slice(&(e.end_offset as i32).to_le_bytes());
        p += 12;
    }

    Ok(out)
}
