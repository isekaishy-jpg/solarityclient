//! External stock-compatibility tests for WDT map manifests and tile paths.

use std::error::Error;
use std::path::Path;

use solarity_asset::{
    ArchiveCatalog, AssetError, AssetStore, ClientDataRoot, Locale, MapCatalog, TerrainMap,
    TerrainTileIndex,
};
use wow_wdt::chunks::MwmoChunk;
use wow_wdt::version::WowVersion;
use wow_wdt::{WdtFile, WdtWriter};

use crate::support::{Fixture, FixtureFile};

/// WDT selection obeys archive precedence and preserves exact grid/path data.
#[test]
fn terrain_map_loads_patched_stock_manifest() -> Result<(), Box<dyn Error>> {
    let map_table = map_table();
    let base_wdt = terrain_wdt(None)?;
    let patch_wdt = terrain_wdt(Some((32, 31, 4395)))?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\Map.dbc",
            bytes: &map_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "World\\Maps\\Northrend\\Northrend.wdt",
            bytes: &base_wdt,
        },
        FixtureFile {
            archive: "patch-2.MPQ",
            path: "World\\Maps\\Northrend\\Northrend.wdt",
            bytes: &patch_wdt,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let maps = MapCatalog::load(&mut store)?;
    let definition = maps.map(571).ok_or("Northrend map is absent")?;

    let terrain = TerrainMap::load(&mut store, definition)?;
    let index = TerrainTileIndex::new(32, 31).ok_or("fixture tile is invalid")?;
    assert_eq!(terrain.map_id(), 571);
    assert_eq!(terrain.directory(), "Northrend");
    assert_eq!(terrain.source().relative_path(), Path::new("patch-2.MPQ"));
    assert_eq!(terrain.existing_tiles().count(), 1);
    assert!(terrain.tile(index).exists());
    assert_eq!(terrain.tile(index).area_id(), 4395);
    assert_eq!(
        terrain.adt_path(index)?.as_str(),
        "WORLD\\MAPS\\NORTHREND\\NORTHREND_32_31.ADT"
    );
    assert_eq!(
        terrain.wdl_path()?.as_str(),
        "WORLD\\MAPS\\NORTHREND\\NORTHREND.WDL"
    );
    assert_eq!(
        TerrainMap::tile_at_world_position(0.0, 0.0),
        TerrainTileIndex::new(32, 32).ok_or("center tile is invalid")?
    );
    Ok(())
}

/// Later or invented WDT chunks fail instead of being silently skipped.
#[test]
fn terrain_map_rejects_non_build_12340_chunks() -> Result<(), Box<dyn Error>> {
    let map_table = map_table();
    let mut wdt = terrain_wdt(None)?;
    wdt.extend_from_slice(b"DIAM");
    wdt.extend_from_slice(&0_u32.to_le_bytes());
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\Map.dbc",
            bytes: &map_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "World\\Maps\\Northrend\\Northrend.wdt",
            bytes: &wdt,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let maps = MapCatalog::load(&mut store)?;
    let definition = maps.map(571).ok_or("Northrend map is absent")?;

    assert!(matches!(
        TerrainMap::load(&mut store, definition),
        Err(AssetError::TerrainDecode { message, .. })
            if message.contains("unknown chunk")
    ));
    Ok(())
}

fn terrain_wdt(tile: Option<(usize, usize, u32)>) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut wdt = WdtFile::new(WowVersion::WotLK);
    wdt.mwmo = Some(MwmoChunk::new());
    if let Some((x, y, area_id)) = tile {
        let entry = wdt
            .main
            .get_mut(x, y)
            .ok_or("fixture WDT tile is invalid")?;
        entry.set_has_adt(true);
        entry.area_id = area_id;
    }
    let mut bytes = Vec::new();
    WdtWriter::new(&mut bytes).write(&wdt)?;
    Ok(bytes)
}

fn map_table() -> Vec<u8> {
    let mut strings = vec![0_u8];
    let directory = append_string(&mut strings, "Northrend");
    let name = append_string(&mut strings, "Northrend");
    let mut fields = [0_u32; 66];
    fields[0] = 571;
    fields[1] = directory;
    fields[5] = name;
    fields[22] = 571;
    fields[59] = u32::MAX;
    fields[63] = 2;
    create_wdbc(1, 66, &fields, &strings)
}

fn create_wdbc(
    record_count: u32,
    field_count: u32,
    fields: &[u32],
    string_block: &[u8],
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(20 + fields.len() * 4 + string_block.len());
    bytes.extend_from_slice(b"WDBC");
    bytes.extend_from_slice(&record_count.to_le_bytes());
    bytes.extend_from_slice(&field_count.to_le_bytes());
    bytes.extend_from_slice(&(field_count * 4).to_le_bytes());
    bytes.extend_from_slice(&(string_block.len() as u32).to_le_bytes());
    for field in fields {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    bytes.extend_from_slice(string_block);
    bytes
}

fn append_string(block: &mut Vec<u8>, value: &str) -> u32 {
    let offset = block.len() as u32;
    block.extend_from_slice(value.as_bytes());
    block.push(0);
    offset
}
