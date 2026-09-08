//! Archive-backed admission of the five named unit water/breath particle models.

use std::error::Error;

use glam::{Mat4, Vec3};
use solarity_asset::{
    AnimationDataCatalog, ArchiveCatalog, AssetStore, BlpTextureCache, ClientDataRoot, Locale,
    M2ModelCache, SpellVisualEffectCatalog,
};
use solarity_rendering::{
    M2BonePose, M2ParticleMeshPlan, M2ParticlePose, M2ParticleTwinkleTable, WorldCamera,
};
use solarity_systems::UnitWaterEffect;

use super::unit_effects::UnitEffectPhase;
use super::{M2Playback, ResidentM2Source, stock_particle_simulations};
use crate::random::CrtRand;

#[test]
#[ignore = "requires SOLARITY_STOCK_DATA_ROOT with locally owned build-12340 archives"]
fn unit_effect_stock_models_prepare_simulate_and_retire() -> Result<(), Box<dyn Error>> {
    let root = std::env::var_os("SOLARITY_STOCK_DATA_ROOT").ok_or("stock data root")?;
    let mut assets = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(root)?,
        Locale::EnUs,
    )?)?;
    let effects = SpellVisualEffectCatalog::load(&mut assets)?;
    let animations = AnimationDataCatalog::load(&mut assets)?;
    let mut models = M2ModelCache::new();
    let mut textures = BlpTextureCache::new();
    let twinkle = M2ParticleTwinkleTable::new(0x24823);
    let camera =
        WorldCamera::stock(Vec3::new(0.0, -5.0, 2.0), Vec3::ZERO, Vec3::Z, 100.0).frame(1.0)?;
    for effect in [
        UnitWaterEffect::RunSpray,
        UnitWaterEffect::WalkSpray,
        UnitWaterEffect::UnderwaterBreath,
        UnitWaterEffect::ColdBreath,
        UnitWaterEffect::InebriatedBubbles,
    ] {
        let definition = effects.named(effect.name()).ok_or("named effect")?;
        let source = ResidentM2Source::load(
            &definition.model_path()?.ok_or("effect model path")?,
            &mut models,
            &mut textures,
            &mut assets,
        )?;
        let model = source.model();
        let mut random = CrtRand::new();
        let mut playback = M2Playback::unit_effect_default_sequence(
            model,
            &animations,
            0,
            0,
            solarity_rendering::M2SequenceStartPhase::DuringSceneUpdate,
            &mut random,
        )?;
        let mut phase = UnitEffectPhase::Playing;
        let mut particles = stock_particle_simulations(model);
        assert!(!particles.is_empty(), "{}", effect.name());
        assert!(
            particles
                .iter()
                .all(|particle| particle.unsupported.is_none()),
            "{}",
            effect.name()
        );
        let duration = playback.sequence_duration_ms as u32;
        assert!(duration > 0, "{}", effect.name());
        let mut emitted = 0;
        let mut vertices = 0;
        let mut time = 0;
        while phase != UnitEffectPhase::Retiring {
            time += 33;
            assert!(time < 60_000, "{} did not complete", effect.name());
            let clock = phase
                .advance(&mut playback, model, time as f32, &mut random)?
                .clock;
            let bones =
                M2BonePose::compose_with_model_view(model.animations(), clock, camera.view())?;
            for (emitter, particle) in model.animations().particles().iter().zip(&mut particles) {
                let pose = M2ParticlePose::sample(model.animations(), emitter, clock)?;
                let transform = bones.particle_emitter_transform(emitter, Mat4::IDENTITY)?;
                let report = match emitter.emitter_type() {
                    1 => particle
                        .simulation
                        .advance_planar_bounded(emitter, pose, 0.033, transform, 1.0)?,
                    2 => particle
                        .simulation
                        .advance_sphere_bounded(emitter, pose, 0.033, transform, 1.0)?,
                    _ => return Err("unsupported unit effect emitter".into()),
                };
                emitted += report.emitted();
                let particle_transform = if emitter.particles_in_model_space() {
                    transform
                } else {
                    Mat4::IDENTITY
                };
                let mesh = M2ParticleMeshPlan::prepare_transformed_with_twinkle_table(
                    emitter,
                    pose,
                    particle.simulation.particles(),
                    camera,
                    particle_transform,
                    1.0,
                    1.0,
                    &twinkle,
                )?;
                vertices += mesh.vertices().len();
                assert!(
                    mesh.vertices()
                        .iter()
                        .all(|vertex| Vec3::from_array(vertex.position()).is_finite())
                );
            }
        }
        assert!(
            emitted > 0 && vertices > 0,
            "{}: births={emitted}, vertices={vertices}",
            effect.name()
        );
        // Retirement pins the animation pose and clears model emission. Continue
        // the actual emitter lifecycle until every retained particle expires.
        let terminal = phase
            .advance(&mut playback, model, time as f32, &mut random)?
            .clock;
        let bones =
            M2BonePose::compose_with_model_view(model.animations(), terminal, camera.view())?;
        for (emitter, particle) in model.animations().particles().iter().zip(&mut particles) {
            particle.simulation.set_emission_enabled(false);
            let pose = M2ParticlePose::sample(model.animations(), emitter, terminal)?;
            let transform = bones.particle_emitter_transform(emitter, Mat4::IDENTITY)?;
            let maximum_lifetime = pose.lifespan() + emitter.lifespan_variation().abs();
            for _ in 0..((maximum_lifetime * 10.0).ceil() as usize + 2) {
                let report = match emitter.emitter_type() {
                    1 => particle
                        .simulation
                        .advance_planar_bounded(emitter, pose, 0.1, transform, 1.0)?,
                    2 => particle
                        .simulation
                        .advance_sphere_bounded(emitter, pose, 0.1, transform, 1.0)?,
                    _ => return Err("unsupported unit effect emitter".into()),
                };
                assert_eq!(report.emitted(), 0);
            }
            assert!(
                particle.simulation.particles().is_empty(),
                "{}",
                effect.name()
            );
        }
        println!(
            "{}: duration={duration}, births={emitted}, vertices={vertices}, mesh_draws={}, bounds={:?}, events={:?}",
            effect.name(),
            source.cpu_source().plan.draws().len(),
            model.bounds(),
            model
                .animations()
                .events()
                .iter()
                .map(|event| event.identifier())
                .collect::<Vec<_>>(),
        );
    }
    Ok(())
}

