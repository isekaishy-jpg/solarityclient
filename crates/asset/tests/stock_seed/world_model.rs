//! External stock-compatibility tests for WMO root and group decoding.

use std::error::Error;

use solarity_asset::{
    ArchiveCatalog, AssetError, AssetPath, AssetStore, ClientDataRoot, DecodedWorldModel, Locale,
};

use crate::support::{Fixture, FixtureFile};

/// A version-17 root admits independently resolved collision-ready groups.
#[test]
fn world_model_loads_stock_group_geometry_and_bsp() -> Result<(), Box<dyn Error>> {
    let root_wmo = root_fixture(1);
    let group_wmo = group_fixture(0x08);
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "World\\Wmo\\Fixture.wmo",
            bytes: &root_wmo,
        },
        FixtureFile {
            archive: "patch-2.MPQ",
            path: "World\\Wmo\\Fixture_000.wmo",
            bytes: &group_wmo,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let path = AssetPath::new("World\\Wmo\\Fixture.wmo")?;

    let model = DecodedWorldModel::load(&mut store, &path)?;
    assert_eq!(model.path(), &path);
    assert_eq!(model.world_model_id(), 42);
    assert_eq!(model.flags(), 0x8);
    assert_eq!(model.bounds(), [[-2.0, -3.0, -4.0], [2.0, 3.0, 4.0]]);
    assert_eq!(model.groups().len(), 1);
    let group = &model.groups()[0];
    assert_eq!(group.index(), 0);
    assert_eq!(group.path().as_str(), "WORLD\\WMO\\FIXTURE_000.WMO");
    assert_eq!(
        group.source().relative_path(),
        std::path::Path::new("patch-2.MPQ")
    );
    assert_eq!(
        group.vertices(),
        &[[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]]
    );
    assert_eq!(group.indices(), &[0, 1, 2]);
    assert_eq!(group.polygons().len(), 1);
    assert!(group.polygons()[0].is_collidable());
    assert!(group.polygons()[0].is_camera_collidable());
    assert!(!group.polygons()[0].is_renderable());
    assert_eq!(group.bsp_nodes().len(), 1);
    assert_eq!(group.bsp_nodes()[0].face_count(), 1);
    assert_eq!(group.bsp_faces(), &[0]);
    Ok(())
}

/// MOPY's no-camera flag remains distinct from ordinary world collision.
#[test]
fn world_model_preserves_stock_no_camera_collision_flag() -> Result<(), Box<dyn Error>> {
    let root_wmo = root_fixture(1);
    let group_wmo = group_fixture(0x0a);
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "World\\Wmo\\Fixture.wmo",
            bytes: &root_wmo,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "World\\Wmo\\Fixture_000.wmo",
            bytes: &group_wmo,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let model = DecodedWorldModel::load(&mut store, &AssetPath::new("World\\Wmo\\Fixture.wmo")?)?;

    assert!(model.groups()[0].polygons()[0].is_collidable());
    assert!(!model.groups()[0].polygons()[0].is_camera_collidable());
    Ok(())
}

/// Post-build chunks fail before the dependency can silently skip them.
#[test]
fn world_model_rejects_unknown_root_chunks() -> Result<(), Box<dyn Error>> {
    let mut root_wmo = root_fixture(0);
    push_chunk(&mut root_wmo, *b"DIAM", &[]);
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "World\\Wmo\\Fixture.wmo",
        bytes: &root_wmo,
    }])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let path = AssetPath::new("World\\Wmo\\Fixture.wmo")?;

    assert!(matches!(
        DecodedWorldModel::load(&mut store, &path),
        Err(AssetError::WorldModelDecode { message, .. })
            if message.contains("unknown build-12340 chunk")
    ));
    Ok(())
}

fn root_fixture(group_count: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    push_chunk(&mut bytes, *b"REVM", &17_u32.to_le_bytes());
    let mut header = vec![0_u8; 64];
    set_u32(&mut header, 4, group_count);
    set_u32(&mut header, 32, 42);
    set_vec3(&mut header, 36, [-2.0, -3.0, -4.0]);
    set_vec3(&mut header, 48, [2.0, 3.0, 4.0]);
    set_u16(&mut header, 60, 0x8);
    push_chunk(&mut bytes, *b"DHOM", &header);
    let mut groups = Vec::with_capacity(group_count as usize * 32);
    for _ in 0..group_count {
        groups.extend_from_slice(&0_u32.to_le_bytes());
        for value in [-1.0_f32, -1.0, -1.0, 1.0, 1.0, 1.0] {
            groups.extend_from_slice(&value.to_le_bytes());
        }
        groups.extend_from_slice(&(-1_i32).to_le_bytes());
    }
    push_chunk(&mut bytes, *b"IGOM", &groups);
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

    let mut group = vec![0_u8; 68];
    set_vec3(&mut group, 12, [-1.0, -1.0, -1.0]);
    set_vec3(&mut group, 24, [1.0, 1.0, 1.0]);
    group.extend_from_slice(&nested);
    let mut bytes = Vec::new();
    push_chunk(&mut bytes, *b"REVM", &17_u32.to_le_bytes());
    push_chunk(&mut bytes, *b"PGOM", &group);
    bytes
}

fn push_chunk(bytes: &mut Vec<u8>, magic: [u8; 4], payload: &[u8]) {
    bytes.extend_from_slice(&magic);
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(payload);
}

fn set_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn set_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn set_vec3(bytes: &mut [u8], offset: usize, value: [f32; 3]) {
    for (axis, value) in value.into_iter().enumerate() {
        bytes[offset + axis * 4..offset + axis * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
}
