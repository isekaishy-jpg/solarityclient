//! External stock-compatibility tests for WMO mesh preparation.

use std::error::Error;
use std::sync::Arc;

use glam::Vec3;
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedWorldModel, Locale,
    WorldModelBatchClass,
};
use solarity_rendering::{
    PlacedWorldModelDrawPlan, WorldCamera, WorldFrustum, WorldModelMeshPlan,
    WorldModelRenderVertex, WorldScreenWindow,
};

use crate::support::{Fixture, FixtureFile};

/// Group-local WotLK geometry combines once with byte-exact MOCV fixup.
#[test]
fn world_model_mesh_plan_combines_stock_surface_ranges() -> Result<(), Box<dyn Error>> {
    let root = root_fixture();
    let group = group_fixture();
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "World\\Wmo\\Render.wmo",
            bytes: &root,
        },
        FixtureFile {
            path: "World\\Wmo\\Render_000.wmo",
            bytes: &group,
        },
    ])?;
    let data_root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(data_root, Locale::EnUs)?)?;
    let model = DecodedWorldModel::load(&mut store, &AssetPath::new("World\\Wmo\\Render.wmo")?)?;

    let plan = WorldModelMeshPlan::prepare(&model)?;
    assert_eq!(plan.path(), model.path());
    assert_eq!(plan.vertices().len(), 3);
    assert_eq!(plan.indices(), &[0, 1, 2]);
    assert_eq!(plan.materials().len(), 1);
    assert_eq!(plan.groups().len(), 1);
    assert_eq!(plan.groups()[0].group_index(), 0);
    assert_eq!(plan.groups()[0].vertex_range(), [0, 3]);
    assert_eq!(plan.groups()[0].index_range(), [0, 3]);
    assert_eq!(plan.draws().len(), 1);
    assert_eq!(plan.draws()[0].first_index(), 0);
    assert_eq!(plan.draws()[0].index_count(), 3);
    assert_eq!(plan.draws()[0].material_id(), 0);
    assert_eq!(plan.draws()[0].class(), WorldModelBatchClass::Transition);
    assert_eq!(plan.draws()[0].bounds(), [[-1, -2, -3], [4, 5, 6]]);

    let first = plan.vertices()[0];
    assert_eq!(first.texture_coordinates(), [[0.0, 0.0], [0.25, 0.75]]);
    assert_color(first.color(), [127, 64, 0, 17]);
    assert_color(first.blend_color(), [3, 2, 1, 4]);
    assert_color(plan.vertices()[1].color(), [150, 255, 255, 255]);
    assert_eq!(
        plan.vertex_bytes().len(),
        plan.vertices().len() * WorldModelRenderVertex::BYTE_SIZE
    );
    assert_eq!(plan.index_bytes(), [0_u8, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0]);

    let mut placed = PlacedWorldModelDrawPlan::prepare(
        Arc::new(plan),
        Vec3::new(10.0, 20.0, 30.0),
        Vec3::new(0.0, -180.0, 0.0),
        1.0,
    )?;
    assert_eq!(
        placed.transform().transform_point3(Vec3::ZERO),
        Vec3::new(10.0, 20.0, 30.0)
    );
    let visible = WorldFrustum::new(
        WorldCamera::stock(
            Vec3::new(10.0, 20.0, 50.0),
            Vec3::new(10.0, 20.0, 30.0),
            Vec3::Y,
            100.0,
        )
        .frame(1.0)?,
        WorldScreenWindow::FULL,
    )?;
    let mut draw_indices = Vec::new();
    placed.select_visible_draws(visible, &mut draw_indices)?;
    assert_eq!(draw_indices, [0]);

    let behind = WorldFrustum::new(
        WorldCamera::stock(
            Vec3::new(10.0, 20.0, 50.0),
            Vec3::new(10.0, 20.0, 60.0),
            Vec3::Y,
            100.0,
        )
        .frame(1.0)?,
        WorldScreenWindow::FULL,
    )?;
    placed.select_visible_draws(behind, &mut draw_indices)?;
    assert!(draw_indices.is_empty());
    Ok(())
}

fn assert_color(actual: [f32; 4], expected: [u8; 4]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!((actual - f32::from(expected) / 255.0).abs() < 0.0001);
    }
}

fn root_fixture() -> Vec<u8> {
    let mut bytes = Vec::new();
    push_chunk(&mut bytes, *b"REVM", &17_u32.to_le_bytes());
    let mut header = vec![0_u8; 64];
    set_u32(&mut header, 0, 1);
    set_u32(&mut header, 4, 1);
    set_vec3(&mut header, 36, [-2.0, -3.0, -4.0]);
    set_vec3(&mut header, 48, [2.0, 3.0, 4.0]);
    push_chunk(&mut bytes, *b"DHOM", &header);
    push_chunk(&mut bytes, *b"XTOM", b"wall.blp\0");
    let mut material = vec![0_u8; 64];
    set_u32(&mut material, 12, 0);
    push_chunk(&mut bytes, *b"TMOM", &material);
    let mut group = vec![0_u8; 32];
    set_vec3(&mut group, 4, [-1.0, -1.0, -1.0]);
    set_vec3(&mut group, 16, [1.0, 1.0, 1.0]);
    set_u32(&mut group, 28, u32::MAX);
    push_chunk(&mut bytes, *b"IGOM", &group);
    bytes
}

fn group_fixture() -> Vec<u8> {
    let mut nested = Vec::new();
    push_chunk(&mut nested, *b"YPOM", &[0x20, 0]);
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
    let mut primary_uv = Vec::new();
    for value in [0.0_f32, 0.0, 1.0, 0.0, 0.0, 1.0] {
        primary_uv.extend_from_slice(&value.to_le_bytes());
    }
    push_chunk(&mut nested, *b"VTOM", &primary_uv);
    let mut secondary_uv = Vec::new();
    for value in [0.25_f32, 0.75, 0.5, 0.5, 0.75, 0.25] {
        secondary_uv.extend_from_slice(&value.to_le_bytes());
    }
    push_chunk(&mut nested, *b"VTOM", &secondary_uv);
    push_chunk(
        &mut nested,
        *b"VCOM",
        &[1, 129, 255, 17, 255, 200, 100, 128, 96, 64, 32, 255],
    );
    push_chunk(
        &mut nested,
        *b"VCOM",
        &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
    );
    let mut batch = vec![0_u8; 24];
    for (value, offset) in [(-1_i16, 0), (-2, 2), (-3, 4), (4, 6), (5, 8), (6, 10)] {
        set_i16(&mut batch, offset, value);
    }
    set_u16(&mut batch, 16, 3);
    // Stock's color fixer uses the transition range's final vertex, which is
    // independent of the batch's submitted index range.
    set_u16(&mut batch, 20, 0);
    push_chunk(&mut nested, *b"ABOM", &batch);

    let mut group = vec![0_u8; 68];
    set_vec3(&mut group, 12, [-1.0, -2.0, -3.0]);
    set_vec3(&mut group, 24, [4.0, 5.0, 6.0]);
    set_u16(&mut group, 40, 1);
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

fn set_i16(bytes: &mut [u8], offset: usize, value: i16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
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
