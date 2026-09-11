//! Camera culling cannot remove an admitted root unit's animated shadow packet.

#![allow(unsafe_code)]

use super::*;
use crate::application::terrain_frame::shadow::{SceneryShadowQueries, WorldShadowAdmission};
use crate::test_support::{ClientFixture, SDL_TEST_LOCK, game_object_models};
use glam::Vec3;
use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_rendering::{
    VulkanBootstrap, WorldCamera, WorldEnvironmentShadowFrame, WorldEnvironmentShadowState,
    WorldScreenWindow, WorldShadowProjection, WorldShadowQuality,
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
            None,
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
    opacity.set_player_hidden(true);
    frame.placements[0].entity_opacity = Some(Rc::clone(&opacity));
    let hidden_camera = WorldCamera::orthographic(
        Vec3::X * 8.,
        Vec3::ZERO,
        Vec3::Z,
        [-2., 2.],
        [-2., 2.],
        0.1,
        100.,
    )
    .frame(1.)?;
    let hidden_shadow = WorldShadowProjection::primary(
        WorldShadowQuality::UnitsHigh,
        Vec3::ZERO,
        hidden_camera.camera().position(),
        -Vec3::Z,
    )?;
    let hidden = frame.prepare_visible_draws_with_unit_effects(
        &renderer,
        WorldFrustum::new(hidden_camera, WorldScreenWindow::FULL)?,
        hidden_camera,
        M2TransparentPass::One,
        Vec3::ZERO,
        25.,
        M2CameraEffectScale::EXTERNAL_CAMERA,
        &mut random,
        None,
        None,
        None,
        None,
        Some(hidden_shadow),
        None,
    )?;
    assert!(
        hidden.shadow_draws.is_empty(),
        "explicit hiding suppresses a fully opaque caster"
    );
    assert_eq!(frame.placements[0].last_effect_time_ms, 21);
    opacity.set_player_hidden(false);
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
            None,
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

#[test]
fn environment_shadows_keep_offscreen_scenery_and_share_visible_bones() -> Result<(), Box<dyn Error>>
{
    let model = game_object_models::model_with_animations(&[0])?;
    let mut animated = model.clone();
    let bone = u32::from_le_bytes(animated[0x30..0x34].try_into()?) as usize;
    animated[bone + 4..bone + 8].copy_from_slice(&0x200u32.to_le_bytes());
    let skin = game_object_models::skin()?;
    let fixture = ClientFixture::with_common_files(&[
        ("Static.m2", &model),
        ("Static00.skin", &skin),
        ("Animated.m2", &animated),
        ("Animated00.skin", &skin),
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
    let models = ["Static.m2", "Animated.m2"].map(|path| -> Result<_, Box<dyn Error>> {
        Ok(Arc::new(DecodedM2Model::load(
            &mut store,
            &AssetPath::new(path)?,
        )?))
    });
    let [static_model, animated_model] = models;
    let models = [static_model?, animated_model?];
    let _lock = SDL_TEST_LOCK.lock().map_err(|_| "SDL lock poisoned")?;
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity environment shadow admission", 64, 64)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: The hidden SDL window outlives its solely owned Vulkan surface.
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
    let center = Vec3::X * 1000.;
    for (source_index, model) in models.iter().enumerate() {
        frame.sources.push(Some(prepare_gpu_source(
            &mut renderer,
            model,
            &[M2ResolvedTexture::StockWhite],
            None,
            M2LocalLightCount::Four,
            M2ModelOrientation::Authored,
        )?));
        assert_eq!(
            frame.sources[source_index]
                .as_ref()
                .ok_or("missing fixture source")?
                .animated_shadow_caster,
            source_index == 1
        );
        for offset in [Vec3::ZERO, Vec3::Z * 10.] {
            let playback = M2Playback::default_sequence(model, &animations, 0, &mut random)?;
            frame.placements.push(m2_gpu_placement(
                source_index,
                Mat4::from_translation(center + offset),
                M2GpuPlacementOwner::Static(ResidentM2Owner::TerrainDoodad {
                    unique_id: frame.placements.len() as u32 + 1,
                }),
                model,
                Some(M2PlaybackStorage::Local(playback)),
                None,
                0,
            )?);
        }
    }
    let camera = WorldCamera::orthographic(
        center + Vec3::X * 8.,
        center,
        Vec3::Z,
        [-2., 2.],
        [-2., 2.],
        0.1,
        100.,
    )
    .frame(1.)?;
    let animated_mesh = frame.sources[1]
        .as_ref()
        .and_then(|source| source.mesh)
        .ok_or("missing animated mesh")?;
    let elevated = |draw: solarity_rendering::M2PreparedDraw| {
        draw.material().to_bytes()[56..60] == 10f32.to_le_bytes()
    };
    for quality in [
        WorldShadowQuality::EnvironmentLow,
        WorldShadowQuality::Cascaded,
    ] {
        let mut state = WorldEnvironmentShadowState::new(quality);
        let primary =
            WorldShadowProjection::primary(quality, center, camera.camera().position(), -Vec3::Z)?
                .with_camera_culling(camera);
        let doodads = Default::default();
        let mut observed = [false; 4];
        for step in 0..9 {
            let updates = state.advance(center)?;
            let environment = WorldEnvironmentShadowFrame::new(
                &state,
                updates,
                camera.camera().position(),
                -Vec3::Z,
            )?;
            let admission = WorldShadowAdmission::new(primary, environment, camera, -Vec3::Z)?;
            let visible = frame.prepare_visible_draws_with_unit_effects(
                &renderer,
                WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
                camera,
                M2TransparentPass::One,
                center,
                100. + step as f32,
                M2CameraEffectScale::EXTERNAL_CAMERA,
                &mut random,
                None,
                None,
                None,
                None,
                Some(primary),
                Some(SceneryShadowQueries {
                    admission: &admission,
                    doodads: &doodads,
                }),
            )?;
            assert_eq!(visible.draws.len(), 2);
            assert!(
                visible.shadow_draws.is_empty(),
                "scenery uses its own primary membership"
            );
            let offscreen = visible
                .environment_shadow_draws
                .iter()
                .filter(|caster| elevated(caster.draw))
                .count();
            assert_eq!(
                visible.bone_transforms.len(),
                2 + offscreen,
                "each visible or shadow-only model owns one shared bone palette"
            );
            for caster in visible.environment_shadow_draws {
                let animated = caster.draw.mesh() == animated_mesh;
                observed[usize::from(animated) * 2 + usize::from(elevated(caster.draw))] = true;
                if quality == WorldShadowQuality::EnvironmentLow {
                    assert_eq!(caster.maps & 8 != 0, animated);
                    assert_eq!(caster.maps & 7 != 0, !animated);
                }
            }
        }
        assert_eq!(
            observed, [true; 4],
            "both caster classes include the offscreen prop"
        );
        assert_eq!(frame.placements[1].last_effect_time_ms, 0);
        assert_eq!(frame.placements[3].last_effect_time_ms, 0);
    }
    renderer.shutdown()?;
    Ok(())
}
