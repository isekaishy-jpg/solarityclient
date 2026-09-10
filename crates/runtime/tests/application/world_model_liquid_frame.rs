//! Replicated WMO liquid packets, pixels, shared factories and retirement.

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
    LiquidDepthTexture, LiquidDepthTextureKind, LiquidDrawMaterial, LiquidFog, LiquidFrame,
    LiquidLighting, LiquidPreparedDraw, LiquidShaderUniform, M2LocalLightState, M2SceneUniform,
    TerrainSceneUniform, VulkanBootstrap, VulkanRenderer, WorldCamera, WorldCameraFrame,
    WorldFrameScene, WorldModelBaseMip, WorldModelSceneUniform, WorldModelTextureFiltering,
};

use super::WorldModelFrame;
use crate::application::game_object_coordinator::RuntimeGameObjectPresentation;
use crate::application::terrain_coordinator::WorldModelSceneGroup;
use crate::application::terrain_coordinator::world_model_residency::ResidentWorldModelScene;
use crate::test_support::{ClientFixture, SDL_TEST_LOCK, liquid_models};

/// Full source admission and real Vulkan pixels follow two replicated transforms.
#[test]
fn replicated_world_model_water_moves_shares_and_retires_its_mesh() -> Result<(), Box<dyn Error>> {
    for interior in [false, true] {
        replicated_water(interior)?;
    }
    Ok(())
}

