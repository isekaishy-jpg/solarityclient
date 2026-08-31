//! External integration tests for active-world terrain residency.

use std::error::Error;
use std::io::Cursor;

use glam::Vec3;
use solarity_asset::{
    ArchiveCatalog, AssetStore, AssetStoreHandle, CharacterAppearanceCatalog, ClientDataRoot,
    CreatureCatalog, Locale, MapCatalog, TerrainTileIndex,
};
use solarity_ecs::{ActiveWorld, PlayerViewState, WorldBootstrap, WorldMapId, WorldTransform};
use solarity_rendering::{WorldCamera, WorldFrustum, WorldScreenWindow};
use solarity_runtime::{
    RuntimePlayerPoll, RuntimePlayerPresentation, RuntimeTerrainCoordinator, RuntimeTerrainPoll,
};
use solarity_systems::{
    CameraSubjectGeometry, resolve_camera_subject_height, resolve_player_camera_pose,
};
use wow_adt::AdtVersion;
use wow_adt::builder::AdtBuilder;
use wow_adt::{DoodadPlacement, WmoPlacement};
use wow_m2::chunks::vertex::M2Vertex;
use wow_m2::common::{C2Vector, C3Vector};
use wow_m2::header::M2Header;
use wow_m2::skin::{OldSkinHeader, SkinSubmesh};
use wow_m2::{M2Model, M2Version, OldSkin};
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

