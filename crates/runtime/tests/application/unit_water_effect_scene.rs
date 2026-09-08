//! Real GPU sources and the authored-event bridge share the current parent pose.

use super::super::super::unit_effects::{
    M2UnitEffectWarmup, ResidentUnitEffect, UnitEffectBinding, UnitEffectRequest,
};
use super::*;
use solarity_systems::UnitWaterEffect;

#[test]
fn unit_water_effect_scene_publishes_attaches_replaces_and_drains() -> Result<(), Box<dyn Error>> {
    let _sdl_guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let platform = SdlPlatform::start(WindowConfiguration::new(128, 128, WindowMode::Windowed))?;
    let mut renderer = renderer(&platform)?;
    for attachment in [17, 19] {
        let fixture = crate::test_support::unit_models::fixture_with_water_effects(attachment)?;
        let mut store = AssetStore::mount(ArchiveCatalog::discover(
            ClientDataRoot::new(fixture.data_root())?,
            Locale::EnUs,
        )?)?;
        let effects = ResidentUnitEffect::load(&mut store)?;
        assert_eq!(effects.len(), 5);
        let mut warmup = M2UnitEffectWarmup::new(effects);
        while !warmup.service_one(&mut renderer)? {}
        let bank = Arc::new(warmup.into_sources());
        let mut presentation = unit_presentation(&fixture)?;
        let mut world = ActiveWorld::enter(WorldBootstrap::new(
            WorldMapId::new(0),
            7,
            "Local",
            Vec3::ZERO,
            0.,
        ));
        add_unit(&mut world, 30, ObjectKind::Unit, 0)?;
        presentation.synchronize_creatures(Some(&world))?;
        let identity = world.object_identity(30).ok_or("unit identity")?;
        let lifetime = Rc::new(());
        let mut random = CrtRand::new();
        let mut frame = M2Frame::prepare(
            &mut renderer,
            &ResidentM2Scene::default(),
            fixture_animations(&fixture)?,
            &mut random,
            Arc::new(M2ParticleTwinkleTable::new(1)),
        )?;
        frame.set_unit_effect_sources(Arc::clone(&bank));
        frame.replace_creatures(
            &mut renderer,
            &presentation.resident_creature_frame_inputs(),
            &mut random,
        )?;
        let camera = WorldCamera::orthographic(
            Vec3::new(8., 0., 0.),
            Vec3::ZERO,
            Vec3::Z,
            [-4., 4.],
            [-2., 2.],
            0.1,
            100.,
        )
        .frame(1.)?;
        let mut callbacks = 0;
        for now in [1., 121., 191., 221.] {
            let mut callback = |event: &super::super::super::RuntimeM2Event,
                                owner: &Rc<UnitAnimationBehavior>,
                                _model: &DecodedM2Model,
                                _transform: Mat4| {
                assert_eq!(event.identifier(), *b"$BTH");
                assert_eq!(event.owner_guid(), Some(30));
                callbacks += 1;
                Some(UnitEffectRequest {
                    identity,
                    lifetime: Rc::downgrade(&lifetime),
                    kind: UnitWaterEffect::UnderwaterBreath,
                    binding: UnitEffectBinding::Attached {
                        owner: Rc::downgrade(owner),
                        model_scale: 1.,
                        attachment,
                    },
                })
            };
            let draws = frame.prepare_visible_draws_with_unit_effects(
                &renderer,
                WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
                camera,
                solarity_rendering::M2TransparentPass::One,
                Vec3::ZERO,
                now,
                M2CameraEffectScale::EXTERNAL_CAMERA,
                &mut random,
                None,
                Some(&mut callback),
            )?;
            assert_eq!(
                draws.draws.len(),
                if now == 1. {
                    1
                } else if now == 221. && attachment == 19 {
                    3
                } else {
                    2
                }
            );
            let effects: Vec<_> = frame
                .placements
                .iter()
                .filter(|p| p.unit_effect.is_some())
                .collect();
            assert_eq!(effects.len(), callbacks);
            for effect in &effects {
                assert!(
                    effect
                        .transform
                        .w_axis
                        .truncate()
                        .abs_diff_eq(Vec3::new(0.25, 0.5, 1.), 0.00001)
                );
            }
            if now == 121. {
                assert_eq!(
                    effects[0]
                        .playback
                        .as_ref()
                        .ok_or("effect playback")?
                        .borrow()
                        .script_timer
                        .ok_or("effect timer")?
                        .start_time_ms(),
                    121
                );
            }
            if now == 191. {
                assert!(
                    effects[0]
                        .particles
                        .iter()
                        .any(|p| !p.simulation.particles().is_empty())
                );
            }
            if now == 221. {
                assert_eq!(
                    effects[0]
                        .unit_effect
                        .as_ref()
                        .ok_or("old effect")?
                        .retiring(),
                    attachment == 17
                );
                assert!(
                    !effects[1]
                        .unit_effect
                        .as_ref()
                        .ok_or("new effect")?
                        .retiring()
                );
                assert_eq!(effects[0].source_index, effects[1].source_index);
            }
        }
        assert_eq!(callbacks, 2);
        drop(lifetime);
        let away = WorldCamera::stock(
            Vec3::new(100., 0., 0.),
            Vec3::new(200., 0., 0.),
            Vec3::Z,
            100.,
        )
        .frame(1.)?;
        for now in (222..6500).step_by(100) {
            let draws = frame.prepare_visible_draws(
                &renderer,
                WorldFrustum::new(away, WorldScreenWindow::FULL)?,
                away,
                solarity_rendering::M2TransparentPass::One,
                Vec3::ZERO,
                now as f32,
                M2CameraEffectScale::EXTERNAL_CAMERA,
                &mut random,
                None,
            )?;
            assert!(draws.draws.is_empty());
            assert!(draws.ribbon_draws.is_empty());
            assert!(
                frame
                    .placements
                    .iter()
                    .filter_map(|p| p.unit_effect.as_ref())
                    .all(|p| p.retiring())
            );
        }
        assert!(frame.placements.iter().all(|p| p.unit_effect.is_none()));

        // A callback can beat the worker/GPU bank. Publication retains its
        // creation clock, skips destroyed owners, and starts the primary in
        // the before-scene phase established by the native load fixtures.
        let mut delayed = M2Frame::prepare(
            &mut renderer,
            &ResidentM2Scene::default(),
            fixture_animations(&fixture)?,
            &mut random,
            Arc::new(M2ParticleTwinkleTable::new(1)),
        )?;
        let living = Rc::new(());
        let destroyed = Rc::new(());
        let original_random = random;
        for lifetime in [&living, &destroyed] {
            delayed.unit_effects.emit(
                UnitEffectRequest {
                    identity,
                    lifetime: Rc::downgrade(lifetime),
                    kind: UnitWaterEffect::RunSpray,
                    binding: UnitEffectBinding::Positioned {
                        position: Vec3::new(0.5, 0., 0.),
                        world_factor: 1.,
                        unit_scale: 1.,
                    },
                },
                &delayed.animations,
                100.,
                &mut random,
            )?;
        }
        assert_eq!(random, original_random);
        drop(destroyed);
        delayed.set_unit_effect_sources(bank);
        delayed.prepare_visible_draws(
            &renderer,
            WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
            camera,
            solarity_rendering::M2TransparentPass::One,
            Vec3::ZERO,
            1000.,
            M2CameraEffectScale::EXTERNAL_CAMERA,
            &mut random,
            None,
        )?;
        assert_eq!(delayed.placements.len(), 1);
        let playback = delayed.placements[0]
            .playback
            .as_ref()
            .ok_or("delayed playback")?
            .borrow();
        assert_eq!(
            playback
                .script_timer
                .ok_or("delayed timer")?
                .start_time_ms(),
            1001
        );
        assert_eq!(playback.global_tick(1000), 900);
        assert_eq!(
            delayed.placements[0].transform.w_axis.truncate(),
            Vec3::new(0.5, 0., 0.)
        );
    }
    Ok(())
}