#[allow(unsafe_code)] // The hidden SDL test surface transfers to Vulkan ownership.
fn replicated_water(interior: bool) -> Result<(), Box<dyn Error>> {
    let flags = if interior { 0 } else { 0x48 };
    let mut files = files_with_dry_group(flags)?;
    for (path, bytes) in &mut files {
        if path.ends_with(".blp") {
            let length = bytes.len();
            bytes[length - 8..].copy_from_slice(&[0, 0, 0, 255, 0, 0, 0, 255]);
        }
    }
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
        "Liquid",
        Vec3::ZERO,
        0.,
    ));
    for (guid, position) in [(90, Vec3::ZERO), (91, Vec3::X * 30.)] {
        let entity = world.create_object(
            guid,
            ObjectKind::GameObject,
            Some(WorldTransform::new(position, 0.)),
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
        .window("Solarity WMO liquid test", 32, 32)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: Bootstrap enables extensions for this live SDL window.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    // SAFETY: Sole surface ownership transfers; the window outlives the renderer.
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (32, 32), 0) }?;
    let mut frame = WorldModelFrame::prepare(
        &mut renderer,
        &ResidentWorldModelScene::default(),
        WorldModelTextureFiltering::Bilinear,
        WorldModelBaseMip::Zero,
    )?;
    frame.synchronize_game_objects(&mut renderer, objects.frame_input(Some(&world)))?;
    assert_eq!(
        frame.sources.len(),
        1,
        "shared decoded roots retain one liquid factory"
    );
    assert_eq!(frame.placements.len(), 2);
    assert_eq!(
        frame.sources[0]
            .as_ref()
            .ok_or("GPU source")?
            .liquid_indices,
        [None, Some(0)],
        "MOGP liquid flag gates the factory; group and compact batch indices differ"
    );
    let mesh = frame.sources[0].as_ref().ok_or("GPU source")?.liquids[0].mesh();
    let camera = camera(Vec3::new(2., 2., 1.))?;
    let mut groups = frame
        .placements
        .iter()
        .map(|placement| WorldModelSceneGroup {
            owner: placement.owner.scene_owner(),
            group: 1,
            indoor_fog: false,
            // Liquid admission does not require a surviving MOBA portal clip.
            frusta: Vec::new(),
        })
        .collect::<Vec<_>>();
    let draws = prepare_draws(&frame, &renderer, camera, &groups, fog(), Vec3::ZERO)?;
    assert_eq!(
        draws.len(),
        2,
        "both admitted owners submit, including the off-camera liquid"
    );
    let tint = if interior {
        [51, 85, 119, 255]
    } else {
        [0, 0, 0, 255]
    };
    capture(&mut renderer, &draws, tint)?;
    assert!(
        prepare_draws(&frame, &renderer, camera, &[], fog(), Vec3::ZERO)?.is_empty(),
        "resident liquid without group admission emits no packet"
    );
    groups[0].group = 0;
    assert!(
        prepare_draws(&frame, &renderer, camera, &groups[..1], fog(), Vec3::ZERO)?.is_empty(),
        "admitting a dry group cannot draw another group's water"
    );
    groups[0].group = 1;
    let native = include_str!("../fixtures/world_model_liquid_fog_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            line.split_whitespace()
                .map(|word| u32::from_str_radix(word, 16))
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<Vec<_>, _>>()?;
    for row in native.iter().filter(|row| row[1] == u32::from(interior)) {
        let other = native
            .iter()
            .find(|other| other[0] != row[0] && other[1..7] == row[1..7])
            .ok_or("opposite native liquid fog bank")?;
        let color = |row: &[u32]| Vec3::from_array([row[11], row[12], row[13]].map(f32::from_bits));
        let selected = color(row);
        let opposite = color(other);
        groups[0].indoor_fog = row[0] != 0;
        groups[1].indoor_fog = !groups[0].indoor_fog;
        let (ordinary, indoor) = if groups[0].indoor_fog {
            (opposite, selected)
        } else {
            (selected, opposite)
        };
        let inverse = f32::from_bits(row[9]);
        let fog = LiquidFog::new(
            Vec3::new(
                inverse,
                f32::from_bits(row[8]) * inverse,
                f32::from_bits(row[10]),
            ),
            indoor,
        );
        for (index, expected) in [selected, opposite].into_iter().enumerate() {
            let center = frame.placements[index]
                .plan
                .transform()
                .transform_point3(Vec3::new(2., 2., 1.));
            let draws = prepare_draws(
                &frame,
                &renderer,
                self::camera(center)?,
                &groups,
                fog,
                ordinary,
            )?;
            assert_eq!(draws.len(), 2);
            let rgb = expected
                .to_array()
                .map(|channel| (channel * 255.).round() as u8);
            capture(&mut renderer, &draws, [rgb[0], rgb[1], rgb[2], 255])?;
        }
    }
    world.update_transform(
        90,
        WorldTransform::new(Vec3::X * 15., std::f32::consts::FRAC_PI_2),
    )?;
    objects.synchronize(Some(&world))?;
    frame.synchronize_game_objects(&mut renderer, objects.frame_input(Some(&world)))?;
    frame.update_game_object_states(objects.frame_input(Some(&world)))?;
    let moved = frame.placements.iter().find(|placement| matches!(placement.owner, super::WorldModelGpuPlacementOwner::GameObject {identity, ..} if identity.guid()==90)).ok_or("moved placement")?;
    let center = moved
        .plan
        .transform()
        .transform_point3(Vec3::new(2., 2., 1.));
    let draws = prepare_draws(
        &frame,
        &renderer,
        self::camera(center)?,
        &groups,
        fog(),
        Vec3::ZERO,
    )?;
    capture(&mut renderer, &draws, tint)?;
    assert_eq!(
        frame.sources[0].as_ref().ok_or("GPU source")?.liquids[0].mesh(),
        mesh
    );
    world.remove_object(90)?;
    objects.synchronize(Some(&world))?;
    frame.synchronize_game_objects(&mut renderer, objects.frame_input(Some(&world)))?;
    assert_eq!(
        frame.sources.len(),
        1,
        "remaining instance retains the shared factory"
    );
    assert_eq!(
        prepare_draws(&frame, &renderer, camera, &groups, fog(), Vec3::ZERO)?.len(),
        1
    );
    world.remove_object(91)?;
    objects.synchronize(Some(&world))?;
    frame.synchronize_game_objects(&mut renderer, objects.frame_input(Some(&world)))?;
    assert!(frame.sources.is_empty());
    assert!(prepare_draws(&frame, &renderer, camera, &groups, fog(), Vec3::ZERO)?.is_empty());
    let surface = renderer.upload_stock_m2_failure()?;
    assert!(
        renderer
            .prepare_liquid_draw(
                mesh,
                LiquidDrawMaterial::Water(LiquidDepthTextureKind::WorldModel),
                surface,
                LiquidShaderUniform::new(
                    Mat4::IDENTITY,
                    Mat4::IDENTITY,
                    Mat4::IDENTITY,
                    Mat4::IDENTITY,
                    lighting(),
                    fog()
                )
            )
            .is_err(),
        "departed factory handle is invalidated immediately"
    );
    renderer.shutdown()?;
    Ok(())
}

