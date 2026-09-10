//! Camera culling cannot remove an admitted root unit's animated shadow packet.

#![allow(unsafe_code)]

use super::*;
use crate::test_support::{ClientFixture, SDL_TEST_LOCK, game_object_models};
use glam::Vec3;
use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_rendering::{
    VulkanBootstrap, WorldCamera, WorldScreenWindow, WorldShadowProjection, WorldShadowQuality,
};
use std::error::Error;

#[test]
fn offscreen_units_keep_shadow_bones_without_advancing_visible_effects()
-> Result<(), Box<dyn Error>> {
    let model = game_object_models::model_with_animations(&[0])?;
    let skin = game_object_models::skin()?;
    let fixture =
        ClientFixture::with_common_files(&[("Caster.m2", &model), ("Caster00.skin", &skin)])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
    let model = Arc::new(DecodedM2Model::load(
        &mut store,
        &AssetPath::new("Caster.m2")?,
    )?);
    let _lock = SDL_TEST_LOCK.lock().map_err(|_| "SDL lock poisoned")?;
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity offscreen unit shadows", 64, 64)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: SDL creates the surface for this instance and transfers sole ownership.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (64, 64), 0) }?;
    let mut random = CrtRand::new();
    let mut frame = M2Frame::prepare(
        &mut renderer,
        &ResidentM2Scene::default(),
        Arc::clone(&animations),
        &mut random,
        Arc::new(M2ParticleTwinkleTable::new(1)),
    )?;
    frame.sources.push(Some(prepare_gpu_source(
        &mut renderer,
        &model,
        &[M2ResolvedTexture::StockWhite],
        None,
        M2LocalLightCount::Four,
        M2ModelOrientation::Authored,
    )?));
    for (owner, position) in [
        (M2GpuPlacementOwner::CreatureBody { guid: 1 }, Vec3::ZERO),
        (M2GpuPlacementOwner::CreatureBody { guid: 2 }, Vec3::Y * 50.),
        (
            M2GpuPlacementOwner::Static(ResidentM2Owner::TerrainDoodad { unique_id: 3 }),
            Vec3::ZERO,
        ),
    ] {
        let playback = M2Playback::default_sequence(&model, &animations, 0, &mut random)?;
        frame.placements.push(m2_gpu_placement(
            0,
            Mat4::from_translation(position),
            owner,
            &model,
            Some(M2PlaybackStorage::Local(playback)),
            None,
            0,
        )?);
    }
    let mut unit_palette = None;
    for (time, target, expected_draws) in [(1., Vec3::X * 9., 0), (21., Vec3::ZERO, 2)] {
        let camera = WorldCamera::orthographic(
            Vec3::X * 8.,
            target,
            Vec3::Z,
            [-2., 2.],
            [-2., 2.],
            0.1,
            100.,
        )
        .frame(1.)?;
        let shadow = WorldShadowProjection::primary(
            WorldShadowQuality::UnitsHigh,
            Vec3::ZERO,
            camera.camera().position(),
            -Vec3::Z,
        )?;
        let visible = frame.prepare_visible_draws_with_unit_effects(
            &renderer,
            WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
            camera,
            M2TransparentPass::One,
            Vec3::ZERO,
            time,
            M2CameraEffectScale::EXTERNAL_CAMERA,
            &mut random,
            None,
            None,
            None,
            None,
            Some(shadow),
        )?;
        assert_eq!(visible.draws.len(), expected_draws);
        assert_eq!(
            visible.shadow_draws.len(),
            1,
            "the nearby creature casts, distant units and scenery do not"
        );
        assert_eq!(
            visible.bone_transforms.len(),
            if expected_draws == 0 { 1 } else { 2 },
            "visible and shadow packets share one unit palette"
        );
        let push = visible.shadow_draws[0].push_constants();
        if let Some(previous) = unit_palette {
            assert_eq!(push, previous);
        }
        unit_palette = Some(push);
        assert_eq!(
            frame.placements[0].last_effect_time_ms,
            if expected_draws == 0 { 0 } else { 21 }
        );
    }
    let opacity = Rc::new(crate::application::entity_opacity::EntityOpacityOwner::default());
    opacity.select_model(1, 1., 0, 21);
    opacity.mark_removed(21);
    frame.placements[0].entity_opacity = Some(opacity);
    frame.retire_removed_models();
    let camera = WorldCamera::orthographic(
        Vec3::X * 8.,
        Vec3::X * 9.,
        Vec3::Z,
        [-2., 2.],
        [-2., 2.],
        0.1,
        100.,
    )
    .frame(1.)?;
    let shadow = WorldShadowProjection::primary(
        WorldShadowQuality::UnitsHigh,
        Vec3::ZERO,
        camera.camera().position(),
        -Vec3::Z,
    )?;
    for (time, expected) in [(521., 1), (1021., 0)] {
        let visible = frame.prepare_visible_draws_with_unit_effects(
            &renderer,
            WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
            camera,
            M2TransparentPass::One,
            Vec3::ZERO,
            time,
            M2CameraEffectScale::EXTERNAL_CAMERA,
            &mut random,
            None,
            None,
            None,
            None,
            Some(shadow),
        )?;
        assert!(visible.draws.is_empty());
        assert_eq!(
            visible.shadow_draws.len(),
            expected,
            "retired units retain root admission and the native 0.55 material cutoff"
        );
        assert_eq!(
            frame.placements[0].last_effect_time_ms, 21,
            "offscreen retired effects retain their previous update timestamp"
        );
    }
    Ok(())
}