#[test]
#[ignore = "requires SOLARITY_STOCK_DATA_ROOT with locally owned build-12340 archives"]
fn unit_effect_stock_models_enter_gpu_scene_and_drain() -> Result<(), Box<dyn Error>> {
    use super::unit_effects::{
        M2UnitEffectWarmup, ResidentUnitEffect, UnitEffectBinding, UnitEffectRequest, WATER_EFFECTS,
    };
    use crate::configuration::{WindowConfiguration, WindowMode};
    use crate::platform::SdlPlatform;
    use solarity_rendering::{
        M2CameraEffectScale, M2TransparentPass, WorldFrustum, WorldScreenWindow,
    };
    use std::{rc::Rc, sync::Arc};
    let _sdl_guard = crate::test_support::SDL_TEST_LOCK
        .lock()
        .map_err(|_| "SDL test lock poisoned")?;
    let root = std::env::var_os("SOLARITY_STOCK_DATA_ROOT").ok_or("stock data root")?;
    let mut assets = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(root)?,
        Locale::EnUs,
    )?)?;
    let animations = Arc::new(AnimationDataCatalog::load(&mut assets)?);
    let sources = ResidentUnitEffect::load(&mut assets)?;
    assert_eq!(sources.len(), 5);
    let platform = SdlPlatform::start(WindowConfiguration::new(128, 128, WindowMode::Windowed))?;
    let mut renderer = super::game_object_scene_tests::renderer(&platform)?;
    let mut warmup = M2UnitEffectWarmup::new(sources);
    while !warmup.service_one(&mut renderer)? {}
    let mut random = CrtRand::new();
    let mut frame = super::M2Frame::prepare(
        &mut renderer,
        &super::ResidentM2Scene::default(),
        animations,
        &mut random,
        Arc::new(M2ParticleTwinkleTable::new(1)),
    )?;
    frame.set_unit_effect_sources(Arc::new(warmup.into_sources()));
    let world = solarity_ecs::ActiveWorld::enter(solarity_ecs::WorldBootstrap::new(
        solarity_ecs::WorldMapId::new(0),
        1,
        "Effect owner",
        Vec3::ZERO,
        0.,
    ));
    let identity = world.object_identity(1).ok_or("unit identity")?;
    let lifetime = Rc::new(());
    for kind in WATER_EFFECTS {
        frame.unit_effects.emit(
            UnitEffectRequest {
                identity,
                lifetime: Rc::downgrade(&lifetime),
                kind,
                binding: UnitEffectBinding::Positioned {
                    position: Vec3::ZERO,
                    world_factor: 1.,
                    unit_scale: 1.,
                },
            },
            &frame.animations,
            0.,
            &mut random,
        )?;
    }
    let camera = WorldCamera::stock(Vec3::new(0., -5., 2.), Vec3::ZERO, Vec3::Z, 100.).frame(1.)?;
    let mut particle_draws = 0;
    let mut mesh_draws = 0;
    for now in (0..15_000).step_by(33) {
        let draws = frame.prepare_visible_draws(
            &renderer,
            WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
            camera,
            M2TransparentPass::One,
            Vec3::ZERO,
            now as f32,
            M2CameraEffectScale::EXTERNAL_CAMERA,
            &mut random,
            None,
        )?;
        particle_draws += draws.particle_draws.len();
        mesh_draws += draws.draws.len();
        if now == 0 {
            assert_eq!(
                frame.placements.len(),
                5,
                "particle-only models need scene admission"
            );
        }
    }
    assert!(particle_draws > 0);
    assert!(
        mesh_draws > 0,
        "inebriated bubbles also contain mesh batches"
    );
    assert!(frame.placements.is_empty());
    assert!(frame.sources.is_empty());
    println!(
        "five archived GPU effect sources: particle_draws={particle_draws}, mesh_draws={mesh_draws}; all drained"
    );
    Ok(())
}
