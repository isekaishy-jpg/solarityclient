//! External stock-compatibility tests for stock scene collision.

use std::error::Error;
use std::sync::Arc;

use glam::Vec3;
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedWorldModel, Locale,
};
use solarity_systems::{
    PlacedWorldModelCollision, PlacedWorldModelLiquid, WorldModelCollisionScene,
    WorldModelLiquidScene,
};

use crate::support::{Fixture, FixtureFile};

/// Placed WMO camera rays traverse MOBN and honor MOPY no-camera faces.
#[test]
fn placed_world_model_uses_stock_camera_collision_faces() -> Result<(), Box<dyn Error>> {
    let root = root_fixture();
    let collidable_group = group_fixture(0x08);
    let no_camera_group = group_fixture(0x0a);
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "World\\Wmo\\Collision.wmo",
            bytes: &root,
        },
        FixtureFile {
            path: "World\\Wmo\\Collision_000.wmo",
            bytes: &collidable_group,
        },
        FixtureFile {
            path: "World\\Wmo\\NoCamera.wmo",
            bytes: &root,
        },
        FixtureFile {
            path: "World\\Wmo\\NoCamera_000.wmo",
            bytes: &no_camera_group,
        },
    ])?;
    let data_root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(data_root, Locale::EnUs)?)?;
    let model = Arc::new(DecodedWorldModel::load(
        &mut store,
        &AssetPath::new("World\\Wmo\\Collision.wmo")?,
    )?);
    let mut scene = WorldModelCollisionScene::new();
    scene.add(PlacedWorldModelCollision::prepare(
        model,
        Vec3::new(10.0, 20.0, 30.0),
        Vec3::new(0.0, -180.0, 0.0),
        1.0,
    )?);
    let start = Vec3::new(10.5, 20.5, 31.0);
    let end = Vec3::new(10.5, 20.5, 29.0);
    assert_eq!(scene.trace_camera(start, end, 1.0)?, Some(0.5));

    let no_camera = Arc::new(DecodedWorldModel::load(
        &mut store,
        &AssetPath::new("World\\Wmo\\NoCamera.wmo")?,
    )?);
    let mut filtered = WorldModelCollisionScene::new();
    filtered.add(PlacedWorldModelCollision::prepare(
        no_camera,
        Vec3::new(10.0, 20.0, 30.0),
        Vec3::new(0.0, -180.0, 0.0),
        1.0,
    )?);
    assert!(filtered.trace_camera(start, end, 1.0)?.is_none());
    Ok(())
}

/// Placed MLIQ tiles use the MODF transform and authored tile behavior.
#[test]
fn placed_world_model_samples_stock_liquid_tiles() -> Result<(), Box<dyn Error>> {
    let mut root = root_fixture();
    set_u16(&mut root, 80, 0x4);
    let group = group_liquid_fixture();
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "World\\Wmo\\Liquid.wmo",
            bytes: &root,
        },
        FixtureFile {
            path: "World\\Wmo\\Liquid_000.wmo",
            bytes: &group,
        },
    ])?;
    let data_root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(data_root, Locale::EnUs)?)?;
    let model = Arc::new(DecodedWorldModel::load(
        &mut store,
        &AssetPath::new("World\\Wmo\\Liquid.wmo")?,
    )?);
    let mut scene = WorldModelLiquidScene::new();
    scene.add(PlacedWorldModelLiquid::prepare(
        model,
        Vec3::new(10.0, 20.0, 30.0),
        Vec3::new(0.0, -180.0, 0.0),
        1.0,
    )?);

    let sample = scene
        .sample(11.0, 21.0, Some(31.0))?
        .ok_or("placed MLIQ surface was not sampled")?;
    assert!((sample.height() - 32.0).abs() < 0.001);
    assert_eq!(sample.liquid_type(), 14);
    assert!(sample.is_fishable());
    assert!(scene.sample(20.0, 20.0, None)?.is_none());
    Ok(())
}