/// MCNK references admit one shared MODF generation into camera collision.
#[test]
fn terrain_residency_admits_referenced_world_models() -> Result<(), Box<dyn Error>> {
    const CLIENT_MAP_ORIGIN: f32 = 32.0 * 533.333_3;
    let placement = WmoPlacement {
        name_id: 0,
        unique_id: 7,
        position: [CLIENT_MAP_ORIGIN, 0.0, CLIENT_MAP_ORIGIN],
        rotation: [0.0, 0.0, 0.0],
        extents_min: [CLIENT_MAP_ORIGIN - 1.0, -1.0, CLIENT_MAP_ORIGIN - 1.0],
        extents_max: [CLIENT_MAP_ORIGIN + 1.0, 1.0, CLIENT_MAP_ORIGIN + 1.0],
        flags: 0,
        doodad_set: 0,
        name_set: 0,
        scale: 1024,
    };
    let doodad = DoodadPlacement {
        name_id: 0,
        unique_id: 9,
        position: [CLIENT_MAP_ORIGIN, 0.5, CLIENT_MAP_ORIGIN],
        rotation: [0.0, 0.0, 0.0],
        scale: 1_024,
        flags: 0,
    };
    let adt = add_last_chunk_object_references(
        AdtBuilder::new()
            .with_version(AdtVersion::WotLK)
            .add_texture("tileset/fixture/grass.blp")
            .add_model("World/Fixture/Collision.m2")
            .add_doodad_placement(doodad)
            .add_wmo("World/Wmo/Fixture.wmo")
            .add_wmo_placement(placement)
            .build()?
            .to_bytes()?,
        &[0],
        &[0],
    )?;
    let root_wmo = root_wmo_fixture();
    let group_wmo = group_wmo_fixture();
    let m2 = m2_collision_fixture()?;
    let skin = skin_fixture()?;
    let fixture = ClientFixture::with_common_files(&[
        ("DBFilesClient\\Map.dbc", &map_table()),
        ("World\\Maps\\Northrend\\Northrend.wdt", &terrain_wdt()?),
        ("World\\Maps\\Northrend\\Northrend_30_21.adt", &adt),
        ("tileset\\fixture\\grass.blp", &bootstrap_texture_blp()),
        ("World\\Wmo\\Fixture.wmo", &root_wmo),
        ("World\\Wmo\\Fixture_000.wmo", &group_wmo),
        ("World\\Fixture\\Collision.m2", &m2),
        ("World\\Fixture\\Collision00.skin", &skin),
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let maps = MapCatalog::load(&mut store)?;
    let assets = AssetStoreHandle::new(store);
    let mut terrain = RuntimeTerrainCoordinator::new(assets, maps);
    let world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        0xF130_0000_0000_0001,
        "WorldModelFixture",
        Vec3::new(1_000.0, 5_800.0, 250.0),
        0.0,
    ));

    terrain.synchronize(Some(&world))?;
    assert_eq!(terrain.resident_world_model_count(), 1);
    assert_eq!(terrain.resident_world_model_source_count(), 1);
    assert_eq!(terrain.resident_m2_count(), 2);
    assert_eq!(terrain.resident_m2_source_count(), 1);
    assert_eq!(terrain.resident_m2_collision_count(), 2);
    let m2_hit = terrain
        .trace_m2_camera(
            Vec3::new(-0.25, -0.25, 1.0),
            Vec3::new(-0.25, -0.25, -1.0),
            1.0,
        )?
        .ok_or("camera ray missed resident M2")?;
    assert!((m2_hit - 0.25).abs() < 0.001);
    let hit = terrain
        .trace_world_model_camera(
            Vec3::new(-0.25, -0.25, 1.0),
            Vec3::new(-0.25, -0.25, -1.0),
            1.0,
        )?
        .ok_or("camera ray missed resident WMO")?;
    assert!((hit - 0.5).abs() < 0.001);
    let liquid = terrain
        .sample_world_model_liquid(-1.0, -1.0, Some(1.0))?
        .ok_or("resident WMO liquid was not sampled")?;
    assert!((liquid.height() - 2.0).abs() < 0.001);
    assert_eq!(liquid.liquid_type(), 14);
    assert!(liquid.is_fishable());
    let height = resolve_camera_subject_height(CameraSubjectGeometry::new(Some(1.75), 2.0, 1.0))?;
    let pose = resolve_player_camera_pose(
        WorldTransform::new(Vec3::ZERO, 0.0),
        PlayerViewState::new(2.0, 0.174_532_92, 0.0, 2),
        height,
    )?;
    let resolved = terrain.resolve_player_camera(pose, 1.0, true, true)?;
    assert!((resolved.eye().z - 1.95).abs() < 0.001);

    terrain.disconnect();
    assert_eq!(terrain.resident_world_model_count(), 0);
    assert_eq!(terrain.resident_world_model_source_count(), 0);
    assert_eq!(terrain.resident_m2_count(), 0);
    assert_eq!(terrain.resident_m2_source_count(), 0);
    assert_eq!(terrain.resident_m2_collision_count(), 0);
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

/// Adds MCRF MDDF and MODF references to the final generated MCNK without
/// moving any later indexed terrain chunk. The fixture builder omits MCRF APIs.
fn add_last_chunk_object_references(
    mut adt: Vec<u8>,
    doodads: &[u32],
    world_models: &[u32],
) -> Result<Vec<u8>, Box<dyn Error>> {
    let chunk_start = adt
        .windows(4)
        .rposition(|window| window == b"KNCM")
        .ok_or("fixture ADT omits MCNK")?;
    let old_size = read_u32(&adt, chunk_start + 4)? as usize;
    let chunk_end = chunk_start + 8 + old_size;
    set_u32(&mut adt, chunk_start + 8 + 0x20, (8 + old_size) as u32);
    set_u32(
        &mut adt,
        chunk_start + 8 + 0x10,
        u32::try_from(doodads.len())?,
    );
    set_u32(
        &mut adt,
        chunk_start + 8 + 0x38,
        u32::try_from(world_models.len())?,
    );
    let payload_size = (doodads.len() + world_models.len()) * 4;
    let mut reference = Vec::with_capacity(8 + payload_size);
    reference.extend_from_slice(b"FRCM");
    reference.extend_from_slice(&u32::try_from(payload_size)?.to_le_bytes());
    for index in doodads.iter().chain(world_models) {
        reference.extend_from_slice(&index.to_le_bytes());
    }
    adt.splice(chunk_end..chunk_end, reference);
    let added_size = 8 + payload_size;
    set_u32(
        &mut adt,
        chunk_start + 4,
        u32::try_from(old_size + added_size)?,
    );

    let mcin_start = adt
        .windows(4)
        .position(|window| window == b"NICM")
        .ok_or("fixture ADT omits MCIN")?;
    set_u32(
        &mut adt,
        mcin_start + 8 + 255 * 16 + 4,
        u32::try_from(old_size + added_size)?,
    );
    Ok(adt)
}

fn m2_collision_fixture() -> Result<Vec<u8>, Box<dyn Error>> {
    let mut model = M2Model {
        header: M2Header::new(M2Version::WotLK),
        name: Some("Collision".to_owned()),
        ..M2Model::default()
    };
    model.header.num_skin_profiles = Some(1);
    model.header.bounding_box_min = [-1.0; 3];
    model.header.bounding_box_max = [2.0; 3];
    model.header.bounding_sphere_radius = 3.0;
    model.header.collision_box_min = [0.0, 0.0, -0.1];
    model.header.collision_box_max = [2.0, 2.0, 0.1];
    model.header.collision_sphere_radius = 2.0_f32.sqrt();
    for index in [0_u16, 1, 2] {
        model
            .raw_data
            .bounding_triangles
            .extend_from_slice(&index.to_le_bytes());
    }
    for position in [[0.0_f32, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]] {
        for component in position {
            model
                .raw_data
                .bounding_vertices
                .extend_from_slice(&component.to_le_bytes());
        }
        model.vertices.push(M2Vertex {
            position: C3Vector {
                x: position[0],
                y: position[1],
                z: position[2],
            },
            bone_weights: [0; 4],
            bone_indices: [0; 4],
            normal: C3Vector {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
            tex_coords: C2Vector { x: 0.0, y: 0.0 },
            tex_coords2: Some(C2Vector { x: 0.0, y: 0.0 }),
        });
    }
    let mut cursor = Cursor::new(Vec::new());
    model.write(&mut cursor)?;
    Ok(cursor.into_inner())
}

fn skin_fixture() -> Result<Vec<u8>, Box<dyn Error>> {
    let skin = OldSkin {
        header: OldSkinHeader {
            bone_count_max: 1,
            ..OldSkinHeader::new()
        },
        indices: vec![0, 1, 2],
        triangles: vec![0, 1, 2],
        bone_indices: vec![0; 12],
        submeshes: vec![SkinSubmesh {
            id: 0,
            level: 0,
            vertex_start: 0,
            vertex_count: 3,
            triangle_start: 0,
            triangle_count: 3,
            bone_count: 0,
            bone_start: 0,
            bone_influence: 0,
            center: [0.0; 3],
            sort_center: [0.0; 3],
            bounding_radius: 1.0,
        }],
        batches: Vec::new(),
    };
    let mut cursor = Cursor::new(Vec::new());
    skin.write(&mut cursor)?;
    Ok(cursor.into_inner())
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, Box<dyn Error>> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or("fixture u32 is out of range")?
            .try_into()?,
    ))
}

