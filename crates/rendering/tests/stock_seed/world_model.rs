//! External stock-compatibility tests for WMO mesh preparation.

use std::error::Error;
use std::sync::Arc;

use glam::{Mat4, Vec3, Vec4};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedWorldModel, Locale,
    WorldModelBatchClass, WorldModelBlendMode,
};
use solarity_rendering::{
    PlacedWorldModelDrawPlan, WorldCamera, WorldFrustum, WorldModelBlendFactor,
    WorldModelBlendState, WorldModelFogMode, WorldModelLightingMode, WorldModelMaterialState,
    WorldModelMaterialUniform, WorldModelMeshPlan, WorldModelRenderVertex, WorldModelSceneUniform,
    WorldModelSurfacePassPlan, WorldScreenWindow,
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

    let material = WorldModelMaterialState::from_material(&model.materials()[0]);
    assert_eq!(material.blend().mode(), WorldModelBlendMode::Mod2x);
    assert_eq!(
        material.blend().color_factors(),
        [
            WorldModelBlendFactor::DestinationColor,
            WorldModelBlendFactor::SourceColor,
        ]
    );
    assert_eq!(
        material.blend().alpha_factors(),
        [
            WorldModelBlendFactor::DestinationAlpha,
            WorldModelBlendFactor::SourceAlpha,
        ]
    );
    assert!(!material.cull_enabled());
    assert!(material.depth_test_enabled());
    assert!(material.depth_write_enabled());
    assert!(!material.is_unlit());
    assert!(!material.is_unfogged());
    assert_eq!(material.texture_clamps(), [true, true]);
    assert_eq!(material.alpha_reference(), 1.0 / 255.0);

    let transition = WorldModelSurfacePassPlan::prepare(
        0x02,
        0,
        WorldModelBatchClass::Transition,
        &model.materials()[0],
    );
    assert!(transition.is_unified());
    assert_eq!(transition.passes().len(), 2);
    assert_eq!(
        transition.passes()[0].fog_mode(),
        WorldModelFogMode::OutdoorColor
    );
    assert_eq!(
        transition.passes()[1].fog_mode(),
        WorldModelFogMode::SceneColor
    );
    assert_eq!(
        transition.passes()[0].material().blend().mode(),
        WorldModelBlendMode::SourceAlphaOpaque
    );
    assert_eq!(
        transition.passes()[0].lighting(),
        WorldModelLightingMode::Exterior
    );
    assert_eq!(
        transition.passes()[1].material().blend().mode(),
        WorldModelBlendMode::InverseSourceAlphaAdd
    );
    assert_eq!(
        transition.passes()[1].lighting(),
        WorldModelLightingMode::RootAmbient
    );

    let scene = WorldModelSceneUniform::new(
        Mat4::IDENTITY,
        Mat4::IDENTITY,
        Vec3::new(1.0, 2.0, 3.0),
        Vec3::new(64.0, 32.0, 128.0) / 255.0,
        Vec3::new(128.0, 64.0, 0.0) / 255.0,
        Vec3::Z,
        Vec4::new(10.0, 500.0, 0.0, 1.0),
    );
    let flattened = scene.flattened_lighting();
    assert_vec3_bytes(flattened[0], [112, 64, 80]);
    assert_vec3_bytes(flattened[1], [96, 48, 64]);
    assert_eq!(scene.to_bytes().len(), WorldModelSceneUniform::BYTE_SIZE);

    let uniform = WorldModelMaterialUniform::new(
        Mat4::IDENTITY,
        model.ambient_color(),
        &model.materials()[0],
        transition.passes()[0],
        1.0,
        Vec3::new(0.1, 0.2, 0.3),
    );
    assert_vec3_bytes(uniform.additive_color(), [63, 31, 15]);
    assert_eq!(uniform.behavior(), [1, 0, 1, 0]);
    assert_eq!(
        uniform.to_bytes(Mat4::IDENTITY).len(),
        WorldModelMaterialUniform::BYTE_SIZE
    );

    let direct_add = WorldModelBlendState::for_mode(WorldModelBlendMode::Add);
    assert_eq!(
        direct_add.color_factors(),
        [
            WorldModelBlendFactor::SourceAlpha,
            WorldModelBlendFactor::One,
        ]
    );

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
    let original = placed.transform();
    let moved = Mat4::from_translation(Vec3::new(10_000.0, 0.0, 0.0))
        * Mat4::from_rotation_x(0.7)
        * original;
    for _ in 0..3 {
        placed.set_transform(moved)?;
        placed.select_visible_draws(visible, &mut draw_indices)?;
        assert!(
            draw_indices.is_empty(),
            "stale placement bounds remained visible"
        );
    }
    placed.set_transform(original)?;
    placed.select_visible_draws(visible, &mut draw_indices)?;
    assert_eq!(draw_indices, [0]);
    assert!(placed.set_transform(Mat4::ZERO).is_err());
    assert_eq!(placed.transform(), original);
    Ok(())
}

