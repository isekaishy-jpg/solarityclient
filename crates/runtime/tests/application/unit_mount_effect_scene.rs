//! Mount event positions and body effect bindings have separate native owners.

use super::super::super::unit_effects::{
    M2UnitEffectWarmup, ResidentUnitEffect, UnitEffectBinding, UnitEffectRequest,
};
use super::*;
use solarity_systems::UnitWaterEffect;

#[test]
fn mount_authored_effects_run_before_rider_callbacks_and_bind_body_attachments()
-> Result<(), Box<dyn Error>> {
    let _sdl_guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let platform = SdlPlatform::start(WindowConfiguration::new(128, 128, WindowMode::Windowed))?;
    let mut renderer = renderer(&platform)?;
    for attachment in [17, 19] {
        let fixture =
            crate::test_support::unit_models::fixture_with_mount_water_effects(attachment)?;
        let mut store = AssetStore::mount(ArchiveCatalog::discover(
            ClientDataRoot::new(fixture.data_root())?,
            Locale::EnUs,
        )?)?;
        let mut warmup = M2UnitEffectWarmup::new(ResidentUnitEffect::load(
            &mut store,
            &solarity_asset::EnvironmentalDamageCatalog::default(),
        )?);
        while !warmup.service_one(&mut renderer)? {}
        let mut presentation = unit_presentation(&fixture)?;
        let mut world = ActiveWorld::enter(WorldBootstrap::new(
            WorldMapId::new(0),
            7,
            "Local",
            Vec3::ZERO,
            0.,
        ));
        add_unit(&mut world, 30, ObjectKind::Unit, 0)?;
        equipment_residency::fields(&mut world, 30, &[(69, 102)])?;
        world.update_transform(30, WorldTransform::new(Vec3::new(3., 4., 1.), 0.))?;
        presentation.synchronize_creatures(Some(&world), |_| None)?;
        let mut random = CrtRand::new();
        let mut frame = M2Frame::prepare(
            &mut renderer,
            &ResidentM2Scene::default(),
            fixture_animations(&fixture)?,
            &mut random,
            Arc::new(M2ParticleTwinkleTable::new(1)),
        )?;
        frame.set_unit_effect_sources(Arc::new(warmup.into_sources()));
        frame.replace_creatures(
            &mut renderer,
            &presentation.resident_creature_frame_inputs(),
            &mut random,
        )?;
        let mount = &frame.placements[0];
        assert_eq!(mount.owner, M2GpuPlacementOwner::CreatureMount { guid: 30 });
        let mount_transform = mount.transform;
        let mount_event_position = mount_transform.transform_point3(Vec3::new(4., 0., 0.));
        let rider = &frame.placements[1];
        let owner = Rc::clone(rider.unit_animation.as_ref().ok_or("rider animation")?);
        let body_transform = mount_transform
            * Mat4::from_translation(Vec3::new(0.25, 0.5, 1.))
            * Mat4::from_scale(Vec3::splat(rider.rider_scale));
        let body_position = body_transform.transform_point3(Vec3::ZERO);
        let effect_position = body_transform.transform_point3(Vec3::new(0.25, 0.5, 1.));
        let camera = WorldCamera::stock(
            Vec3::new(100., 0., 0.),
            Vec3::new(200., 0., 0.),
            Vec3::Z,
            100.,
        )
        .frame(1.)?;
        let lifetime = Rc::new(());
        let mut events = Vec::new();
        for now in [1., 121.] {
            frame.update_creature_states(
                &presentation.resident_creature_frame_inputs(),
                now,
                &mut random,
            )?;
            let mut callback = |event: &super::super::super::RuntimeM2Event,
                                animation: &Rc<UnitAnimationBehavior>,
                                model: &DecodedM2Model,
                                transform: Mat4| {
                assert_eq!(event.identifier(), *b"$BTH");
                assert_eq!(event.owner_guid(), Some(30));
                assert!(Rc::ptr_eq(animation, &owner));
                // The mount has the opposite attachment. 6F9260 selects through
                // Unit_C+B4, so both native adapter invocations use the body.
                assert!(model.attachment(attachment).is_some());
                assert!(
                    model
                        .attachment(if attachment == 17 { 19 } else { 17 })
                        .is_none()
                );
                assert!(
                    transform.abs_diff_eq(body_transform, 0.00001),
                    "callback {}: {transform:?}; expected {body_transform:?}",
                    events.len()
                );
                if events.is_empty() {
                    assert!(
                        event.position().abs_diff_eq(mount_event_position, 0.00001),
                        "mount event {:?}; expected {mount_event_position:?}; body {body_position:?}",
                        event.position()
                    );
                    assert_eq!(
                        animation.playback().borrow().previous_event_scene_time_ms,
                        1
                    );
                } else {
                    assert!(event.position().abs_diff_eq(body_position, 0.00001));
                }
                events.push(event.position());
                Some(UnitEffectRequest {
                    identity: world.object_identity(30)?,
                    lifetime: Rc::downgrade(&lifetime),
                    kind: UnitWaterEffect::UnderwaterBreath.into(),
                    kit: None,
                    sound_entry: 0,
                    binding: UnitEffectBinding::Attached {
                        owner: Rc::downgrade(animation),
                        model_scale: 1.,
                        attachment: if model.attachment(17).is_some() {
                            17
                        } else {
                            19
                        },
                    },
                })
            };
            frame.prepare_visible_draws_with_unit_effects(
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
                None,
                None,
                None,
            )?;
        }
        assert_eq!(
            events.len(),
            2,
            "both models dispatch exactly once before draw culling"
        );
        let effects = frame
            .placements
            .iter()
            .filter(|placement| placement.unit_effect.is_some())
            .collect::<Vec<_>>();
        assert_eq!(effects.len(), 2);
        assert!(
            effects[0]
                .unit_effect
                .as_ref()
                .ok_or("first effect")?
                .retiring()
        );
        assert!(
            !effects[1]
                .unit_effect
                .as_ref()
                .ok_or("replacement effect")?
                .retiring()
        );
        for effect in effects {
            assert!(
                effect
                    .transform
                    .w_axis
                    .truncate()
                    .abs_diff_eq(effect_position, 0.00001)
            );
            assert_eq!(
                effect
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
    }
    Ok(())
}
