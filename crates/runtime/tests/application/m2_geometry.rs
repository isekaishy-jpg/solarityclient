//! Joined visible work matches the frozen serial traversal across moving frames.

#![allow(unsafe_code)]

use crate::frame_cpu_support::continuation_support;

use super::super::super::*;
use crate::application::frame_pipeline::FrameWait;
use crate::application::unit_animation::{UnitAnimationBehavior, UnitAnimationInput};
use crate::configuration::{WindowConfiguration, WindowMode};
use crate::platform::SdlPlatform;
use crate::test_support::{ClientFixture, SDL_TEST_LOCK, game_object_models, unit_models};
use glam::Vec3;
use solarity_asset::ResourceLease;
use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_cpu::{CpuExecutor, CpuPoolConfig};
use solarity_ecs::{ActiveWorld, UnitAnimationTier, WorldBootstrap, WorldMapId};
use solarity_rendering::{
    VulkanBootstrap, WorldCamera, WorldScreenWindow, WorldShadowProjection, WorldShadowQuality,
};
use std::{error::Error, num::NonZeroUsize};

/// Covers indexed emitter streams, ribbons, faded meshes, shadow palettes and culling.
#[test]
fn joined_geometry_matches_serial_during_motion_and_visibility_changes()
-> Result<(), Box<dyn Error>> {
    compare_geometry(24, 16, false)
}

/// Measures the complete old/new M2 path with many authored effect consumers.
#[test]
#[ignore = "manual optimized synthetic M2 preparation comparison"]
fn benchmark_joined_geometry_against_serial_traversal() -> Result<(), Box<dyn Error>> {
    compare_geometry(512, 256, true)
}