fn assert_color(actual: [f32; 4], expected: [u8; 4]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!((actual - f32::from(expected) / 255.0).abs() < 0.0001);
    }
}

fn assert_vec3_bytes(actual: Vec3, expected: [u8; 3]) {
    for (actual, expected) in actual.to_array().into_iter().zip(expected) {
        assert!((actual - f32::from(expected) / 255.0).abs() < 0.0001);
    }
}

/// Native 7AC6A0 uses the exterior count as a prefix length, while colored
/// and unified callbacks use all MOBA records, including transition batches.
#[test]
fn world_model_mesh_plan_uses_native_callback_batch_counts() -> Result<(), Box<dyn Error>> {
    for (root_flags, group_flags, counts, expected) in [
        (0_u16, 0, [1, 1, 1], 1),
        (0, 0, [1, 2, 0], 0),
        (0, 0, [0, 1, 2], 2),
        (0, 0, [0, 0, 3], 3),
        (0, 4, [1, 2, 0], 3),
        (2, 0, [1, 2, 0], 3),
        (2, 4, [1, 1, 1], 3),
        (0, 0, [0, 0, 4], 4),
    ] {
        let mut root = root_fixture();
        set_u16(&mut root, 20 + 60, root_flags);
        let group = group_surface_fixture(group_flags, counts, 3);
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
        let mut store = AssetStore::mount(ArchiveCatalog::discover(
            ClientDataRoot::new(fixture.data_root())?,
            Locale::EnUs,
        )?)?;
        let model =
            DecodedWorldModel::load(&mut store, &AssetPath::new("World\\Wmo\\Render.wmo")?)?;
        let plan = WorldModelMeshPlan::prepare(&model);
        if expected == 4 {
            assert!(matches!(
                plan,
                Err(solarity_rendering::WorldModelMeshPlanError::DrawRange {
                    group_index: 0,
                    batch_index: 4,
                    ..
                })
            ));
            continue;
        }
        let plan = plan?;
        assert_eq!(
            plan.draws().len(),
            expected,
            "flags {root_flags}/{group_flags}, {counts:?}"
        );
        assert_eq!(plan.groups()[0].draw_range(), 0..expected);
        assert_eq!(
            plan.shadow_draws().len(),
            3,
            "7AB760 retains all MOBA batches"
        );
        assert_eq!(plan.groups()[0].shadow_draw_range(), 0..3);
        // Filtering draw callbacks retains the complete resident geometry.
        assert_eq!(plan.vertices().len(), 3);
        assert_eq!(plan.indices(), &[0, 1, 2]);
        if counts[0] == 1 && expected != 0 {
            assert_eq!(plan.draws()[0].class(), WorldModelBatchClass::Transition);
        }
    }
    Ok(())
}

