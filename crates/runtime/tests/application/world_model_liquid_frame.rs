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
    LiquidLighting, LiquidShaderUniform, M2LocalLightState, M2SceneUniform, TerrainSceneUniform,
    VulkanBootstrap, VulkanRenderer, WorldCamera, WorldCameraFrame, WorldFrameScene, WorldFrustum,
    WorldModelBaseMip, WorldModelSceneUniform, WorldModelTextureFiltering, WorldScreenWindow,
};

use super::WorldModelFrame;
use crate::application::game_object_coordinator::RuntimeGameObjectPresentation;
use crate::application::terrain_coordinator::world_model_residency::ResidentWorldModelScene;
use crate::test_support::{ClientFixture, SDL_TEST_LOCK, liquid_models};

/// Full source admission and real Vulkan pixels follow two replicated transforms.
#[test]
#[allow(unsafe_code)] // The hidden SDL test surface transfers to Vulkan ownership.
fn replicated_world_model_water_moves_shares_and_retires_its_mesh() -> Result<(), Box<dyn Error>> {
    let mut files = liquid_models::files(0, 0, 0, 1, 0xff335577);
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
    let mesh = frame.sources[0].as_ref().ok_or("GPU source")?.liquids[0].mesh();
    let camera = camera(Vec3::new(2., 2., 1.))?;
    capture(&frame, &mut renderer, camera)?;
    world.update_transform(
        90,
        WorldTransform::new(Vec3::X * 15., std::f32::consts::FRAC_PI_2),
    )?;
    objects.synchronize(Some(&world))?;
    frame.synchronize_game_objects(&mut renderer, objects.frame_input(Some(&world)))?;
    frame.update_game_object_states(objects.frame_input(Some(&world)))?;
    let mut hidden = Vec::new();
    frame.prepare_liquid_draws(
        &renderer,
        WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
        camera,
        lighting(),
        fog(),
        0,
        false,
        None,
        &mut hidden,
    )?;
    assert!(hidden.is_empty(), "moved factory leaves the old view");
    let moved = frame.placements.iter().find(|placement| matches!(placement.owner, super::WorldModelGpuPlacementOwner::GameObject {identity, ..} if identity.guid()==90)).ok_or("moved placement")?;
    let center = moved
        .plan
        .transform()
        .transform_point3(Vec3::new(2., 2., 1.));
    capture(&frame, &mut renderer, self::camera(center)?)?;
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
    world.remove_object(91)?;
    objects.synchronize(Some(&world))?;
    frame.synchronize_game_objects(&mut renderer, objects.frame_input(Some(&world)))?;
    assert!(frame.sources.is_empty());
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

/// Every pixel must carry the authored interior tint from the live WMO source.
fn capture(
    frame: &WorldModelFrame,
    renderer: &mut VulkanRenderer,
    camera: WorldCameraFrame,
) -> Result<(), Box<dyn Error>> {
    let mut draws = Vec::new();
    frame.prepare_liquid_draws(
        renderer,
        WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
        camera,
        lighting(),
        fog(),
        0,
        false,
        None,
        &mut draws,
    )?;
    assert_eq!(draws.len(), 1);
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
        &draws, &depths[0], &depths[1], &depths[2], 0,
    ));
    renderer.request_frame_capture()?;
    let report =
        renderer.present_world_frame(scene, &[], &[], &[], &[], &[], &[], &[], &[], &[])?;
    assert_eq!(report.liquid_draw_count(), 1);
    let image = renderer
        .take_captured_frame()?
        .ok_or("missing frame capture")?;
    for pixel in image.rgba8().as_chunks::<4>().0 {
        assert_eq!(*pixel, [51, 85, 119, 255]);
    }
    Ok(())
}
