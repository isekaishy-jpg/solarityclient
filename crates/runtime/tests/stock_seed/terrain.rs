//! External integration tests for active-world terrain residency.

use std::error::Error;

use glam::Vec3;
use solarity_asset::{
    ArchiveCatalog, AssetStore, AssetStoreHandle, ClientDataRoot, Locale, MapCatalog,
    TerrainTileIndex,
};
use solarity_ecs::{ActiveWorld, WorldBootstrap, WorldMapId};
use solarity_rendering::{WorldCamera, WorldFrustum, WorldScreenWindow};
use solarity_runtime::{RuntimeTerrainCoordinator, RuntimeTerrainPoll};
use wow_adt::AdtVersion;
use wow_adt::builder::AdtBuilder;
use wow_wdt::chunks::MwmoChunk;
use wow_wdt::version::WowVersion;
use wow_wdt::{WdtFile, WdtWriter};

use crate::support::ClientFixture;

/// World entry admits only the exact player ADT and prepares each MCNK once.
#[test]
fn terrain_residency_follows_authoritative_player_tile() -> Result<(), Box<dyn Error>> {
    let map_table = map_table();
    let wdt = terrain_wdt()?;
    let adt = AdtBuilder::new()
        .with_version(AdtVersion::WotLK)
        .add_texture("tileset/fixture/grass.blp")
        .build()?
        .to_bytes()?;
    let fixture = ClientFixture::with_common_files(&[
        ("DBFilesClient\\Map.dbc", &map_table),
        ("World\\Maps\\Northrend\\Northrend.wdt", &wdt),
        ("World\\Maps\\Northrend\\Northrend_30_21.adt", &adt),
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let maps = MapCatalog::load(&mut store)?;
    let assets = AssetStoreHandle::new(store);
    let mut terrain = RuntimeTerrainCoordinator::new(assets, maps);
    let player_position = Vec3::new(1_000.0, 5_800.0, 250.0);
    let world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        0xF130_0000_0000_0001,
        "TerrainFixture",
        player_position,
        0.0,
    ));
    let tile = TerrainTileIndex::new(30, 21).ok_or("fixture tile is invalid")?;

    assert_eq!(
        terrain.synchronize(Some(&world))?,
        RuntimeTerrainPoll::TileLoaded { map_id: 571, tile }
    );
    assert_eq!(terrain.resident_tile().map(|tile| tile.index()), Some(tile));
    assert_eq!(
        terrain.resident_tile().map(|tile| tile.chunks().len()),
        Some(256)
    );
    assert_eq!(
        terrain.synchronize(Some(&world))?,
        RuntimeTerrainPoll::Current { map_id: 571, tile }
    );

    // Visibility consumes an explicit camera; residency does not invent one.
    let frame = WorldCamera::stock(
        Vec3::new(100.0, -16.0, 2.0),
        Vec3::new(99.0, -16.0, 2.0),
        Vec3::Z,
        200.0,
    )
    .frame(1.0)?;
    let visible = terrain.visible_chunks(WorldFrustum::new(frame, WorldScreenWindow::FULL)?)?;
    assert!(!visible.is_empty());

    assert_eq!(terrain.synchronize(None)?, RuntimeTerrainPoll::Idle);
    assert!(terrain.active_map().is_none());
    Ok(())
}

fn terrain_wdt() -> Result<Vec<u8>, Box<dyn Error>> {
    let mut wdt = WdtFile::new(WowVersion::WotLK);
    wdt.mwmo = Some(MwmoChunk::new());
    let entry = wdt
        .main
        .get_mut(30, 21)
        .ok_or("fixture WDT tile is invalid")?;
    entry.set_has_adt(true);
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
    let mut bytes = Vec::with_capacity(20 + fields.len() * 4 + strings.len());
    bytes.extend_from_slice(b"WDBC");
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&66_u32.to_le_bytes());
    bytes.extend_from_slice(&(66_u32 * 4).to_le_bytes());
    bytes.extend_from_slice(&(strings.len() as u32).to_le_bytes());
    for field in fields {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    bytes.extend_from_slice(&strings);
    bytes
}

fn append_string(block: &mut Vec<u8>, value: &str) -> u32 {
    let offset = block.len() as u32;
    block.extend_from_slice(value.as_bytes());
    block.push(0);
    offset
}
