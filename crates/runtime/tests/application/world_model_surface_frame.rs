//! Real Vulkan submission through retained WMO owner and portal batch regions.

use std::{error::Error, sync::Arc};

use glam::{Mat4, Vec3, Vec4};
use solarity_asset::{
    AnimationDataCatalog, ArchiveCatalog, AssetStore, AssetStoreHandle, ClientDataRoot,
    GameObjectDisplayCatalog, Locale,
};
use solarity_ecs::{
    ActiveWorld, GameObjectPresentation, ObjectKind, ObjectPresentation, WorldBootstrap,
    WorldMapId, WorldTransform,
};
use solarity_rendering::{
    M2LocalLightState, M2SceneUniform, TerrainSceneUniform, VulkanBootstrap, WorldCamera,
    WorldEnvironmentShadowFrame, WorldEnvironmentShadowState, WorldFrameScene, WorldModelBaseMip,
    WorldModelSceneUniform, WorldModelTextureFiltering, WorldShadowProjection, WorldShadowQuality,
};
use solarity_systems::WorldSceneCameraFrame;

use super::{RuntimeWorldModelMovementOwner, WorldModelFrame, WorldModelSceneGroup};
use crate::application::game_object_coordinator::RuntimeGameObjectPresentation;
use crate::application::terrain_coordinator::world_model_residency::ResidentWorldModelScene;
use crate::application::terrain_frame::shadow::WorldShadowAdmission;
use crate::application::terrain_frame::world_model::WorldModelVisibleFrame;
use crate::test_support::{ClientFixture, SDL_TEST_LOCK, liquid_models};