fn root_wmo_fixture() -> Vec<u8> {
    let mut bytes = Vec::new();
    push_wmo_chunk(&mut bytes, *b"REVM", &17_u32.to_le_bytes());
    let mut header = vec![0_u8; 64];
    set_u32(&mut header, 4, 1);
    set_u32(&mut header, 16, 1);
    set_u32(&mut header, 20, 1);
    set_u32(&mut header, 24, 1);
    set_u32(&mut header, 32, 42);
    set_vec3(&mut header, 36, [-5.0, -5.0, -1.0]);
    set_vec3(&mut header, 48, [5.0, 5.0, 3.0]);
    set_u16(&mut header, 60, 0x4);
    push_wmo_chunk(&mut bytes, *b"DHOM", &header);
    let mut group = Vec::new();
    group.extend_from_slice(&0_u32.to_le_bytes());
    for value in [-5.0_f32, -5.0, -1.0, 5.0, 5.0, 3.0] {
        group.extend_from_slice(&value.to_le_bytes());
    }
    group.extend_from_slice(&(-1_i32).to_le_bytes());
    push_wmo_chunk(&mut bytes, *b"IGOM", &group);
    push_wmo_chunk(&mut bytes, *b"NDOM", b"World\\Fixture\\Collision.mdx\0");
    let mut doodad = Vec::new();
    doodad.extend_from_slice(&0_u32.to_le_bytes());
    for value in [10.0_f32, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0] {
        doodad.extend_from_slice(&value.to_le_bytes());
    }
    doodad.extend_from_slice(&1.0_f32.to_le_bytes());
    doodad.extend_from_slice(&[u8::MAX; 4]);
    push_wmo_chunk(&mut bytes, *b"DDOM", &doodad);
    let mut set = [0_u8; 32];
    let name = b"Set_$DefaultGlobal";
    set[..name.len()].copy_from_slice(name);
    set_u32(&mut set, 24, 1);
    push_wmo_chunk(&mut bytes, *b"SDOM", &set);
    bytes
}

