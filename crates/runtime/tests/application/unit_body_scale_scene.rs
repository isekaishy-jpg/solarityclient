//! Scale changes reach the renderer through live creature residency.

use super::*;
use crate::application::RuntimeCreaturePoll;

/// Distinct display/model scales, late family admission, level and pet-number
/// updates must affect the actual model matrix while retaining the server word.
#[test]
fn creature_body_scale_updates_reach_gpu_placement() -> Result<(), Box<dyn Error>> {
    let _sdl_guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let fixture = crate::test_support::unit_models::fixture_with_body_scale()?;
    let mut presentation = unit_presentation(&fixture)?;
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.0,
    ));
    add_unit(&mut world, 30, ObjectKind::Unit, 0)?;
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
    for (family, level, pet_number, object_scale, expected) in [
        (None, 3, 0, 2.0_f32, 1.0_f32),
        (Some(1), 3, 0, 2.0, 1.5),
        (Some(1), 5, 0, 2.0, 2.5),
        (Some(1), 1, 0, 2.0, 1.0),
        (Some(1), 1, 77, 2.0, 0.5),
        (Some(1), 1, 77, 1.0, 0.25),
        (None, 1, 77, 1.0, 0.5),
    ] {
        let fields = [(4, object_scale.to_bits()), (54, level), (75, pet_number)];
        world.update_fields(30, fields)?;
        solarity_systems::project_object_fields(&mut world, 30, fields)?;
        assert_eq!(
            presentation.synchronize_creatures(Some(&world), |_| family.map(|id| (id, 0)))?,
            RuntimeCreaturePoll::ModelsChanged,
        );
        let inputs = presentation.resident_creature_frame_inputs();
        assert_eq!(inputs[0].object_scale(), expected);
        assert_eq!(
            world
                .object_presentation(30)
                .ok_or("object fields")?
                .scale(),
            object_scale
        );
        frame.replace_creatures(&mut renderer, &inputs, &mut random)?;
        let placement = frame
            .placements
            .iter()
            .find(|placement| placement.owner == M2GpuPlacementOwner::CreatureBody { guid: 30 })
            .ok_or("creature placement")?;
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            assert_eq!(
                placement.transform.transform_vector3(axis).length(),
                expected
            );
        }
        assert_eq!(
            presentation.synchronize_creatures(Some(&world), |_| family.map(|id| (id, 0)))?,
            RuntimeCreaturePoll::Current,
        );
    }
    Ok(())
}