/// Two disjoint portal windows select right then left, leave the center hidden,
/// and share one source across owners without confusing transforms or fog banks.
#[test]
#[allow(unsafe_code)] // Sole surface ownership transfers from the hidden SDL window.
fn world_model_surface_packets_and_pixels_follow_owner_portal_regions() -> Result<(), Box<dyn Error>>
{
    let (root, group) = surface_files();
    let mut files = liquid_models::files(0, 0, 0, 1, 0);
    files[0].1 = root;
    files[1].1 = group.clone();
    // Keep one missing-MOCV group and one authored-color group so their native
    // callbacks exercise exterior override and selected-bank routing together.
    let mut colored_group = group.clone();
    word(&mut colored_group, 28, 12);
    chunk(
        &mut colored_group,
        b"VCOM",
        &[127, 127, 127, 255].repeat(12),
    );
    let group_size = (colored_group.len() - 20) as u32;
    word(&mut colored_group, 16, group_size);
    files[1].1 = colored_group;
    files.push(("World\\Liquid_001.wmo".to_owned(), group));
    let fixture = ClientFixture::with_common_files(
        &files
            .iter()
            .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
            .collect::<Vec<_>>(),
    )?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let displays = GameObjectDisplayCatalog::load(&mut store)?;
    let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
    let mut objects =
        RuntimeGameObjectPresentation::new(AssetStoreHandle::new(store), displays, animations);
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Surface",
        Vec3::ZERO,
        0.,
    ));
    for (guid, x) in [(90, 0.), (91, 10.)] {
        let entity = world.create_object(
            guid,
            ObjectKind::GameObject,
            Some(WorldTransform::new(Vec3::X * x, 0.)),
            [],
        )?;
        world.storage_mut().add_component(
            entity,
            (
                ObjectPresentation::new(1, 1.),
                GameObjectPresentation::from_fields(42, 0, u32::from_le_bytes([1, 35, 0, 0])),
            ),
        );
    }
    objects.synchronize(Some(&world))?;
    let _guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity WMO surface test", 64, 64)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: Bootstrap enables extensions for this live SDL window.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    // SAFETY: Sole surface ownership transfers; the window outlives the renderer.
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (64, 64), 0) }?;
    let mut frame = WorldModelFrame::prepare(
        &mut renderer,
        &ResidentWorldModelScene::default(),
        WorldModelTextureFiltering::Bilinear,
        WorldModelBaseMip::Zero,
    )?;
    frame.synchronize_game_objects(&mut renderer, objects.frame_input(Some(&world)))?;
    assert_eq!(frame.sources.len(), 1);
    assert_eq!(frame.placements.len(), 2);
    let local_camera = WorldSceneCameraFrame::perspective(
        Vec3::Z * 10.,
        Vec3::ZERO,
        -Vec3::Z,
        Vec3::Y,
        1.,
        1.,
        [0.1, 100.],
    )?;
    let right = local_camera.frustum_for_window([0.4, 0.75, 0.6, 0.9])?;
    let left = local_camera.frustum_for_window([0.4, 0.1, 0.6, 0.25])?;
    let center = local_camera.frustum_for_window([0.4, 0.45, 0.6, 0.55])?;
    let second = RuntimeWorldModelMovementOwner::GameObject {
        identity: world.object_identity(91).ok_or("second owner")?,
    };
    let first = RuntimeWorldModelMovementOwner::GameObject {
        identity: world.object_identity(90).ok_or("first owner")?,
    };
    let mut groups = [
        WorldModelSceneGroup {
            owner: second,
            group: 1,
            indoor_fog: false,
            frusta: vec![right, left, right],
            doodads: Default::default(),
        },
        WorldModelSceneGroup {
            owner: first,
            group: 0,
            indoor_fog: false,
            frusta: vec![center],
            doodads: Default::default(),
        },
        WorldModelSceneGroup {
            owner: RuntimeWorldModelMovementOwner::Static { unique_id: 999 },
            group: 0,
            indoor_fog: false,
            frusta: vec![center],
            doodads: Default::default(),
        },
    ];
    let WorldModelVisibleFrame {
        draws,
        last_group: last,
        ..
    } = frame.prepare_visible_draws(&mut renderer, &groups, 1., Vec3::ZERO, Vec3::ZERO)?;
    assert_eq!(last, Some(1));
    assert_eq!(
        draws
            .iter()
            .map(|draw| draw.index_range())
            .collect::<Vec<_>>(),
        [[30, 6], [18, 6], [6, 6]]
    );
    assert!(draws.iter().all(|draw| draw.mesh() == draws[0].mesh()));
    for (draw, x) in draws.iter().zip([10., 10., 0.]) {
        let bytes = draw.material().to_bytes(Mat4::IDENTITY);
        assert_eq!(f32::from_le_bytes(bytes[48..52].try_into()?), x);
    }
    let camera = WorldCamera::new(
        Vec3::new(10., 0., 10.),
        Vec3::X * 10.,
        Vec3::Y,
        1.,
        0.1,
        100.,
    )
    .frame(1.)?;
    let scene = |fog_parameters| {
        WorldFrameScene::new(
            TerrainSceneUniform::new(
                camera.projection(),
                camera.view(),
                Vec3::ONE,
                Vec3::ZERO,
                Vec3::Z,
            ),
            WorldModelSceneUniform::new(
                camera.projection(),
                camera.view(),
                camera.camera().position(),
                Vec3::ONE,
                Vec3::ZERO,
                Vec3::Z,
                fog_parameters,
            ),
            M2SceneUniform::new(
                camera.projection(),
                camera.view(),
                camera.camera().position(),
                Vec3::ONE,
                Vec3::ZERO,
                Vec3::Z,
                Vec4::ZERO,
                Vec3::ZERO,
                [M2LocalLightState::disabled(); 4],
            ),
        )
    };
    renderer.request_frame_capture()?;
    let report = renderer.present_world_frame(
        scene(Vec4::new(100., 200., 0., 1.)),
        &[],
        &[],
        draws,
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
    )?;
    assert_eq!(report.world_model_draw_count(), 3);
    let image = renderer
        .take_captured_frame()?
        .ok_or("missing surface capture")?;
    // Missing MOCV supplies native 127/255 light; the shader doubles it.
    for (x, expected) in [
        (12, [0, 254, 0, 255]),
        (32, [0, 0, 0, 255]),
        (52, [0, 254, 0, 255]),
    ] {
        let offset = (32 * 64 + x) * 4;
        assert_eq!(image.rgba8()[offset..offset + 4], expected, "pixel {x}");
    }
    // The next frame must release prior acceptance markers and clip regions.
    let WorldModelVisibleFrame {
        draws,
        last_group: last,
        ..
    } = frame.prepare_visible_draws(&mut renderer, &groups[1..2], 1., Vec3::ZERO, Vec3::ZERO)?;
    assert_eq!(last, Some(0));
    assert_eq!(
        draws
            .iter()
            .map(|draw| draw.index_range())
            .collect::<Vec<_>>(),
        [[6, 6]]
    );
    let WorldModelVisibleFrame {
        draws,
        last_group: last,
        ..
    } = frame.prepare_visible_draws(&mut renderer, &[], 1., Vec3::ZERO, Vec3::ZERO)?;
    assert!(draws.is_empty());
    assert_eq!(last, None);

    // Native 7B3F30 chooses the accumulated group flag, independently of the
    // camera's bank. The missing-MOCV side group must still use exterior fog;
    // the authored-color center group follows its selected bank next frame.
    groups[1].owner = second;
    let native = include_str!("../fixtures/world_model_group_fog_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            line.split_whitespace()
                .map(|word| u32::from_str_radix(word, 16))
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(native.len(), 64);
    for row in &native {
        assert_eq!(row.len(), 13);
        let other = native
            .iter()
            .find(|other| other[0] == row[0] ^ 0x8000 && other[1..6] == row[1..6])
            .ok_or("missing opposite native fog bank")?;
        let color = |row: &[u32]| {
            Vec3::new(
                f32::from_bits(row[10]),
                f32::from_bits(row[11]),
                f32::from_bits(row[12]),
            )
        };
        let selected = color(row);
        let opposite = color(other);
        groups[0].indoor_fog = row[0] & 0x8000 != 0;
        groups[1].indoor_fog = !groups[0].indoor_fog;
        let (ordinary, indoor) = if groups[0].indoor_fog {
            (opposite, selected)
        } else {
            (selected, opposite)
        };
        let WorldModelVisibleFrame {
            draws,
            last_group: last,
            ..
        } = frame.prepare_visible_draws(&mut renderer, &groups, 1., ordinary, indoor)?;
        assert_eq!(last, Some(1));
        assert_eq!(draws.len(), 3);
        for (draw, expected) in draws.iter().zip([ordinary, ordinary, opposite]) {
            let bytes = draw.material().to_bytes(camera.view());
            let actual = bytes[96..108]
                .as_chunks::<4>()
                .0
                .iter()
                .map(|word| u32::from_le_bytes(*word))
                .collect::<Vec<_>>();
            assert_eq!(
                actual,
                expected.to_array().map(f32::to_bits),
                "group flag {:08x}",
                row[0]
            );
        }
        let fog_parameters = Vec4::new(
            f32::from_bits(row[6]),
            f32::from_bits(row[7]),
            0.,
            f32::from_bits(row[9]),
        );
        renderer.request_frame_capture()?;
        renderer.present_world_frame(
            scene(fog_parameters),
            &[],
            &[],
            draws,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
        )?;
        let image = renderer
            .take_captured_frame()?
            .ok_or("missing fog capture")?;
        for (x, expected) in [(12, ordinary), (32, opposite), (52, ordinary)] {
            let offset = (32 * 64 + x) * 4;
            for (actual, expected) in image.rgba8()[offset..offset + 3]
                .iter()
                .zip(expected.to_array())
            {
                let expected = (expected * 255.).round() as u8;
                assert!(
                    actual.abs_diff(expected) <= 1,
                    "fog pixel {x}: {actual} != {expected}"
                );
            }
        }
    }
    let shadow_camera = WorldCamera::orthographic(
        Vec3::Z * 20.,
        Vec3::ZERO,
        Vec3::Y,
        [-30., 30.],
        [-30., 30.],
        0.1,
        100.,
    )
    .frame(1.)?;
    for quality in [
        WorldShadowQuality::EnvironmentLow,
        WorldShadowQuality::Cascaded,
    ] {
        let mut state = WorldEnvironmentShadowState::new(quality);
        let updates = state.advance(Vec3::ZERO)?;
        let primary = WorldShadowProjection::primary(
            quality,
            Vec3::ZERO,
            shadow_camera.camera().position(),
            -Vec3::Z,
        )?
        .with_camera_culling(shadow_camera);
        let environment = WorldEnvironmentShadowFrame::new(
            &state,
            updates,
            shadow_camera.camera().position(),
            -Vec3::Z,
        )?;
        let admission = WorldShadowAdmission::new(primary, environment, shadow_camera, -Vec3::Z)?;
        frame.prepare_shadow_draws(&renderer, Some(&admission))?;
        let visible =
            frame.prepare_visible_draws(&mut renderer, &[], 1., Vec3::ZERO, Vec3::ZERO)?;
        assert!(visible.draws.is_empty());
        assert_eq!(
            visible.shadow_draws.len(),
            4,
            "both groups of both moving owners cast their merged opaque ranges without portal visibility"
        );
        assert!(visible.shadow_draws.iter().all(|caster| caster.maps == 8));
    }
    frame.prepare_shadow_draws(&renderer, None)?;
    assert!(
        frame.shadow_draws.is_empty(),
        "disabling clears prior shadow packets"
    );
    renderer.shutdown()?;
    Ok(())
}