fn group_wmo_fixture() -> Vec<u8> {
    let mut nested = Vec::new();
    push_wmo_chunk(&mut nested, *b"YPOM", &[0x08, 0xff]);
    let mut indices = Vec::new();
    for index in [0_u16, 1, 2] {
        indices.extend_from_slice(&index.to_le_bytes());
    }
    push_wmo_chunk(&mut nested, *b"IVOM", &indices);
    let mut vertices = Vec::new();
    for vertex in [[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
        for value in vertex {
            vertices.extend_from_slice(&value.to_le_bytes());
        }
    }
    push_wmo_chunk(&mut nested, *b"TVOM", &vertices);
    let mut normals = Vec::new();
    for _ in 0..3 {
        for value in [0.0_f32, 0.0, 1.0] {
            normals.extend_from_slice(&value.to_le_bytes());
        }
    }
    push_wmo_chunk(&mut nested, *b"RNOM", &normals);
    let mut node = Vec::new();
    node.extend_from_slice(&4_u16.to_le_bytes());
    node.extend_from_slice(&(-1_i16).to_le_bytes());
    node.extend_from_slice(&(-1_i16).to_le_bytes());
    node.extend_from_slice(&1_u16.to_le_bytes());
    node.extend_from_slice(&0_u32.to_le_bytes());
    node.extend_from_slice(&0.0_f32.to_le_bytes());
    push_wmo_chunk(&mut nested, *b"NBOM", &node);
    push_wmo_chunk(&mut nested, *b"RBOM", &0_u16.to_le_bytes());
    let mut liquid = vec![0_u8; 30];
    set_u32(&mut liquid, 0, 2);
    set_u32(&mut liquid, 4, 2);
    set_u32(&mut liquid, 8, 1);
    set_u32(&mut liquid, 12, 1);
    for _ in 0..4 {
        liquid.extend_from_slice(&[0, 0, 0, 0]);
        liquid.extend_from_slice(&2.0_f32.to_le_bytes());
    }
    liquid.push(0x41);
    push_wmo_chunk(&mut nested, *b"QILM", &liquid);

    let mut group = vec![0_u8; 68];
    set_vec3(&mut group, 12, [-5.0, -5.0, -1.0]);
    set_vec3(&mut group, 24, [5.0, 5.0, 3.0]);
    set_u32(&mut group, 52, 2);
    group.extend_from_slice(&nested);
    let mut bytes = Vec::new();
    push_wmo_chunk(&mut bytes, *b"REVM", &17_u32.to_le_bytes());
    push_wmo_chunk(&mut bytes, *b"PGOM", &group);
    bytes
}

fn push_wmo_chunk(bytes: &mut Vec<u8>, magic: [u8; 4], payload: &[u8]) {
    bytes.extend_from_slice(&magic);
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(payload);
}

fn set_vec3(bytes: &mut [u8], offset: usize, value: [f32; 3]) {
    for (axis, value) in value.into_iter().enumerate() {
        set_f32(bytes, offset + axis * 4, value);
    }
}
