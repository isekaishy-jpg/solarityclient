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
        &mut crate::frame_cpu_support::gpu_preparation(&mut renderer),
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
            &mut crate::frame_cpu_support::gpu_preparation(&mut renderer),
            &ResidentM2Scene::default(),
            Arc::clone(&animations),
            &mut random,
            Arc::new(M2ParticleTwinkleTable::new(1)),
        )?;
        frame.sources.push(Some(source.clone()));
        for index in 0..count {
            // These models are outside the camera frustum but inside the native
            // unit shadow volume. Their emitter clocks must remain untouched.
            let y = if !measure && index % 4 == 3 {
                18.
            } else {
                (index % 6) as f32
            };
            let transform = Mat4::from_translation(Vec3::new(0., y, ((index / 6) % 4) as f32));
            let playback = M2Playback::default_sequence(&model, &animations, 0, &mut random)?;
            let mut placement = m2_gpu_placement(
                0,
                transform,
                if !measure && index >= count * 3 / 4 {
                    // Immutable scenery shares the moving/abandoned frame test;
                    // earlier dynamic removal relocates its cached admission.
                    M2GpuPlacementOwner::Static(
                        crate::application::terrain_coordinator::m2_residency::ResidentM2Owner::TerrainDoodad {
                            unique_id: index as u32 + 1,
                        },
                    )
                } else {
                    M2GpuPlacementOwner::CreatureBody { guid: index + 1 }
                },
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
            {
                let total: std::num::NonZeroUsize = NonZeroUsize::new(4).ok_or("workers")?;
                solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                    .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
            },
            NonZeroUsize::new(8).ok_or("capacity")?,
            solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
        ),
        platform.coordinator_notifier(),
    )?;
    let mut charged_output = false;
    let mut saw_particles = false;
    let mut saw_ribbons = false;
    let mut saw_shadow_only_job = false;
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
            &mut crate::application::frame_pipeline::FrameWait::Offline,
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
        assert_eq!(
            a.environment_shadow_draws, b.environment_shadow_draws,
            "environment shadows step {step}"
        );
        let collect_bones = |source: &dyn solarity_rendering::M2BonePaletteSource| {
            (0..source.palette_count())
                .flat_map(|index| source.palette(index))
                .copied()
                .collect::<Vec<_>>()
        };
        assert_eq!(a.bone_transforms.len(), b.bone_transforms.len());
        assert_eq!(
            collect_bones(a.bone_transforms),
            collect_bones(b.bone_transforms)
        );
        assert_eq!(a.particle_vertices, b.particle_vertices);
        assert_eq!(a.particle_indices, b.particle_indices);
        assert_eq!(a.particle_draws, b.particle_draws);
        assert_eq!(a.ribbon_vertices, b.ribbon_vertices);
        assert_eq!(a.ribbon_draws, b.ribbon_draws);
        assert_eq!(a.water_scene_order, b.water_scene_order);
        assert_eq!(a.particle_vertex_capacity, b.particle_vertex_capacity);
        assert_eq!(a.particle_index_capacity, b.particle_index_capacity);
        if let Some(draw) = b.draws.first().copied() {
            assert_eq!(draw.relocate_bones(0)?, draw);
            let shifted = draw.relocate_bones(32)?;
            assert_eq!(shifted.material(), draw.material());
            assert_eq!(shifted.scene_order(), draw.scene_order());
            assert_eq!(shifted.shadow_material(), draw.shadow_material());
            let original = draw.push_constants().to_bytes();
            let relocated = shifted.push_constants().to_bytes();
            assert_eq!(&original[4..], &relocated[4..]);
            assert_eq!(
                u32::from_le_bytes(relocated[..4].try_into()?),
                u32::from_le_bytes(original[..4].try_into()?) + 32
            );
            assert!(draw.relocate_bones(1)?.relocate_bones(u32::MAX).is_err());
        }
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
        saw_shadow_only_job |= candidate.geometry_batch.jobs.iter().any(|owner| {
            let job = owner.job();
            job.input.is_some_and(|input| {
                input.visible.is_none() && (input.primary_shadow || input.environment_maps != 0)
            }) && job.palette.pending
                && !job.owns_effects
        });
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
                .all(|owner| !owner.job().owns_effects)
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
        assert!(
            saw_shadow_only_job,
            "offscreen palettes and shadow packets ran in owned jobs"
        );
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
        assert_worker_admission_refusal(&cpu, candidate, camera)?;
        assert_finalization_start_refusal(&cpu, candidate)?;
        assert_late_pose_connected_admission(candidate.sources[0].as_ref().ok_or("source")?)?;
        super::super::poses::ScenePoses::assert_connected_admission(
            candidate.sources[0].as_ref().ok_or("source")?,
        )?;
        let mut abandoned = candidate.begin_visible_draws_with_unit_effects(
            &renderer,
            &cpu,
            &mut crate::application::frame_pipeline::FrameWait::Offline,
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
        // Exhaust at most one pose/spatial yield per authored root, then the
        // terminal geometry/finalization boundaries. Do not advance receivers:
        // Drop must recover the completed worker-owned streams itself.
        for _ in 0..count + 4 {
            abandoned.try_admit(&cpu, &mut randoms[1], None, None, None)?;
            abandoned.wait(&mut FrameWait::Offline)?;
        }
        drop(abandoned);
        assert!(
            !candidate.visible_draws.is_empty(),
            "abandonment restores the worker-finalized output owner"
        );
        assert!(!candidate.geometry_batch.submitted);
        assert!(
            candidate
                .geometry_batch
                .jobs
                .iter()
                .all(|owner| !owner.job().owns_effects)
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
                .all(|owner| !owner.job().owns_effects)
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

/// Each model fits separately; refusal of the complete group must leave both
/// living simulations and every output allocation untouched at the real worker entry.
fn assert_worker_admission_refusal(
    cpu: &CpuExecutor,
    frame: &mut M2Frame,
    camera: solarity_rendering::WorldCameraFrame,
) -> Result<(), Box<dyn Error>> {
    use super::{chunk::GeometryChunk, job::GeometryContext, owner::GeometryOwner};
    use solarity_cpu::{
        CpuStorageBudget, CpuStorageClass, CpuStoragePlan, CpuStorageWorkingSet, CpuWorkerScratch,
        FrameBatch,
    };
    let inputs: Vec<_> = frame
        .geometry_batch
        .jobs
        .iter()
        .filter_map(|owner| owner.job().input)
        .filter(|input| input.visible.is_some())
        .take(2)
        .collect();
    assert_eq!(inputs.len(), 2);
    let probe = CpuStorageBudget::new(CpuStoragePlan::new(usize::MAX, 0, 0));
    let mut chunk = GeometryChunk::default();
    chunk.reserve(cpu.storage())?;
    chunk.scratch = Some(CpuWorkerScratch::new(cpu, CpuStorageClass::Frame)?);
    let mut particles = Vec::new();
    let mut ribbons = Vec::new();
    let mut largest = 0;
    let mut combined = CpuStorageWorkingSet::default();
    for input in inputs {
        let source = frame.sources[input.source_index].as_ref().ok_or("source")?;
        let placement = &mut frame.placements[input.placement_index];
        particles.push(
            placement
                .particles
                .iter()
                .map(|particle| particle.simulation.particles().to_vec())
                .collect::<Vec<_>>(),
        );
        ribbons.push(
            placement
                .ribbons
                .iter()
                .map(|ribbon| ribbon.sections().cloned().collect::<Vec<_>>())
                .collect::<Vec<_>>(),
        );
        let mut owner = GeometryOwner::new(cpu.storage())?;
        let job = owner.job_mut();
        job.input = Some(input);
        job.context = Some(GeometryContext {
            storage: probe.clone(),
            source: Arc::clone(source),
            camera,
            effect_scale: M2CameraEffectScale::EXTERNAL_CAMERA,
            twinkle: Arc::clone(&frame.particle_twinkle),
        });
        std::mem::swap(&mut job.particles, &mut placement.particles);
        std::mem::swap(&mut job.ribbons, &mut placement.ribbons);
        let mut individual = CpuStorageWorkingSet::default();
        job.include_working_set(&probe, &input, source, &mut individual)?;
        largest = largest.max(individual.bytes());
        job.include_working_set(&probe, &input, source, &mut combined)?;
        chunk.jobs.push(owner)?;
    }
    assert!(combined.bytes() > largest);
    let refused = CpuStorageBudget::new(CpuStoragePlan::new(largest, 0, 0));
    for owner in chunk.jobs.iter_mut() {
        owner.job_mut().context.as_mut().ok_or("context")?.storage = refused.clone();
    }
    let mut chunks = vec![chunk];
    let mut batch = FrameBatch::with_context(GeometryChunk::execute);
    batch.start(cpu, &mut chunks)?;
    batch.reclaim(&mut chunks)?;
    for (index, owner) in chunks[0].jobs.iter_mut().enumerate() {
        let job = owner.job_mut();
        assert!(matches!(
            job.result.as_ref(),
            Some(Err(RuntimeTerrainFrameError::Cpu(_)))
        ));
        assert_eq!(job.pose.allocated_bytes(), 0);
        assert_eq!(job.visible_draws.capacity(), 0);
        assert_eq!(job.material_poses.capacity(), 0);
        assert_eq!(job.particle_vertices.capacity(), 0);
        assert_eq!(job.ribbon_vertices.capacity(), 0);
        for (before, after) in particles[index].iter().zip(&job.particles) {
            assert_eq!(before.as_slice(), after.simulation.particles());
        }
        for (before, after) in ribbons[index].iter().zip(&job.ribbons) {
            assert!(before.iter().eq(after.sections()));
        }
        assert!(
            job.context.is_none(),
            "refused group releases immutable resource pins"
        );
        let placement = &mut frame.placements[job.input.ok_or("input")?.placement_index];
        std::mem::swap(&mut job.particles, &mut placement.particles);
        std::mem::swap(&mut job.ribbons, &mut placement.ribbons);
    }
    assert_eq!(refused.snapshot().used(CpuStorageClass::Frame), 0);
    Ok(())
}

/// A failed scheduler start happens after capture; the ordinary return path must
/// restore all frame vectors/model owners without assembling or losing their storage.
fn assert_finalization_start_refusal(
    cpu: &CpuExecutor,
    frame: &mut M2Frame,
) -> Result<(), Box<dyn Error>> {
    use solarity_cpu::{CpuError, FrameBatch, FrameBatchPlan};
    let mut holds = Vec::new();
    loop {
        let mut hold = FrameBatch::new(|_: &mut ()| {});
        match hold.begin(cpu, FrameBatchPlan::new(0, 0)) {
            Ok(()) => holds.push(hold),
            Err(CpuError::AtCapacity { .. }) => break,
            Err(error) => return Err(error.into()),
        }
    }
    let counts = (
        frame.visible_draws.len(),
        frame.particle_vertices.len(),
        frame.ribbon_vertices.len(),
        frame.geometry_batch.jobs.len(),
    );
    let addresses = (
        frame.visible_draws.as_ptr(),
        frame.particle_vertices.as_ptr(),
        frame.ribbon_vertices.as_ptr(),
        frame.geometry_batch.jobs.as_ptr(),
    );
    let mut work = super::super::diagnostics::Work::new();
    assert!(matches!(
        frame.begin_finalization(cpu, &mut work, M2TransparentPass::One),
        Err(RuntimeTerrainFrameError::Cpu(CpuError::AtCapacity { .. }))
    ));
    assert!(
        frame.geometry_batch.jobs.is_empty(),
        "refused captured jobs remain in their owned cell"
    );
    assert!(
        frame
            .finish_finalization(&mut FrameWait::Offline, Some(&mut work))?
            .is_none()
    );
    assert_eq!(
        counts,
        (
            frame.visible_draws.len(),
            frame.particle_vertices.len(),
            frame.ribbon_vertices.len(),
            frame.geometry_batch.jobs.len()
        )
    );
    assert_eq!(
        addresses,
        (
            frame.visible_draws.as_ptr(),
            frame.particle_vertices.as_ptr(),
            frame.ribbon_vertices.as_ptr(),
            frame.geometry_batch.jobs.as_ptr()
        )
    );
    drop(holds);
    Ok(())
}

/// Exercise actual late-pose preparation, including copied overrides and named demand,
/// under a budget that permits all but one byte of its complete cold phase.
fn assert_late_pose_connected_admission(source: &M2GpuSource) -> Result<(), Box<dyn Error>> {
    use super::super::poses::LatePose;
    use solarity_cpu::{CpuStorageClass as Class, CpuStorageKind as Kind, CpuStoragePlan};
    use solarity_rendering::{M2BoneSamples, M2BoneTransforms};
    let clock = M2AnimationClock::new(0, 120., 120.);
    let transforms = [(0, Mat4::from_translation(Vec3::Y))];
    let sequences = [(0, clock)];
    let overrides = M2BonePoseOverrides {
        model_oriented_billboard_bones: &source.model_oriented_billboard_bones,
        bone_transforms: &transforms,
        bone_sequences: &sequences,
        ..Default::default()
    };
    for palette in [true, false] {
        let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
            solarity_cpu::CpuExecutionPlan::new(0, 1, 1, 1)?,
            NonZeroUsize::MIN,
            CpuStoragePlan::new(1 << 20, 1 << 20, 0),
        ))?;
        let budget = cpu.storage().clone();
        let baseline = budget.snapshot().used(Class::Frame);
        let mut probe = LatePose::default();
        probe.start(
            &cpu,
            0,
            source,
            clock,
            Mat4::IDENTITY,
            overrides,
            &[0],
            palette,
        )?;
        probe.finish(&mut FrameWait::Offline)?;
        let required = budget.snapshot().peak(Class::Frame) - baseline;
        drop(probe);
        assert_eq!(budget.snapshot().used(Class::Frame), baseline);
        let pressure = budget.reserve(
            Class::Frame,
            Kind::Scratch,
            budget.snapshot().limit(Class::Frame) - baseline - required + 1,
        )?;
        let held = budget.snapshot().used(Class::Frame);
        let mut late = LatePose::default();
        assert!(matches!(
            late.start(
                &cpu,
                0,
                source,
                clock,
                Mat4::IDENTITY,
                overrides,
                &[0],
                palette
            ),
            Err(RuntimeTerrainFrameError::Cpu(
                solarity_cpu::CpuError::StorageAtCapacity { .. }
            ))
        ));
        assert!(late.is_ready());
        assert_eq!(
            budget.snapshot().used(Class::Frame),
            held,
            "refusal cannot grow even the copied inputs or scheduler"
        );
        drop(pressure);
        late.start(
            &cpu,
            0,
            source,
            clock,
            Mat4::IDENTITY,
            overrides,
            &[0],
            palette,
        )?;
        late.finish(&mut FrameWait::Offline)?;
        let pressure = budget.reserve(
            Class::Frame,
            Kind::Scratch,
            budget.snapshot().limit(Class::Frame) - budget.snapshot().used(Class::Frame),
        )?;
        late.start(
            &cpu,
            0,
            source,
            clock,
            Mat4::IDENTITY,
            overrides,
            &[0],
            palette,
        )?;
        late.finish(&mut FrameWait::Offline)?;
        drop(pressure);
        if palette {
            let mut serial = M2BonePose::default();
            serial.recompose_with_overrides(
                source.model.animations(),
                clock,
                Mat4::IDENTITY,
                overrides,
            )?;
            let mut output = M2BonePose::default();
            assert!(late.take(
                0,
                &source.model,
                clock,
                Mat4::IDENTITY,
                overrides,
                &mut output
            )?);
            assert_eq!(output.transforms(), serial.transforms());
        } else {
            let mut serial = M2BoneSamples::default();
            serial.recompose(
                source.model.animations(),
                clock,
                Mat4::IDENTITY,
                overrides,
                &[0],
            )?;
            let mut output = M2BoneSamples::default();
            assert!(late.take_samples(
                0,
                &source.model,
                clock,
                Mat4::IDENTITY,
                overrides,
                &[0],
                &mut output
            )?);
            assert_eq!(output.bone_transform(0), serial.bone_transform(0));
        }
        drop(late);
        cpu.shutdown()?;
        assert_eq!(budget.snapshot().used(Class::Frame), baseline);
    }
    Ok(())
}
