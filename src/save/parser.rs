use anyhow::{bail, Result};

use super::cursor::Cursor;
use super::value::{read_value, resolve_name, Value};

#[derive(Debug, Clone, Copy)]
pub struct TableEntry {
    pub offset: usize,
    pub size: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    /// SXAP: payload format header (three version ints).
    Sxap,
    /// MANU: string table token.
    Manu,
    /// VL: named + typed value.
    Vl,
    /// OP: named + typed value.
    Op,
    /// AVAL: named + typed value with an extra header int.
    Aval,
    /// PORP: named + typed value with explicit value size.
    Porp,
    /// SS: sized set of inline child tokens.
    Ss,
    /// BS: block start; children are the following table entries.
    Bs,
    /// BLCK: named block with inline child tokens.
    Blck,
    /// ROTS ("STOR" reversed): nested item-storage token stream, terminated
    /// by a literal "STOR". Found inside entity/component data.
    Stor,
    /// A token that failed to parse; kept opaque.
    Invalid,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: NodeKind,
    pub name: String,
    pub type_name: String,
    pub value: Option<Value>,
    pub children: Vec<Node>,
    /// Absolute span of this token in the decompressed buffer.
    pub offset: usize,
    pub token_size: usize,
    /// Size from the variable table (may exceed token_size for sets whose
    /// children follow as separate table entries).
    pub table_size: usize,
    /// Parse error message for Invalid nodes.
    pub error: Option<String>,
}

impl Node {
    fn invalid(entry: TableEntry, err: String) -> Node {
        Node {
            kind: NodeKind::Invalid,
            name: String::new(),
            type_name: String::new(),
            value: None,
            children: Vec::new(),
            offset: entry.offset,
            token_size: entry.size,
            table_size: entry.size,
            error: Some(err),
        }
    }

    pub fn is_set(&self) -> bool {
        matches!(self.kind, NodeKind::Ss | NodeKind::Bs | NodeKind::Blck)
    }
}

/// Parse the MANU string table. `manu_offset` is the position of the "MANU"
/// magic (right after the "NM" section magic).
pub fn parse_manu(cur: &mut Cursor, manu_offset: usize) -> Result<Vec<String>> {
    cur.seek(manu_offset)?;
    if cur.read_str(4)? != "MANU" {
        bail!("bad MANU magic at {manu_offset:#x}");
    }
    let count = cur.read_i32()? as usize;
    let _unknown1 = cur.read_i32()?;
    let mut names = Vec::with_capacity(count);
    for _ in 0..count {
        let len = cur.read_u8()? as usize;
        names.push(cur.read_str(len)?);
    }
    let _unknown2 = cur.read_i32()?;
    if cur.read_str(4)? != "ENOD" {
        bail!("MANU table not terminated by ENOD");
    }
    Ok(names)
}

/// Parse all variable-table entries into a hierarchical tree.
///
/// Tokens are parsed individually from their table offsets; set tokens whose
/// table size exceeds their own token size then absorb the following entries
/// as children (recursively), matching the game's flat-table layout.
pub fn parse_variables(data: &[u8], entries: &[TableEntry], names: &[String]) -> Vec<Node> {
    let mut flat: Vec<Node> = Vec::with_capacity(entries.len());
    for (i, e) in entries.iter().enumerate() {
        // The table's size field can disagree with the distance to the next
        // entry; the distance is authoritative for tokenizing. The last two
        // entries have no reliable next-offset, so fall back to the size.
        let token_size = if i + 2 < entries.len() {
            entries[i + 1].offset - e.offset
        } else {
            e.size
        };
        let mut node = match parse_token(data, *e, token_size, names) {
            Ok(n) => n,
            Err(err) => Node::invalid(*e, err.to_string()),
        };
        // The distance to the next entry is the token's true footprint in the
        // flat table, which is what the set-absorption budget math needs.
        node.token_size = token_size;
        flat.push(node);
    }

    // Hierarchical absorption: a set consumes following siblings until its
    // table size is exhausted.
    let mut iter = flat.into_iter();
    let mut roots = Vec::new();
    while let Some(node) = iter.next() {
        let (node, _consumed) = absorb(node, &mut iter);
        roots.push(node);
    }
    roots
}

/// Returns the node plus the total token bytes it consumed from the stream
/// (its own token plus all absorbed descendants).
fn absorb(mut node: Node, iter: &mut std::vec::IntoIter<Node>) -> (Node, usize) {
    let mut consumed = node.token_size;
    if node.is_set() && node.table_size > node.token_size {
        let mut budget = node.table_size as isize - node.token_size as isize;
        while budget > 0 {
            let Some(child) = iter.next() else { break };
            let (child, child_consumed) = absorb(child, iter);
            budget -= child_consumed as isize;
            consumed += child_consumed;
            node.children.push(child);
        }
    }
    (node, consumed)
}

/// Parse consecutive child tokens from `cur` until `budget` bytes are
/// consumed. A child that fails to parse becomes an opaque Invalid node
/// covering the rest of the budget rather than poisoning the parent.
fn parse_children(
    data: &[u8],
    cur: &mut Cursor,
    budget: usize,
    names: &[String],
    out: &mut Vec<Node>,
) -> Result<()> {
    let mut remaining = budget;
    while remaining > 0 {
        let child_entry = TableEntry {
            offset: cur.pos(),
            size: remaining,
        };
        match parse_token(data, child_entry, remaining, names) {
            Ok(child) => {
                let consumed = child.token_size.max(1);
                remaining = remaining.saturating_sub(consumed);
                cur.seek(child_entry.offset + consumed)?;
                out.push(child);
            }
            Err(err) => {
                out.push(Node::invalid(child_entry, err.to_string()));
                break;
            }
        }
    }
    Ok(())
}