/// 7D82E0 merges only blend-zero groups, retaining indices between MOBA ranges.
#[test]
fn world_model_shadow_ranges_merge_only_entirely_opaque_groups() -> Result<(), Box<dyn Error>> {
    for blend in [0, 1, 2] {
        let mut root = root_fixture();
        let material = root
            .windows(4)
            .position(|bytes| bytes == b"TMOM")
            .ok_or("missing fixture MOMT")?
            + 8;
        set_u32(&mut root, material + 8, blend);
        let group = group_surface_ranges(0, [0, 2, 0], &[(0, 3), (6, 3)]);
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
        let mut store = AssetStore::mount(ArchiveCatalog::discover(
            ClientDataRoot::new(fixture.data_root())?,
            Locale::EnUs,
        )?)?;
        let model =
            DecodedWorldModel::load(&mut store, &AssetPath::new("World\\Wmo\\Render.wmo")?)?;
        let plan = WorldModelMeshPlan::prepare(&model)?;
        assert!(
            plan.draws().is_empty(),
            "ordinary exterior callback has no batches"
        );
        let ranges = plan
            .shadow_draws()
            .iter()
            .map(|draw| [draw.first_index(), draw.index_count()])
            .collect::<Vec<_>>();
        assert_eq!(
            ranges,
            if blend == 0 {
                vec![[0, 9]]
            } else {
                vec![[0, 3], [6, 3]]
            }
        );
        assert_eq!(plan.groups()[0].shadow_draw_range(), 0..ranges.len());
    }
    Ok(())
}

pub(crate) fn root_fixture() -> Vec<u8> {
    let mut bytes = Vec::new();
    push_chunk(&mut bytes, *b"REVM", &17_u32.to_le_bytes());
    let mut header = vec![0_u8; 64];
    set_u32(&mut header, 0, 1);
    set_u32(&mut header, 4, 1);
    set_u32(&mut header, 28, 0xff20_3040);
    set_vec3(&mut header, 36, [-2.0, -3.0, -4.0]);
    set_vec3(&mut header, 48, [2.0, 3.0, 4.0]);
    push_chunk(&mut bytes, *b"DHOM", &header);
    // A valid empty primary string exercises stock's MapObj green image.
    push_chunk(&mut bytes, *b"XTOM", b"\0");
    let mut material = vec![0_u8; 64];
    // MOMT 0x08 and 0x10 are deliberately present: unlike M2, neither changes
    // WMO depth state. 0x04 selects two-sided rendering and 0x40/0x80 clamp.
    set_u32(&mut material, 0, 0xdc);
    set_u32(&mut material, 8, 5);
    set_u32(&mut material, 12, 0);
    set_u32(&mut material, 16, 0xff80_4020);
    push_chunk(&mut bytes, *b"TMOM", &material);
    let mut group = vec![0_u8; 32];
    set_vec3(&mut group, 4, [-1.0, -1.0, -1.0]);
    set_vec3(&mut group, 16, [1.0, 1.0, 1.0]);
    set_u32(&mut group, 28, u32::MAX);
    push_chunk(&mut bytes, *b"IGOM", &group);
    bytes
}

pub(crate) fn group_fixture() -> Vec<u8> {
    group_surface_fixture(4, [1, 0, 0], 1)
}

/// Authors independent MOGP counts and MOBA length for callback selection tests.
fn group_surface_fixture(flags: u32, counts: [u16; 3], batches: usize) -> Vec<u8> {
    group_surface_ranges(flags, counts, &vec![(0, 3); batches])
}

fn group_surface_ranges(flags: u32, counts: [u16; 3], ranges: &[(u32, u16)]) -> Vec<u8> {
    let mut nested = Vec::new();
    let index_count = ranges
        .iter()
        .map(|&(first, count)| first + u32::from(count))
        .max()
        .unwrap_or(3);
    push_chunk(
        &mut nested,
        *b"YPOM",
        &[0x20, 0].repeat(index_count as usize / 3),
    );
    let mut indices = Vec::new();
    for index in (0..index_count).map(|index| (index % 3) as u16) {
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
    let mut batches = Vec::new();
    for &(first, count) in ranges {
        set_u32(&mut batch, 12, first);
        set_u16(&mut batch, 16, count);
        batches.extend_from_slice(&batch);
    }
    push_chunk(&mut nested, *b"ABOM", &batches);

    let mut group = vec![0_u8; 68];
    set_u32(&mut group, 8, flags);
    set_vec3(&mut group, 12, [-1.0, -2.0, -3.0]);
    set_vec3(&mut group, 24, [4.0, 5.0, 6.0]);
    for (index, count) in counts.into_iter().enumerate() {
        set_u16(&mut group, 40 + index * 2, count);
    }
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
