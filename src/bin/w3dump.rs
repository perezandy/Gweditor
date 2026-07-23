use std::path::PathBuf;

use anyhow::Result;
use gweditor::save::{Node, SaveFile};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(args.next().expect("usage: w3dump <save.sav> [depth]"));
    let max_depth: usize = args.next().map(|d| d.parse().unwrap()).unwrap_or(2);

    let save = SaveFile::load(&path)?;
    if std::env::args().any(|a| a == "--raw") {
        // Write next to the current working directory, not the saves folder.
        let out = PathBuf::from(path.file_name().unwrap()).with_extension("raw");
        std::fs::write(&out, &save.data)?;
        println!("wrote decompressed payload to {}", out.display());
    }
    println!("payload bytes: {}", save.data.len() - save.header_size);
    println!("versions:      {:?}", save.versions);
    println!("cnames:        {}", save.names.len());
    println!("root tokens:   {}", save.roots.len());

    let mut invalid = 0usize;
    let mut total = 0usize;
    for root in &save.roots {
        count(root, &mut total, &mut invalid);
    }
    println!("total nodes:   {total} ({invalid} invalid)");

    match gweditor::save::facts::parse_facts(&save) {
        Ok(facts) => println!("facts:         {}", facts.len()),
        Err(e) => println!("facts:         FAILED: {e}"),
    }
    let inventories = gweditor::save::inventory::find_inventories(&save);
    println!("inventories:   {}", inventories.len());
    for inv in inventories.iter().take(8) {
        println!(
            "  [{} items] @{:#x} {}",
            inv.items.len(),
            inv.span.0,
            inv.label
        );
        for item in inv.items.iter().take(4) {
            println!(
                "     {} x{} dur {} {:?}",
                item.name, item.quantity, item.durability, item.enchantment
            );
        }
    }
    println!();

    for root in &save.roots {
        print_node(root, 0, max_depth);
    }
    Ok(())
}

fn count(n: &Node, total: &mut usize, invalid: &mut usize) {
    *total += 1;
    if n.error.is_some() {
        *invalid += 1;
    }
    for c in &n.children {
        count(c, total, invalid);
    }
}

fn print_node(n: &Node, depth: usize, max_depth: usize) {
    if depth > max_depth {
        return;
    }
    let indent = "  ".repeat(depth);
    let value = n
        .value
        .as_ref()
        .map(|v| format!(" = {}", v.display(16)))
        .unwrap_or_default();
    let err = n
        .error
        .as_ref()
        .map(|e| format!(" !! {e}"))
        .unwrap_or_default();
    let ty = if n.type_name.is_empty() {
        String::new()
    } else {
        format!(": {}", n.type_name)
    };
    println!(
        "{indent}{:?} {}{ty}{value} @{:#x} (token {}, table {}, {} children){err}",
        n.kind,
        n.name,
        n.offset,
        n.token_size,
        n.table_size,
        n.children.len()
    );
    for c in &n.children {
        print_node(c, depth + 1, max_depth);
    }
}
