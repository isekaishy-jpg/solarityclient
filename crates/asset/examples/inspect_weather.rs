//! Inspect the installed weather declarations used by the native receiver.

use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, Locale, WdbcTable, WeatherCatalog,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args_os()
        .nth(1)
        .ok_or("expected Data directory")?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(root)?,
        Locale::EnUs,
    )?)?;
    let table = WdbcTable::load(&mut store, &AssetPath::new("DBFilesClient/Weather.dbc")?)?;
    let catalog = WeatherCatalog::load(&mut store)?;
    println!("Weather {:?}", table.header());
    for row in 0..table.header().record_count() {
        let record = table.record(row).ok_or("truncated weather row")?;
        let id = u32::from_le_bytes(record[..4].try_into()?);
        println!("{:?}", catalog.definition(id));
    }
    let lights = solarity_asset::LightCatalog::load(&mut store)?;
    let missing = lights
        .lights()
        .iter()
        .filter(|light| light.parameter_ids()[2] == 0 || light.parameter_ids()[3] == 0)
        .count();
    println!(
        "Light weather banks: {missing}/{} contain a zero slot",
        lights.lights().len()
    );
    for light in lights.lights().iter().filter(|light| light.is_global()) {
        println!(
            "global light {} map {} {:?}",
            light.id(),
            light.map_id(),
            light.parameter_ids()
        );
    }
    Ok(())
}