/// Uses identical resources and deterministic state; timings exclude equality checks.
fn compare_geometry(count: u64, steps: u32, measure: bool) -> Result<(), Box<dyn Error>> {
    // The controlled unit uses native stand-turn selection as its scene clock
    // advances. Author both turns rather than inventing a missing-sequence fallback.
    let mut bytes =
        game_object_models::model_with_animations(if measure { &[0] } else { &[0, 96, 97] })?;
    unit_models::append_effects(&mut bytes, 1);
    // This fixture's generic emitter leaves twinkle coverage at zero. Author
    // full coverage and a constant unit scale so independent pool addresses
    // produce identical visible cards while exercising all particle streams.
    let emitter = u32::from_le_bytes(bytes[0x12c..0x130].try_into()?) as usize;
    for offset in [0x164, 0x168, 0x16c] {
        bytes[emitter + offset..emitter + offset + 4].copy_from_slice(&1_f32.to_le_bytes());
    }
    let skin = game_object_models::skin()?;
    // Unit tier resolution reads AnimationData before selecting an authored M2
    // sequence. This fixture needs both halves of the native animation contract.
    let mut animation_data = b"WDBC".to_vec();
    for value in [3_u32, 8, 32, 1] {
        animation_data.extend_from_slice(&value.to_le_bytes());
    }
    for id in [0_u32, 96, 97] {
        for value in [id, 0, 0, 0, 0, 0, id, 0] {
            animation_data.extend_from_slice(&value.to_le_bytes());
        }
    }
    animation_data.push(0);
    let fixture = ClientFixture::with_common_files(&[
        ("Batch.m2", &bytes),
        ("Batch00.skin", &skin),
        ("DBFilesClient\\AnimationData.dbc", &animation_data),
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
    let model = ResourceLease::new(DecodedM2Model::load(
        &mut store,
        &AssetPath::new("Batch.m2")?,
    )?);
    let _lock = SDL_TEST_LOCK.lock().map_err(|_| "SDL lock poisoned")?;
    let mut platform = SdlPlatform::start(WindowConfiguration::new(64, 64, WindowMode::Windowed))?;
    let bootstrap = VulkanBootstrap::start(&platform.vulkan_instance_extensions()?)?;
    // SAFETY: SDL transfers sole surface ownership; its hidden window outlives the renderer.
    let surface = unsafe { platform.create_vulkan_surface(bootstrap.instance_handle()) }?;
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (64, 64), 0) }?;
    let source = prepare_gpu_source(
        &mut renderer,
        &model,
        &[M2ResolvedTexture::StockWhite],
        None,
        M2LocalLightCount::Four,
        M2ModelOrientation::Authored,
    )?;
    let mut frames = Vec::new();
    let mut randoms = Vec::new();
    for _ in 0..2 {
        let mut random = CrtRand::new();
        let mut frame = M2Frame::prepare(
            &mut renderer,
            &ResidentM2Scene::default(),
            Arc::clone(&animations),
            &mut random,
            Arc::new(M2ParticleTwinkleTable::new(1)),
        )?;
        frame.sources.push(Some(source.clone()));
        for index in 0..count {
            let transform =
                Mat4::from_translation(Vec3::new(0., (index % 6) as f32, ((index / 6) % 4) as f32));
            let playback = M2Playback::default_sequence(&model, &animations, 0, &mut random)?;
            let mut placement = m2_gpu_placement(
                0,
                transform,
                M2GpuPlacementOwner::CreatureBody { guid: index + 1 },
                &model,
                Some(M2PlaybackStorage::Local(playback)),
                None,
                0,
            )?;
            if index % 3 == 0 {
                placement.opacity = 0.4;
            }
            if !measure && index == count / 2 {
                // Pause partway through real ordered traversal: earlier models
                // already own worker effect state, later models remain untouched.
                let world = ActiveWorld::enter(WorldBootstrap::new(
                    WorldMapId::new(0),
                    index + 1,
                    "Pose",
                    Vec3::ZERO,
                    0.,
                ));
                let owner = Rc::new(UnitAnimationBehavior::new(
                    world.object_identity(index + 1).ok_or("root identity")?,
                    ResourceLease::clone(&model),
                    Arc::clone(&animations),
                    UnitAnimationInput::new(1, UnitAnimationTier::Ground, false, None),
                    0,
                ));
                owner.synchronize(0, &mut random)?;
                placement.playback = Some(M2PlaybackStorage::Shared(owner.playback()));
                placement.unit_animation = Some(owner);
            }
            frame.placements.push(placement);
        }
        frames.push(frame);
        randoms.push(random);
    }
    let mut cpu = CpuExecutor::with_notifier(
        CpuPoolConfig::new(
            NonZeroUsize::new(4).ok_or("workers")?,
            NonZeroUsize::new(8).ok_or("capacity")?,
            solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
        ),
        platform.coordinator_notifier(),
    )?;
    let mut charged_output = false;
    let mut saw_particles = false;
    let mut saw_ribbons = false;
    let mut reference_time = std::time::Duration::ZERO;
    let mut candidate_time = std::time::Duration::ZERO;
    let mut measured = 0;
    for step in 0..steps {
        let camera = WorldCamera::orthographic(
            Vec3::new(12., step as f32 * 0.3, 3.),
            Vec3::new(0., 2., 1.),
            Vec3::Z,
            [-5., 5.],
            [-5., 5.],
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
        for frame in &mut frames {
            frame.placements[0].transform = Mat4::from_translation(Vec3::Y * step as f32 * 0.25);
            frame.placements[1].placement_valid = step % 3 != 0;
            if step == 8 {
                frame.placements.retain(|placement| {
                    !matches!(
                        placement.owner,
                        M2GpuPlacementOwner::CreatureBody { guid: 4 | 8 | 12 }
                    )
                });
                frame.placement_topology_dirty = true;
            }
        }
        let [reference, candidate] = frames.as_mut_slice() else {
            return Err("two frames".into());
        };
        let [reference_random, candidate_random] = randoms.as_mut_slice() else {
            return Err("two streams".into());
        };
        // Alternate scene-light publication so packet-dependent receiver demand
        // and remapped scene indices are checked alongside the geometry streams.
        let lighting = (step % 2 != 0).then_some((
            solarity_rendering::M2SceneUniform::new(
                camera.projection(),
                camera.view(),
                camera.camera().position(),
                Vec3::ZERO,
                Vec3::ZERO,
                Vec3::Z,
                glam::Vec4::ZERO,
                Vec3::ZERO,
                [solarity_rendering::M2LocalLightState::disabled(); 4],
            ),
            solarity_rendering::M2DirectionalLight::new(
                -Vec3::Z,
                Vec3::splat(0.8),
                Vec3::splat(0.9),
            ),
        ));
        let started = std::time::Instant::now();
        let a = reference.prepare_visible_draws_reference(
            &renderer,
            None,
            WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
            camera,
            M2TransparentPass::One,
            Vec3::ZERO,
            (step * if measure { 16 } else { 167 } + 1) as f32,
            M2CameraEffectScale::EXTERNAL_CAMERA,
            reference_random,
            None,
            None,
            lighting,
            None,
            Some(shadow),
            None,
        )?;
        let reference_elapsed = started.elapsed();
        let started = std::time::Instant::now();
        // A blocked frame lane must not prevent independent main preparation.
        // The root midway through traversal also requires a worker palette.
        let held = (!measure && step == 0)
            .then(|| continuation_support::HeldFrameWorkers::new(&cpu))
            .transpose()?;
        let mut pending = candidate.begin_visible_draws_with_unit_effects(
            &renderer,
            &cpu,
            WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
            camera,
            M2TransparentPass::One,
            Vec3::ZERO,
            (step * if measure { 16 } else { 167 } + 1) as f32,
            M2CameraEffectScale::EXTERNAL_CAMERA,
            candidate_random,
            None,
            None,
            lighting,
            None,
            Some(shadow),
            None,
        )?;
        if let Some(held) = held {
            // Repeated nonblocking resumption leaves all ordered state untouched
            // and returns the renderer for a real main-only Vulkan operation.
            for _ in 0..3 {
                assert!(!pending.try_advance(&cpu, candidate_random, None, None, None)?);
                assert!(
                    pending.scene_lights().is_none(),
                    "sources cannot escape before receiver callbacks"
                );
            }
            renderer.present_clear([64.0, 64.0])?;
            held.release()?;
        }
        while !pending.try_advance(&cpu, candidate_random, None, None, None)? {
            pending.wait(&mut FrameWait::Native(&mut platform))?;
        }
        // A ready continuation stays ready; revisiting it cannot duplicate
        // receivers, reorder packets again, advance effects or consume RNG.
        assert!(pending.try_advance(&cpu, candidate_random, None, None, None)?);
        assert!(pending.try_advance(&cpu, candidate_random, None, None, None)?);
        let b = pending.into_visible_frame()?;
        if step >= 32 {
            reference_time += reference_elapsed;
            candidate_time += started.elapsed();
            measured += 1;
        }
        assert_eq!(a.draws, b.draws, "mesh step {step}");
        assert_eq!(
            a.instance_scenes, b.instance_scenes,
            "receiver scenes step {step}"
        );
        assert_eq!(a.shadow_draws, b.shadow_draws, "shadow step {step}");
        assert_eq!(a.bone_transforms, b.bone_transforms);
        assert_eq!(a.particle_vertices, b.particle_vertices);
        assert_eq!(a.particle_indices, b.particle_indices);
        assert_eq!(a.particle_draws, b.particle_draws);
        assert_eq!(a.ribbon_vertices, b.ribbon_vertices);
        assert_eq!(a.ribbon_draws, b.ribbon_draws);
        assert_eq!(a.water_scene_order, b.water_scene_order);
        assert_eq!(a.particle_vertex_capacity, b.particle_vertex_capacity);
        assert_eq!(a.particle_index_capacity, b.particle_index_capacity);
        if let Some(draw) = b.particle_draws.first().copied() {
            assert_eq!(draw.relocate(0, 0, 0, 0)?, draw);
            assert!(draw.relocate(u32::MAX, 0, 0, 0).is_err());
            assert!(draw.relocate(0, u32::MAX, 0, 0).is_err());
            let moved = draw.relocate(8, 12, 0, 3)?;
            assert_eq!(moved.vertex_offset(), draw.vertex_offset() + 8);
            assert_eq!(moved.first_index(), draw.first_index() + 12);
            assert_eq!(moved.effect_order(), draw.effect_order() + 3);
            assert_eq!(moved.scene_order(), draw.scene_order());
        }
        if let Some(draw) = b.ribbon_draws.first().copied() {
            assert_eq!(draw.relocate(0, 0, 0)?, draw);
            assert!(draw.relocate(u32::MAX, 0, 0).is_err());
            let moved = draw.relocate(8, 0, 3)?;
            assert_eq!(moved.first_vertex(), draw.first_vertex() + 8);
            assert_eq!(moved.effect_order(), draw.effect_order() + 3);
        }
        charged_output |= cpu.storage().snapshot().bytes(
            solarity_cpu::CpuStorageClass::Frame,
            solarity_cpu::CpuStorageKind::Result,
        ) > 0;
        saw_particles |= !b.particle_vertices.is_empty();
        saw_ribbons |= !b.ribbon_vertices.is_empty();
        assert_eq!(
            reference.scene_lighting.directionals(),
            candidate.scene_lighting.directionals(),
            "light sources step {step}"
        );
        assert!(
            candidate
                .geometry_batch
                .jobs
                .iter()
                .all(|job| !job.owns_effects)
        );
        assert_eq!(
            reference_random, candidate_random,
            "shared CRT stream order"
        );
        for (a, b) in reference.placements.iter().zip(candidate.placements.iter()) {
            assert_eq!(a.last_effect_time_ms, b.last_effect_time_ms);
            assert_eq!(a.particles.len(), b.particles.len());
            for (a, b) in a.particles.iter().zip(&b.particles) {
                assert_eq!(a.simulation.particles(), b.simulation.particles());
            }
            for (a, b) in a.ribbons.iter().zip(&b.ribbons) {
                assert!(a.sections().eq(b.sections()));
            }
        }
    }
    assert!(
        saw_particles && saw_ribbons,
        "both mutable effect consumers exercised: particles={saw_particles} ribbons={saw_ribbons}"
    );
    if !measure {
        let candidate = &mut frames[1];
        let camera = WorldCamera::orthographic(
            Vec3::new(12., 0., 3.),
            Vec3::new(0., 2., 1.),
            Vec3::Z,
            [-5., 5.],
            [-5., 5.],
            0.1,
            100.,
        )
        .frame(1.)?;
        let abandoned = candidate.begin_visible_draws_with_unit_effects(
            &renderer,
            &cpu,
            WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
            camera,
            M2TransparentPass::One,
            Vec3::ZERO,
            2_900.,
            M2CameraEffectScale::EXTERNAL_CAMERA,
            &mut randoms[1],
            None,
            None,
            None,
            None,
            None,
            None,
        )?;
        drop(abandoned);
        assert!(!candidate.geometry_batch.submitted);
        assert!(
            candidate
                .geometry_batch
                .jobs
                .iter()
                .all(|job| !job.owns_effects)
        );
        assert!(
            candidate
                .placements
                .iter()
                .all(|placement| placement.particles.len() == 1)
        );
        let source = candidate.sources[0].as_mut().ok_or("source")?;
        std::sync::Arc::make_mut(source).particles.clear();
        let result = candidate.prepare_visible_draws_with_unit_effects(
            &renderer,
            &cpu,
            &mut FrameWait::Native(&mut platform),
            WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
            camera,
            M2TransparentPass::One,
            Vec3::ZERO,
            3_000.,
            M2CameraEffectScale::EXTERNAL_CAMERA,
            &mut randoms[1],
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert!(matches!(
            result,
            Err(RuntimeTerrainFrameError::M2ParticleResourceCount { .. })
        ));
        assert!(
            candidate
                .geometry_batch
                .jobs
                .iter()
                .all(|job| !job.owns_effects)
        );
        assert!(
            candidate
                .placements
                .iter()
                .all(|placement| placement.particles.len() == 1)
        );
    }
    if measure {
        println!(
            "M2 geometry owners={count} frames={measured} old_ms={:.6} joined_ms={:.6}",
            reference_time.as_secs_f64() * 1000. / f64::from(measured),
            candidate_time.as_secs_f64() * 1000. / f64::from(measured)
        );
    }
    // Successful frames retained charged output; failure can retire it sooner.
    assert!(charged_output);
    drop(frames);
    assert_eq!(
        cpu.storage().snapshot().bytes(
            solarity_cpu::CpuStorageClass::Frame,
            solarity_cpu::CpuStorageKind::Result
        ),
        0
    );
    assert_eq!(
        cpu.storage().snapshot().bytes(
            solarity_cpu::CpuStorageClass::Frame,
            solarity_cpu::CpuStorageKind::Scratch
        ),
        0
    );
    cpu.shutdown()?;
    renderer.shutdown()?;
    Ok(())
}