/// Two groups share three separated quads with independent signed-i16 bounds.
fn surface_files() -> (Vec<u8>, Vec<u8>) {
    let mut root = Vec::new();
    chunk(&mut root, b"REVM", &17u32.to_le_bytes());
    let mut header = [0; 64];
    word(&mut header, 0, 1);
    word(&mut header, 4, 2);
    word(&mut header, 60, 8); // Preserve authored colors in the colored group.
    let bounds = [-4f32, -1., 0., 4., 1., 0.]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    header[36..60].copy_from_slice(&bounds);
    chunk(&mut root, b"DHOM", &header);
    chunk(&mut root, b"XTOM", &[0]);
    let mut material = [0; 64];
    word(&mut material, 0, 5); // Unlit, fogged, two-sided stock green.
    chunk(&mut root, b"TMOM", &material);
    let mut info = [0; 32];
    word(&mut info, 0, 8);
    info[4..28].copy_from_slice(&bounds);
    word(&mut info, 28, u32::MAX);
    chunk(&mut root, b"IGOM", &info.repeat(2));
    let mut group = Vec::new();
    chunk(&mut group, b"REVM", &17u32.to_le_bytes());
    let mut header = vec![0; 68];
    word(&mut header, 8, 8);
    header[12..36].copy_from_slice(&bounds);
    header[44..46].copy_from_slice(&3u16.to_le_bytes());
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut batches = Vec::new();
    for (index, [low, high]) in [[-4i16, -2], [-1, 1], [2, 4]].into_iter().enumerate() {
        for point in [[low, -1, 0], [high, -1, 0], [high, 1, 0], [low, 1, 0]] {
            vertices.extend(
                point
                    .into_iter()
                    .flat_map(|value| f32::from(value).to_le_bytes()),
            );
        }
        indices.extend(
            [0u16, 1, 2, 0, 2, 3]
                .into_iter()
                .flat_map(|value| (value + index as u16 * 4).to_le_bytes()),
        );
        let mut batch = [0; 24];
        for (axis, value) in [low, -1, 0, high, 1, 0].into_iter().enumerate() {
            batch[axis * 2..axis * 2 + 2].copy_from_slice(&value.to_le_bytes());
        }
        word(&mut batch, 12, index as u32 * 6);
        batch[16..18].copy_from_slice(&6u16.to_le_bytes());
        batch[18..20].copy_from_slice(&(index as u16 * 4).to_le_bytes());
        batch[20..22].copy_from_slice(&(index as u16 * 4 + 3).to_le_bytes());
        batches.extend(batch);
    }
    chunk(&mut header, b"YPOM", &[0x20, 0].repeat(6));
    chunk(&mut header, b"IVOM", &indices);
    chunk(&mut header, b"TVOM", &vertices);
    let normal = [0f32, 0., 1.]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    chunk(&mut header, b"RNOM", &normal.repeat(12));
    chunk(&mut header, b"VTOM", &[0; 96]);
    chunk(&mut header, b"ABOM", &batches);
    chunk(&mut group, b"PGOM", &header);
    (root, group)
}

/// Encodes the original reversed-FourCC chunk layout.
fn chunk(bytes: &mut Vec<u8>, magic: &[u8; 4], payload: &[u8]) {
    bytes.extend(magic);
    bytes.extend((payload.len() as u32).to_le_bytes());
    bytes.extend(payload);
}

/// Writes one fixed-layout native header field.
fn word(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