// `size` decrements keep the byte budget consistent across branches even
// where a later explicit size takes over, so some assignments are unread.
#[allow(unused_assignments)]
fn parse_token(
    data: &[u8],
    entry: TableEntry,
    token_size: usize,
    names: &[String],
) -> Result<Node> {
    let mut cur = Cursor::new(data);
    cur.seek(entry.offset)?;

    let magic4: Vec<u8> = cur.peek(4).to_vec();
    let mut node = Node {
        kind: NodeKind::Invalid,
        name: String::new(),
        type_name: String::new(),
        value: None,
        children: Vec::new(),
        offset: entry.offset,
        token_size,
        table_size: entry.size,
        error: None,
    };

    let mut size = token_size;
    match &magic4[..] {
        b"SXAP" => {
            cur.take(4)?;
            size -= 4;
            node.kind = NodeKind::Sxap;
            let _v = [cur.read_i32()?, cur.read_i32()?, cur.read_i32()?];
        }
        b"MANU" => {
            node.kind = NodeKind::Manu;
            parse_manu(&mut cur, entry.offset)?;
        }
        b"AVAL" => {
            cur.take(4)?;
            size -= 4;
            node.kind = NodeKind::Aval;
            let name_idx = cur.read_u16()?;
            let type_idx = cur.read_u16()?;
            let value_size = cur.read_i32()? as usize;
            size -= 8;
            node.name = resolve_name(names, name_idx);
            node.type_name = resolve_name(names, type_idx);
            let value_start = cur.pos();
            let mut vsize = value_size;
            node.value = Some(read_value(&mut cur, names, &node.type_name, &mut vsize)?);
            // The declared size is authoritative; skip any undecoded tail.
            cur.seek(value_start + value_size)?;
        }
        b"PORP" => {
            cur.take(4)?;
            size -= 4;
            node.kind = NodeKind::Porp;
            let name_idx = cur.read_u16()?;
            let type_idx = cur.read_u16()?;
            size -= 4;
            let value_size = cur.read_i32()? as usize;
            size -= 4;
            node.name = resolve_name(names, name_idx);
            node.type_name = resolve_name(names, type_idx);
            let value_start = cur.pos();
            let mut vsize = value_size;
            node.value = Some(read_value(&mut cur, names, &node.type_name, &mut vsize)?);
            cur.seek(value_start + value_size)?;
        }
        b"ROTS" => {
            cur.take(4)?;
            node.kind = NodeKind::Stor;
            node.name = "STOR".to_string();
            let inner = cur.read_u32()? as usize;
            let end = cur.pos() + inner;
            parse_children(data, &mut cur, inner, names, &mut node.children)?;
            cur.seek(end)?;
            if cur.take(4)? != b"STOR" {
                bail!("ROTS block at {:#x} not terminated by STOR", entry.offset);
            }
        }
        b"BLCK" => {
            cur.take(4)?;
            size -= 4;
            node.kind = NodeKind::Blck;
            let name_idx = cur.read_u16()?;
            let blck_size = cur.read_u32()? as usize;
            size -= 6;
            node.name = resolve_name(names, name_idx);
            // blck_size describes the children's byte length; the outer
            // budget caps it defensively.
            let budget = blck_size.min(size);
            let end = cur.pos() + budget;
            parse_children(data, &mut cur, budget, names, &mut node.children)?;
            cur.seek(end)?;
        }
        _ => match &magic4[..2] {
            b"VL" => {
                cur.take(2)?;
                size -= 2;
                node.kind = NodeKind::Vl;
                let name_idx = cur.read_u16()?;
                let type_idx = cur.read_u16()?;
                size -= 4;
                node.name = resolve_name(names, name_idx);
                node.type_name = resolve_name(names, type_idx);
                node.value = Some(read_value(&mut cur, names, &node.type_name, &mut size)?);
            }
            b"OP" => {
                cur.take(2)?;
                size -= 2;
                node.kind = NodeKind::Op;
                let name_idx = cur.read_u16()?;
                let type_idx = cur.read_u16()?;
                size -= 4;
                node.name = resolve_name(names, name_idx);
                node.type_name = resolve_name(names, type_idx);
                node.value = Some(read_value(&mut cur, names, &node.type_name, &mut size)?);
            }
            b"SS" => {
                cur.take(2)?;
                size -= 2;
                node.kind = NodeKind::Ss;
                node.name = "SS".to_string();
                let inner = cur.read_i32()? as usize;
                size -= 4;
                let budget = inner.min(size);
                let end = cur.pos() + budget;
                parse_children(data, &mut cur, budget, names, &mut node.children)?;
                cur.seek(end)?;
            }
            b"BS" => {
                cur.take(2)?;
                size -= 2;
                node.kind = NodeKind::Bs;
                let name_idx = cur.read_u16()?;
                size -= 2;
                node.name = resolve_name(names, name_idx);
            }
            _ => {
                bail!(
                    "unknown token magic {:02x?} at {:#x}",
                    &magic4[..magic4.len().min(4)],
                    entry.offset
                );
            }
        },
    }

    // Record how many bytes this token actually consumed. Callers that
    // tokenize by table distance override this with the distance.
    node.token_size = cur.pos() - entry.offset;
    Ok(node)
}
