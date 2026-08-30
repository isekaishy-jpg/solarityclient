//! External stock-compatibility tests for WDT map manifests and tile paths.

use std::error::Error;
use std::io::Cursor;
use std::path::Path;

use solarity_asset::{
    ArchiveCatalog, AssetError, AssetStore, ClientDataRoot, Locale, MapCatalog, TerrainMap,
    TerrainTileIndex,
};
use wow_adt::builder::{AdtBuilder, BuiltAdt};
use wow_adt::{AdtVersion, DoodadPlacement, ParsedAdt, WmoPlacement, parse_adt};
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
    assert_eq!(
        TerrainMap::tile_at_world_position(1_000.0, 5_800.0),
        TerrainTileIndex::new(30, 21).ok_or("fixture world tile is invalid")?
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

/// A monolithic WotLK ADT becomes an owned row-major render-data tile.
#[test]
fn terrain_tile_decodes_stock_chunk_geometry() -> Result<(), Box<dyn Error>> {
    let map_table = map_table();
    let wdt = terrain_wdt(Some((32, 32, 1)))?;
    let adt = AdtBuilder::new()
        .with_version(AdtVersion::WotLK)
        .add_texture("tileset/fixture/grass.blp")
        .add_model("world/fixture/tree.m2")
        .add_wmo("world/fixture/house.wmo")
        .add_doodad_placement(DoodadPlacement {
            name_id: 0,
            unique_id: 7,
            position: [16_000.0, 250.0, 12_000.0],
            rotation: [10.0, 20.0, 30.0],
            scale: 1_024,
            flags: 0,
        })
        .add_wmo_placement(WmoPlacement {
            name_id: 0,
            unique_id: 8,
            position: [16_100.0, 300.0, 12_100.0],
            rotation: [15.0, 25.0, 35.0],
            extents_min: [16_000.0, 200.0, 12_000.0],
            extents_max: [16_200.0, 400.0, 12_200.0],
            flags: 0,
            doodad_set: 1,
            name_set: 2,
            scale: 1_024,
        })
        .build()?
        .to_bytes()?;
    let adt = asymmetric_terrain_adt(adt)?;
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
        FixtureFile {
            archive: "common.MPQ",
            path: "World\\Maps\\Northrend\\Northrend_32_32.adt",
            bytes: &adt,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let maps = MapCatalog::load(&mut store)?;
    let definition = maps.map(571).ok_or("Northrend map is absent")?;
    let terrain = TerrainMap::load(&mut store, definition)?;
    let index = TerrainTileIndex::new(32, 32).ok_or("fixture tile is invalid")?;

    let tile = terrain.load_tile(&mut store, index)?;
    assert_eq!(tile.index(), index);
    assert_eq!(tile.chunks().len(), 256);
    assert_eq!(tile.textures().len(), 1);
    assert_eq!(tile.textures()[0].as_str(), "TILESET\\FIXTURE\\GRASS.BLP");
    assert_eq!(tile.chunks()[0].index().x(), 0);
    assert_eq!(tile.chunks()[0].index().y(), 0);
    assert_eq!(tile.chunks()[0].heights().len(), 145);
    assert_eq!(tile.chunks()[0].normals().len(), 145);
    assert_eq!(tile.chunks()[0].position(), [1_000.0, 6_000.0, 200.0]);
    assert_eq!(tile.chunks()[0].normals()[0], [1.0, 0.0, 0.0]);
    assert_eq!(tile.chunks()[255].index().x(), 15);
    assert_eq!(tile.chunks()[255].index().y(), 15);
    assert_eq!(tile.doodads().len(), 1);
    assert_eq!(tile.world_models().len(), 1);
    assert_position(tile.doodads()[0].position(), [1_066.666, 5_066.666, 250.0]);
    assert_position(
        tile.world_models()[0].position(),
        [966.666, 4_966.666, 300.0],
    );
    let [minimum, maximum] = tile.world_models()[0].bounds();
    assert_position(minimum, [866.666, 4_866.666, 200.0]);
    assert_position(maximum, [1_066.666, 5_066.666, 400.0]);
    Ok(())
}

fn assert_position(actual: [f32; 3], expected: [f32; 3]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!((actual - expected).abs() < 0.01, "{actual} != {expected}");
    }
}

/// Gives the first generated chunk distinct values in every stored axis.
///
/// The dependency's fixture writer otherwise emits symmetric zero origins,
/// which cannot detect a transposed stock coordinate basis.
fn asymmetric_terrain_adt(bytes: Vec<u8>) -> Result<Vec<u8>, Box<dyn Error>> {
    let ParsedAdt::Root(mut root) = parse_adt(&mut Cursor::new(bytes))? else {
        return Err("fixture did not decode as a root ADT".into());
    };
    let first = root
        .mcnk_chunks
        .first_mut()
        .ok_or("fixture root ADT contains no MCNK chunks")?;
    // Stored `[zpos, xpos, ypos]` becomes ECS `[X, Y, Z]`.
    first.header.position = [6_000.0, 1_000.0, 200.0];
    let first_normal = first
        .normals
        .as_mut()
        .and_then(|normals| normals.normals.first_mut())
        .ok_or("fixture MCNK contains no normal")?;
    first_normal.x = 127;
    first_normal.y = 0;
    first_normal.z = 0;
    Ok(BuiltAdt::from_root_adt(*root, None).to_bytes()?)
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
