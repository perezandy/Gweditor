use super::parser::{Node, NodeKind};
use super::SaveFile;

/// One inventory item record, located via its anchor (the constant
/// `01 00 00 00 00 00` field that follows name/seed/flags in every record
/// of the game's raw CInventoryComponent serialization).
#[derive(Debug, Clone)]
pub struct Item {
    /// Absolute offset of the anchor field.
    pub anchor: usize,
    pub name: String,
    pub quantity: u16,
    /// Absolute offset of the u16 quantity, for in-place patching.
    pub quantity_off: usize,
    pub durability: f32,
    /// Absolute offset of the f32 durability.
    pub durability_off: usize,
    pub dye: Option<(String, String)>,
    /// Runeword/glyphword pair, when present.
    pub enchantment: Vec<String>,
    /// Modifier CNames with their u32 values (e.g. NGPItemAdjusted).
    pub mods: Vec<(String, u32)>,
}

#[derive(Debug)]
pub struct Inventory {
    /// Best-effort owner label from the enclosing entity's tag list.
    pub label: String,
    /// Byte span of the containing blob.
    pub span: (usize, usize),
    pub items: Vec<Item>,
}

/// Find all raw inventory blobs in the save.
///
/// Inventory data is not token-encoded, so it surfaces as Invalid nodes in
/// the parsed tree. Each such span is scanned for item records; spans that
/// yield items become inventories, labeled with the nearest tag list found
/// among the node's siblings (e.g. PLAYER for Geralt).
pub fn find_inventories(save: &SaveFile) -> Vec<Inventory> {
    let mut result = Vec::new();
    for root in &save.roots {
        walk(save, root, &mut Vec::new(), &mut result);
    }
    // Largest first: the player's inventory is typically the biggest.
    result.sort_by_key(|inv| std::cmp::Reverse(inv.items.len()));
    result
}

fn walk<'a>(
    save: &SaveFile,
    node: &'a Node,
    siblings_stack: &mut Vec<&'a [Node]>,
    out: &mut Vec<Inventory>,
) {
    if node.kind == NodeKind::Invalid && node.token_size >= 30 {
        let lo = node.offset;
        let hi = (node.offset + node.token_size).min(save.data.len());
        let items = scan_items(&save.data, &save.names, lo, hi);
        if !items.is_empty() {
            let label = siblings_stack
                .iter()
                .rev()
                .find_map(|sibs| tag_label(sibs))
                .unwrap_or_default();
            out.push(Inventory {
                label,
                span: (lo, hi),
                items,
            });
        }
    }
    siblings_stack.push(&node.children);
    for child in &node.children {
        walk(save, child, siblings_stack, out);
    }
    siblings_stack.pop();
}

/// Pull a readable label out of a `tags` block among the given siblings.
fn tag_label(siblings: &[Node]) -> Option<String> {
    let tags = siblings.iter().find(|n| n.name == "tags")?;
    let aval = tags.children.iter().find(|n| n.name == "tags")?;
    match &aval.value {
        Some(super::Value::Array(items)) if !items.is_empty() => {
            let names: Vec<String> = items
                .iter()
                .map(|v| v.display(0))
                .filter(|s| !s.is_empty())
                .collect();
            if names.is_empty() {
                None
            } else {
                Some(names.join(", "))
            }
        }
        _ => None,
    }
}

/// Scan a byte range for item records via their anchor pattern.
pub fn scan_items(data: &[u8], names: &[String], lo: usize, hi: usize) -> Vec<Item> {
    let valid = |idx: u16| idx >= 1 && (idx as usize) <= names.len();
    let name_of = |idx: u16| names[idx as usize - 1].clone();
    let rd_u16 = |p: usize| u16::from_le_bytes([data[p], data[p + 1]]);
    let rd_u32 = |p: usize| u32::from_le_bytes(data[p..p + 4].try_into().unwrap());

    let mut items = Vec::new();
    if hi > data.len() || lo + 30 > hi {
        return items;
    }
    let mut a = lo + 6;
    while a + 20 <= hi {
        // Anchor: unk1 == 1 (u32) followed by zero u16.
        if data[a] != 1 || rd_u32(a) != 1 || rd_u16(a + 4) != 0 {
            a += 1;
            continue;
        }
        let Some(item) = decode_at(data, &valid, &name_of, a, hi) else {
            a += 1;
            continue;
        };
        a = item.1;
        items.push(item.0);
    }
    items
}

/// Try to decode an item record whose anchor starts at `a`.
/// Returns the item and the offset right past the record's mods.
#[allow(clippy::type_complexity)]
fn decode_at(
    data: &[u8],
    valid: &dyn Fn(u16) -> bool,
    name_of: &dyn Fn(u16) -> String,
    a: usize,
    hi: usize,
) -> Option<(Item, usize)> {
    let rd_u16 = |p: usize| u16::from_le_bytes([data[p], data[p + 1]]);
    let rd_u32 = |p: usize| u32::from_le_bytes(data[p..p + 4].try_into().unwrap());
    let rd_f32 = |p: usize| f32::from_le_bytes(data[p..p + 4].try_into().unwrap());

    let name_idx = rd_u16(a - 6);
    if !valid(name_idx) {
        return None;
    }
    let flags2 = data[a + 6];
    if flags2 > 7 {
        return None;
    }
    let mut q = a + 7;
    let mut enchantment = Vec::new();
    if flags2 & 1 != 0 {
        if q + 4 > hi {
            return None;
        }
        let e1 = rd_u16(q);
        let e2 = rd_u16(q + 2);
        if !valid(e1) || !valid(e2) {
            return None;
        }
        enchantment.push(name_of(e1));
        enchantment.push(name_of(e2));
        q += 4;
    }
    if q + 13 > hi {
        return None;
    }
    let d1 = rd_u16(q);
    let d2 = rd_u16(q + 2);
    if !valid(d1) || !valid(d2) {
        return None;
    }
    let quantity_off = q + 4;
    let quantity = rd_u16(quantity_off);
    let durability_off = q + 6;
    let durability = rd_f32(durability_off);
    if !(durability == -1.0 || (0.0..=101.0).contains(&durability)) {
        return None;
    }
    let mod_count = data[q + 10] as usize;
    if mod_count > 60 {
        return None;
    }
    let mut p = q + 11;
    let mut mods = Vec::with_capacity(mod_count);
    for _ in 0..mod_count {
        if p + 7 > hi {
            return None;
        }
        let mn = rd_u16(p);
        let mv = rd_u32(p + 2);
        if data[p + 6] != 2 || !valid(mn) {
            return None;
        }
        mods.push((name_of(mn), mv));
        p += 7;
    }

    let dye_item = name_of(d1);
    let dye_color = name_of(d2);
    let dye = if dye_item == "Dye Default" {
        None
    } else {
        Some((dye_item, dye_color))
    };
    Some((
        Item {
            anchor: a,
            name: name_of(name_idx),
            quantity,
            quantity_off,
            durability,
            durability_off,
            dye,
            enchantment,
            mods,
        },
        p,
    ))
}