/// Keep MLIQ bytes in group zero but clear its native admission flag; group one is wet.
fn files_with_dry_group(flags: u32) -> Result<FixtureFiles, Box<dyn Error>> {
    let mut files = liquid_models::files(flags, flags, 0, 1, 0xff335577);
    let root = &mut files
        .iter_mut()
        .find(|(path, _)| path == "World\\Liquid.wmo")
        .ok_or("root fixture")?
        .1;
    let mut rebuilt = Vec::new();
    let mut offset = 0;
    while offset < root.len() {
        let magic = &root[offset..offset + 4];
        let length = u32::from_le_bytes(root[offset + 4..offset + 8].try_into()?) as usize;
        let mut payload = root[offset + 8..offset + 8 + length].to_vec();
        if magic == b"DHOM" {
            payload[4..8].copy_from_slice(&2u32.to_le_bytes());
        } else if magic == b"IGOM" {
            payload.extend_from_within(..);
        }
        rebuilt.extend(magic);
        rebuilt.extend((payload.len() as u32).to_le_bytes());
        rebuilt.extend(payload);
        offset += length + 8;
    }
    *root = rebuilt;
    let wet = files
        .iter_mut()
        .find(|(path, _)| path.ends_with("_000.wmo"))
        .ok_or("group fixture")?;
    let mut dry = wet.1.clone();
    // MVER occupies 12 bytes; MOGP's payload starts at 20 and its flags at +8.
    dry[28..32].copy_from_slice(&flags.to_le_bytes());
    wet.0 = "World\\Liquid_001.wmo".to_owned();
    files.push(("World\\Liquid_000.wmo".to_owned(), dry));
    Ok(files)
}

type FixtureFiles = Vec<(String, Vec<u8>)>;

/// A tight view of the interior of the transformed liquid cell.
fn camera(center: Vec3) -> Result<WorldCameraFrame, Box<dyn Error>> {
    Ok(WorldCamera::orthographic(
        center + Vec3::Z * 50.,
        center,
        Vec3::Y,
        [-0.5, 0.5],
        [-0.5, 0.5],
        0.1,
        100.,
    )
    .frame(1.)?)
}

/// Deliberately dark exterior light proves the native interior override is used.
fn lighting() -> LiquidLighting {
    LiquidLighting::new(Vec3::Z, Vec3::ZERO, Vec3::ZERO, Vec3::ZERO)
}

/// Disable fog so the capture isolates the material, tint and light factory.
fn fog() -> LiquidFog {
    LiquidFog::new(Vec3::new(0., 1., 1.), Vec3::ZERO)
}

/// Submit exactly the supplied native group queue with one coherent light sample.
fn prepare_draws(
    frame: &WorldModelFrame,
    renderer: &VulkanRenderer,
    camera: WorldCameraFrame,
    groups: &[WorldModelSceneGroup],
    fog: LiquidFog,
    ordinary: Vec3,
) -> Result<Vec<LiquidPreparedDraw>, Box<dyn Error>> {
    let mut draws = Vec::new();
    frame.prepare_liquid_draws(
        renderer,
        groups,
        camera,
        lighting(),
        fog,
        ordinary,
        0,
        false,
        None,
        &mut draws,
    )?;
    Ok(draws)
}

/// Every visible pixel must carry the expected liquid lighting or native fog.
fn capture(
    renderer: &mut VulkanRenderer,
    draws: &[LiquidPreparedDraw],
    expected: [u8; 4],
) -> Result<(), Box<dyn Error>> {
    let depths = [
        LiquidDepthTextureKind::River,
        LiquidDepthTextureKind::Ocean,
        LiquidDepthTextureKind::WorldModel,
    ]
    .map(|kind| LiquidDepthTexture::prepare(kind, [0; 2], [255; 2]));
    let scene = WorldFrameScene::new(
        TerrainSceneUniform::new(
            Mat4::IDENTITY,
            Mat4::IDENTITY,
            Vec3::ONE,
            Vec3::ZERO,
            Vec3::Z,
        ),
        WorldModelSceneUniform::new(
            Mat4::IDENTITY,
            Mat4::IDENTITY,
            Vec3::ZERO,
            Vec3::ONE,
            Vec3::ZERO,
            Vec3::Z,
            Vec4::ZERO,
        ),
        M2SceneUniform::new(
            Mat4::IDENTITY,
            Mat4::IDENTITY,
            Vec3::ZERO,
            Vec3::ONE,
            Vec3::ZERO,
            Vec3::Z,
            Vec4::ZERO,
            Vec3::ZERO,
            [M2LocalLightState::disabled(); 4],
        ),
    )
    .with_liquids(LiquidFrame::new(
        draws, &depths[0], &depths[1], &depths[2], 0,
    ));
    renderer.request_frame_capture()?;
    let report =
        renderer.present_world_frame(scene, &[], &[], &[], &[], &[], &[], &[], &[], &[])?;
    assert_eq!(report.liquid_draw_count(), draws.len());
    let image = renderer
        .take_captured_frame()?
        .ok_or("missing frame capture")?;
    for pixel in image.rgba8().as_chunks::<4>().0 {
        assert!(
            pixel
                .iter()
                .zip(expected)
                .all(|(&actual, expected)| actual.abs_diff(expected) <= 1),
            "liquid pixel {pixel:?}, expected {expected:?}"
        );
    }
    Ok(())
}
