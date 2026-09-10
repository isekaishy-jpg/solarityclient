//! Inspect build-12340 vehicle and seat tables through normal archive precedence.

use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, Locale, VehicleCatalog, WdbcTable,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args_os()
        .nth(1)
        .ok_or("expected Data directory")?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(root)?,
        Locale::EnUs,
    )?)?;
    let catalog = VehicleCatalog::load(&mut store)?;
    for name in ["Vehicle", "VehicleSeat"] {
        let table = WdbcTable::load(
            &mut store,
            &AssetPath::new(format!("DBFilesClient/{name}.dbc"))?,
        )?;
        println!("{name} {:?}", table.header());
        let mut unresolved = 0;
        let mut negative_attachments = 0;
        for index in 0..table.header().record_count() {
            let record = table.record(index).ok_or("missing row")?;
            let id = u32::from_le_bytes(record[..4].try_into()?);
            if name == "Vehicle" {
                let definition = catalog.vehicle(id).ok_or("unindexed vehicle")?;
                for slot in 0..8 {
                    let seat = definition.seat_id(slot).ok_or("seat slot")?;
                    unresolved += usize::from(seat != 0 && catalog.seat(seat).is_none());
                }
                if index < 8 {
                    println!("{definition:?}");
                }
            } else {
                let definition = catalog.seat(id).ok_or("unindexed seat")?;
                negative_attachments += usize::from(definition.attachment_id() < 0);
                if index < 8 {
                    println!("{definition:?}");
                }
            }
        }
        println!(
            "unresolved nonzero seats: {unresolved}; negative attachments: {negative_attachments}"
        );
    }
    Ok(())
}
