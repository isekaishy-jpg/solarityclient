//! Local validation of one real build-12340 `Map.dbc` and WDT pair.

use std::env;
use std::error::Error;
use std::io;
use std::path::PathBuf;
use std::str::FromStr;

use solarity_asset::{
    ArchiveCatalog, AssetStore, ClientDataRoot, Locale, MapCatalog, TerrainMap, TerrainTileIndex,
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args_os();
    let executable = arguments
        .next()
        .unwrap_or_else(|| "validate_world_map".into());
    let data_root = arguments.next().ok_or_else(|| usage_error(&executable))?;
    let locale = arguments.next().ok_or_else(|| usage_error(&executable))?;
    let map_id = arguments.next().ok_or_else(|| usage_error(&executable))?;
    let tile_x = arguments.next();
    let tile_y = arguments.next();
    if arguments.next().is_some() || tile_x.is_some() != tile_y.is_some() {
        return Err(usage_error(&executable).into());
    }
    let locale = locale
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "locale is not UTF-8"))?;
    let map_id = map_id
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "map ID is not UTF-8"))?
        .parse::<u32>()?;
    let root = ClientDataRoot::new(PathBuf::from(data_root))?;
    let catalog = ArchiveCatalog::discover(root, Locale::from_str(locale)?)?;
    let mut store = AssetStore::mount(catalog)?;
    let maps = MapCatalog::load(&mut store)?;
    let definition = maps.map(map_id).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("Map.dbc does not contain map ID {map_id}"),
        )
    })?;
    let map = TerrainMap::load(&mut store, definition)?;
    println!(
        "map={} name={:?} directory={} flags={:#X} tiles={} global_wmo={} source={}",
        map.map_id(),
        definition.name(),
        map.directory(),
        map.flags(),
        map.existing_tiles().count(),
        map.global_world_model().is_some(),
        map.source().relative_path().display(),
    );
    if let (Some(tile_x), Some(tile_y)) = (tile_x, tile_y) {
        let tile_x = parse_u8(&tile_x, "tile X")?;
        let tile_y = parse_u8(&tile_y, "tile Y")?;
        let index = TerrainTileIndex::new(tile_x, tile_y).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("tile [{tile_x}, {tile_y}] is outside the 64-by-64 map"),
            )
        })?;
        let tile = map.load_tile(&mut store, index)?;
        println!(
            "tile=[{}, {}] chunks={} textures={} doodads={} world_models={} liquid={} source={}",
            tile.index().x(),
            tile.index().y(),
            tile.chunks().len(),
            tile.textures().len(),
            tile.doodads().len(),
            tile.world_models().len(),
            tile.has_liquid_table(),
            tile.source().relative_path().display(),
        );
        if let Some(first) = tile.chunks().first() {
            let minimum_height = first
                .heights()
                .iter()
                .copied()
                .fold(f32::INFINITY, f32::min);
            let maximum_height = first
                .heights()
                .iter()
                .copied()
                .fold(f32::NEG_INFINITY, f32::max);
            println!(
                "first_chunk=[{}, {}] position={:?} relative_height=[{}, {}] layers={} alpha={} sounds={}",
                first.index().x(),
                first.index().y(),
                first.position(),
                minimum_height,
                maximum_height,
                first.layers().len(),
                first.alpha_map().is_some(),
                first.sound_emitters().len(),
            );
        }
        for chunk in tile.chunks().iter().take(17).skip(1).filter(|chunk| {
            (chunk.index().x() == 1 && chunk.index().y() == 0)
                || (chunk.index().x() == 0 && chunk.index().y() == 1)
        }) {
            println!(
                "neighbor_chunk=[{}, {}] position={:?}",
                chunk.index().x(),
                chunk.index().y(),
                chunk.position(),
            );
        }
        if let Some(doodad) = tile.doodads().first() {
            println!(
                "first_doodad={} position={:?} rotation={:?}",
                doodad.path(),
                doodad.position(),
                doodad.rotation(),
            );
        }
        if let Some(world_model) = tile.world_models().first() {
            println!(
                "first_world_model={} position={:?} bounds={:?}",
                world_model.path(),
                world_model.position(),
                world_model.bounds(),
            );
        }
    }
    Ok(())
}

fn parse_u8(value: &std::ffi::OsStr, name: &str) -> Result<u8, io::Error> {
    value
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("{name} is not UTF-8")))?
        .parse::<u8>()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))
}

fn usage_error(executable: &std::ffi::OsStr) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!(
            "usage: {} <Data directory> <locale> <map ID> [tile X tile Y]",
            PathBuf::from(executable).display()
        ),
    )
}
