//! Native mount/player scales survive residency, attachment posing and dismounts.

use super::*;

/// Original 73D5D0/71C0E0 records exercise both players with non-unit body,
/// display and model scales. The mount attachment must not resize its rider.
#[test]
fn native_mount_scales_reach_local_and_remote_rider_matrices() -> Result<(), Box<dyn Error>> {
    let _sdl_guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let fixture = crate::test_support::unit_models::fixture_with_mount_scale()?;
    let mut presentation = unit_presentation(&fixture)?;
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.0,
    ));
    for guid in [7, 20] {
        add_unit(&mut world, guid, ObjectKind::Player, 0)?;
    }
    let platform = SdlPlatform::start(WindowConfiguration::new(128, 128, WindowMode::Windowed))?;
    let mut renderer = renderer(&platform)?;
    let mut random = CrtRand::new();
    let mut frame = M2Frame::prepare(
        &mut renderer,
        &ResidentM2Scene::default(),
        fixture_animations(&fixture)?,
        &mut random,
        Arc::new(M2ParticleTwinkleTable::new(1)),
    )?;
    let camera = WorldCamera::orthographic(
        Vec3::new(8.0, 0.0, 0.0),
        Vec3::ZERO,
        Vec3::Z,
        [-4.0, 4.0],
        [-2.0, 2.0],
        0.1,
        100.0,
    )
    .frame(1.0)?;
    let native = include_bytes!("../fixtures/unit-mount-scale-native.bin");
    assert_eq!(&native[..8], b"UMS12340");
    let count = u32::from_le_bytes(native[8..12].try_into()?);
    assert_eq!(native.len(), 12 + count as usize * 32);
    for (index, record) in native[12..].as_chunks::<32>().0.iter().enumerate() {
        let mount_id = u32::from_le_bytes(record[..4].try_into()?);
        let mut values = [0.0_f32; 7];
        for (value, bytes) in values.iter_mut().zip(record[4..].as_chunks::<4>().0) {
            *value = f32::from_le_bytes(*bytes);
        }
        let [
            object_scale,
            display_scale,
            _,
            retained,
            reciprocal,
            model_scale,
            body_scale,
        ] = values;
        assert_eq!(retained.to_bits(), display_scale.to_bits());
        for guid in [7, 20] {
            let fields = [(4, object_scale.to_bits()), (69, mount_id)];
            world.update_fields(guid, fields)?;
            solarity_systems::project_object_fields(&mut world, guid, fields)?;
        }
        presentation.synchronize(Some(&world))?;
        presentation.synchronize_remote_players(Some(&world))?;
        let local = presentation.resident_frame_input().ok_or("local player")?;
        let remote = presentation.resident_remote_player_frame_inputs();
        for input in [&local, &remote[0]] {
            assert_eq!(input.object_scale().to_bits(), body_scale.to_bits());
            if let Some(mount) = input.mount() {
                assert_ne!(mount_id, 0);
                assert_eq!(mount.object_scale().to_bits(), model_scale.to_bits());
                assert_eq!(mount.rider_scale().to_bits(), reciprocal.to_bits());
            } else {
                assert_eq!(mount_id, 0);
            }
        }
        frame.replace_player(&mut renderer, Some(local), &mut random)?;
        frame.replace_remote_players(&mut renderer, &remote, &mut random)?;
        frame.prepare_visible_draws(
            &renderer,
            WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
            camera,
            solarity_rendering::M2TransparentPass::One,
            Vec3::ZERO,
            index as f32 * 100.0,
            M2CameraEffectScale::EXTERNAL_CAMERA,
            &mut random,
            None,
        )?;
        for (body, mount) in [
            (
                M2GpuPlacementOwner::PlayerBody { guid: 7 },
                M2GpuPlacementOwner::PlayerMount { guid: 7 },
            ),
            (
                M2GpuPlacementOwner::RemotePlayerBody { guid: 20 },
                M2GpuPlacementOwner::RemotePlayerMount { guid: 20 },
            ),
        ] {
            let rider = frame
                .placements
                .iter()
                .find(|placement| placement.owner == body)
                .ok_or("rider")?;
            // Stock stores a reciprocal before composing the attachment matrix;
            // the final axes retain that float rounding instead of recomputing a ratio.
            let expected = if mount_id == 0 {
                body_scale
            } else {
                model_scale * reciprocal
            };
            for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                let length = rider.transform.transform_vector3(axis).length();
                assert!(
                    (length - expected).abs() < 1e-6,
                    "case {index}: {body:?}: {length} != {expected}"
                );
            }
            let mount = frame
                .placements
                .iter()
                .find(|placement| placement.owner == mount);
            assert_eq!(mount.is_some(), mount_id != 0);
            if let Some(mount) = mount {
                for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                    assert!(
                        (mount.transform.transform_vector3(axis).length() - model_scale).abs()
                            < 1e-6
                    );
                }
            }
        }
    }
    Ok(())
}
