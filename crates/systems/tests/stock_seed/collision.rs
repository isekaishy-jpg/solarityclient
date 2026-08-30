//! External stock-compatibility tests for stock scene collision.

use std::error::Error;
use std::sync::Arc;

use glam::Vec3;
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedWorldModel, Locale,
};
use solarity_systems::{PlacedWorldModelCollision, WorldModelCollisionScene};

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

fn push_chunk(bytes: &mut Vec<u8>, magic: [u8; 4], payload: &[u8]) {
    bytes.extend_from_slice(&magic);
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(payload);
}

fn set_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn set_vec3(bytes: &mut [u8], offset: usize, value: [f32; 3]) {
    for (axis, value) in value.into_iter().enumerate() {
        bytes[offset + axis * 4..offset + axis * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
}