fn root_fixture() -> Vec<u8> {
    let mut bytes = Vec::new();
    push_chunk(&mut bytes, *b"REVM", &17_u32.to_le_bytes());
    let mut header = vec![0_u8; 64];
    set_u32(&mut header, 4, 1);
    set_vec3(&mut header, 36, [-1.0, -1.0, -1.0]);
    set_vec3(&mut header, 48, [2.0, 2.0, 1.0]);
    push_chunk(&mut bytes, *b"DHOM", &header);
    let mut group = Vec::new();
    group.extend_from_slice(&0_u32.to_le_bytes());
    for value in [-1.0_f32, -1.0, -1.0, 2.0, 2.0, 1.0] {
        group.extend_from_slice(&value.to_le_bytes());
    }
    group.extend_from_slice(&(-1_i32).to_le_bytes());
    push_chunk(&mut bytes, *b"IGOM", &group);
    bytes
}

fn group_fixture(polygon_flags: u8) -> Vec<u8> {
    let mut nested = Vec::new();
    push_chunk(&mut nested, *b"YPOM", &[polygon_flags, 0xff]);
    let mut indices = Vec::new();
    for index in [0_u16, 1, 2] {
        indices.extend_from_slice(&index.to_le_bytes());
    }
    push_chunk(&mut nested, *b"IVOM", &indices);
    let mut vertices = Vec::new();
    for vertex in [[0.0_f32, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]] {
        for value in vertex {
            vertices.extend_from_slice(&value.to_le_bytes());
        }
    }
    push_chunk(&mut nested, *b"TVOM", &vertices);
    let mut normals = Vec::new();
    for _ in 0..3 {
        for value in [0.0_f32, 0.0, 1.0] {
            normals.extend_from_slice(&value.to_le_bytes());
        }
    }
    push_chunk(&mut nested, *b"RNOM", &normals);
    let mut node = Vec::new();
    node.extend_from_slice(&4_u16.to_le_bytes());
    node.extend_from_slice(&(-1_i16).to_le_bytes());
    node.extend_from_slice(&(-1_i16).to_le_bytes());
    node.extend_from_slice(&1_u16.to_le_bytes());
    node.extend_from_slice(&0_u32.to_le_bytes());
    node.extend_from_slice(&0.0_f32.to_le_bytes());
    push_chunk(&mut nested, *b"NBOM", &node);
    push_chunk(&mut nested, *b"RBOM", &0_u16.to_le_bytes());
    let mut container = vec![0_u8; 68];
    set_vec3(&mut container, 12, [-1.0, -1.0, -1.0]);
    set_vec3(&mut container, 24, [2.0, 2.0, 1.0]);
    container.extend_from_slice(&nested);
    let mut bytes = Vec::new();
    push_chunk(&mut bytes, *b"REVM", &17_u32.to_le_bytes());
    push_chunk(&mut bytes, *b"PGOM", &container);
    bytes
}

fn group_liquid_fixture() -> Vec<u8> {
    let mut bytes = group_fixture(0x08);
    let mut liquid = vec![0_u8; 30];
    set_u32(&mut liquid, 0, 2);
    set_u32(&mut liquid, 4, 2);
    set_u32(&mut liquid, 8, 1);
    set_u32(&mut liquid, 12, 1);
    set_vec3(&mut liquid, 16, [0.0, 0.0, 0.0]);
    for _ in 0..4 {
        liquid.extend_from_slice(&[0, 0, 0, 0]);
        liquid.extend_from_slice(&2.0_f32.to_le_bytes());
    }
    liquid.push(0x41);
    let mut chunk = Vec::new();
    push_chunk(&mut chunk, *b"QILM", &liquid);
    let old_size = u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    bytes.extend_from_slice(&chunk);
    set_u32(&mut bytes, 16, old_size + chunk.len() as u32);
    // MVER occupies 12 bytes and the MOGP header occupies another 8.
    set_u32(&mut bytes, 20 + 52, 2);
    bytes
}

fn push_chunk(bytes: &mut Vec<u8>, magic: [u8; 4], payload: &[u8]) {
    bytes.extend_from_slice(&magic);
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(payload);
}

fn set_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn set_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn set_vec3(bytes: &mut [u8], offset: usize, value: [f32; 3]) {
    for (axis, value) in value.into_iter().enumerate() {
        bytes[offset + axis * 4..offset + axis * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
}
