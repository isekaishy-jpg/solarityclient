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
    let adt = append_stacked_liquid_fixture(
        AdtBuilder::new()
            .with_version(AdtVersion::WotLK)
            .add_texture("tileset/fixture/grass.blp")
            .build()?
            .to_bytes()?,
    );
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
    let liquid_x = (32.0 - 21.0 - 0.5 / 128.0) * 533.333_3;
    let liquid_y = (32.0 - 30.0 - 0.5 / 128.0) * 533.333_3;
    let highest = terrain
        .sample_liquid(liquid_x, liquid_y, None)?
        .ok_or("fixture liquid was not sampled")?;
    assert!((highest.height() - 200.0).abs() < 0.001);
    assert_eq!(highest.liquid_type(), 2);
    assert!(highest.is_fishable());
    assert!(!highest.is_deep());
    assert_liquid_height(
        terrain.sample_liquid(liquid_x, liquid_y, Some(50.0))?,
        100.0,
    )?;
    assert_liquid_height(
        terrain.sample_liquid(liquid_x, liquid_y, Some(150.0))?,
        200.0,
    )?;
    assert_liquid_height(
        terrain.sample_liquid(liquid_x, liquid_y, Some(250.0))?,
        200.0,
    )?;
    assert!(
        terrain
            .sample_liquid(liquid_x + 100.0, liquid_y, None)?
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

fn assert_liquid_height(
    sample: Option<solarity_systems::TerrainLiquidSample>,
    expected: f32,
) -> Result<(), Box<dyn Error>> {
    let sample = sample.ok_or("fixture liquid was not sampled")?;
    assert!((sample.height() - expected).abs() < 0.001);
    Ok(())
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

/// Appends two overlapping planar MH2O layers for stacked-surface selection.
fn append_stacked_liquid_fixture(mut adt: Vec<u8>) -> Vec<u8> {
    const INSTANCE_OFFSET: usize = 256 * 12;
    const ATTRIBUTES_OFFSET: usize = INSTANCE_OFFSET + 2 * 24;
    const LOWER_VERTEX_OFFSET: usize = ATTRIBUTES_OFFSET + 16;
    const UPPER_VERTEX_OFFSET: usize = LOWER_VERTEX_OFFSET + 20;
    let mut payload = vec![0_u8; UPPER_VERTEX_OFFSET + 20];
    set_u32(&mut payload, 0, INSTANCE_OFFSET as u32);
    set_u32(&mut payload, 4, 2);
    set_u32(&mut payload, 8, ATTRIBUTES_OFFSET as u32);
    write_liquid_instance(&mut payload, INSTANCE_OFFSET, 100.0, LOWER_VERTEX_OFFSET);
    write_liquid_instance(
        &mut payload,
        INSTANCE_OFFSET + 24,
        200.0,
        UPPER_VERTEX_OFFSET,
    );
    set_u64(&mut payload, ATTRIBUTES_OFFSET, 1);
    write_height_depth_vertices(&mut payload, LOWER_VERTEX_OFFSET, 100.0);
    write_height_depth_vertices(&mut payload, UPPER_VERTEX_OFFSET, 200.0);
    adt.extend_from_slice(b"O2HM");
    adt.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    adt.extend_from_slice(&payload);
    adt
}

fn write_liquid_instance(bytes: &mut [u8], offset: usize, height: f32, vertices: usize) {
    set_u16(bytes, offset, 2);
    set_u16(bytes, offset + 2, 0);
    set_f32(bytes, offset + 4, height);
    set_f32(bytes, offset + 8, height);
    bytes[offset + 14] = 1;
    bytes[offset + 15] = 1;
    set_u32(bytes, offset + 20, vertices as u32);
}

fn write_height_depth_vertices(bytes: &mut [u8], offset: usize, height: f32) {
    for index in 0..4 {
        set_f32(bytes, offset + index * 4, height);
    }
    bytes[offset + 16..offset + 20].fill(u8::MAX);
}

fn set_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn set_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn set_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn set_f32(bytes: &mut [u8], offset: usize, value: f32) {
    set_u32(bytes, offset, value.to_bits());
}
