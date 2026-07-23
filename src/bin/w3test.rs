use std::path::PathBuf;

use anyhow::{bail, Result};
use gweditor::save::{facts, inventory, SaveFile};

/// Round-trip and edit test:
/// 1. load save, recompress unmodified, reload, verify identical payload
/// 2. patch a quantity, save, reload, verify the change persisted
fn main() -> Result<()> {
    let path = PathBuf::from(std::env::args().nth(1).expect("usage: w3test <save.sav>"));
    let out = std::env::temp_dir().join("w3test_roundtrip.sav");

    let save = SaveFile::load(&path)?;
    save.save(&out)?;
    let reloaded = SaveFile::load(&out)?;
    if save.data != reloaded.data {
        bail!("round-trip payload mismatch");
    }
    if save.roots.len() != reloaded.roots.len() || save.names != reloaded.names {
        bail!("round-trip structure mismatch");
    }
    println!("round-trip OK ({} bytes payload)", save.data.len());

    // Edit test: bump the player's Crowns.
    let mut save = save;
    let invs = inventory::find_inventories(&save);
    let player = invs
        .iter()
        .find(|i| i.label.contains("PLAYER"))
        .expect("no player inventory");
    let crowns = player
        .items
        .iter()
        .find(|i| i.name == "Crowns")
        .expect("no crowns");
    println!("crowns before: {}", crowns.quantity);
    let new_qty: u16 = 12345;
    let off = crowns.quantity_off;
    save.patch(off, &new_qty.to_le_bytes())?;
    save.save(&out)?;

    let reloaded = SaveFile::load(&out)?;
    let invs = inventory::find_inventories(&reloaded);
    let player = invs.iter().find(|i| i.label.contains("PLAYER")).unwrap();
    let crowns = player.items.iter().find(|i| i.name == "Crowns").unwrap();
    if crowns.quantity != new_qty {
        bail!("edit did not persist: {}", crowns.quantity);
    }
    println!("edit persisted: crowns now {}", crowns.quantity);

    // Facts still parse on the edited file.
    let f = facts::parse_facts(&reloaded)?;
    println!("facts on edited file: {}", f.len());
    std::fs::remove_file(&out).ok();
    println!("ALL OK");
    Ok(())
}
