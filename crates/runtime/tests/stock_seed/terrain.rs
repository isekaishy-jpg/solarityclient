//! External integration tests for active-world terrain residency.

use std::error::Error;

use glam::Vec3;
use solarity_asset::{
    ArchiveCatalog, AssetStore, AssetStoreHandle, CharacterAppearanceCatalog, ClientDataRoot,
    CreatureCatalog, Locale, MapCatalog, TerrainTileIndex,
};
use solarity_ecs::{ActiveWorld, WorldBootstrap, WorldMapId};
use solarity_rendering::{WorldCamera, WorldFrustum, WorldScreenWindow};
use solarity_runtime::{
    RuntimePlayerPoll, RuntimePlayerPresentation, RuntimeTerrainCoordinator, RuntimeTerrainPoll,
};
use wow_adt::AdtVersion;
use wow_adt::builder::AdtBuilder;
use wow_wdt::chunks::MwmoChunk;
use wow_wdt::version::WowVersion;
use wow_wdt::{WdtFile, WdtWriter};

use crate::support::{ClientFixture, bootstrap_texture_blp};

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
        ("tileset\\fixture\\grass.blp", &bootstrap_texture_blp()),
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let maps = MapCatalog::load(&mut store)?;
    let creatures = CreatureCatalog::load(&mut store)?;
    let characters = CharacterAppearanceCatalog::load(&mut store)?;
    let assets = AssetStoreHandle::new(store);
    let mut player = RuntimePlayerPresentation::new(assets.clone(), creatures, characters);
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

    // World verification precedes the local player's create-object packet.
    // Presentation waits for those fields instead of inventing a body model.
    assert_eq!(
        player.synchronize(Some(&world))?,
        RuntimePlayerPoll::Pending
    );
    assert!(player.resident_model().is_none());
    assert!(player.camera_pose().is_none());
    assert!(player.world_camera(777.0).is_none());

    assert_eq!(
        terrain.synchronize(Some(&world))?,
        RuntimeTerrainPoll::TileLoaded { map_id: 571, tile }
    );
    assert_eq!(terrain.resident_tile().map(|tile| tile.index()), Some(tile));
    assert_eq!(
        terrain.resident_tile().map(|tile| tile.chunks().len()),
        Some(256)
    );
    let textures = terrain
        .resident_texture_sources()
        .ok_or("resident tile omitted its MTEX sources")?;
    assert_eq!(textures.len(), 1);
    assert_eq!(textures[0].path().as_str(), "TILESET\\FIXTURE\\GRASS.BLP");
    assert_eq!(textures[0].mip_dimensions(0), Some((2, 1)));
    let mesh = terrain
        .resident_mesh_plan()
        .ok_or("resident tile omitted its aggregate mesh plan")?;
    assert_eq!(mesh.tile(), tile);
    assert_eq!(mesh.textures(), [textures[0].path().clone()]);
    assert_eq!(mesh.chunks().len(), 256);
    let first_bounds = mesh.chunks()[0].bounds();
    let ray_x = (first_bounds[0][0] + first_bounds[1][0]) * 0.5;
    let ray_y = (first_bounds[0][1] + first_bounds[1][1]) * 0.5;
    let ray_start = Vec3::new(ray_x, ray_y, first_bounds[1][2] + 10.0);
    let ray_end = Vec3::new(ray_x, ray_y, first_bounds[0][2] - 10.0);
    let collision = terrain
        .trace_collision(ray_start, ray_end, 0.0, 1.0)?
        .ok_or("vertical ray missed fixture terrain")?;
    assert!(collision.fraction() > 0.0 && collision.fraction() < 1.0);
    assert!(collision.normal().z > 0.0);
    assert!(
        terrain
            .trace_collision(ray_end, ray_start, 0.0, 1.0)?
            .is_none()
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
    assert_eq!(player.synchronize(None)?, RuntimePlayerPoll::Idle);
    assert!(terrain.active_map().is_none());
    assert!(terrain.resident_texture_sources().is_none());
    assert!(terrain.resident_mesh_plan().is_none());
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
