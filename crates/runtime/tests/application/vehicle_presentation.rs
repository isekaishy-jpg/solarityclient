//! Native seat joins and the live model admission consumer.

use super::*;
use solarity_asset::VehicleCatalog;
use solarity_ecs::{
    WorldMovementContext, WorldMovementSpeeds, WorldMovementState, WorldMovementTransport,
};

#[test]
fn vehicle_seat_join_and_fade_match_native_for_every_seat_byte() -> Result<(), Box<dyn Error>> {
    let fixture = crate::test_support::unit_models::fixture()?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let catalog = VehicleCatalog::load(&mut store)?;
    let mut count = 0;
    for line in include_str!("../fixtures/vehicle_seat_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let values = line
            .split_whitespace()
            .map(|word| u32::from_str_radix(word, 16))
            .collect::<Result<Vec<_>, _>>()?;
        let seat = if values[0] == 0 {
            None
        } else {
            catalog.passenger_seat(if values[0] == 2 { 1 } else { 0 }, values[1] as u8 as i8)
        };
        assert_eq!(seat.map_or(0, |seat| seat.id()), values[4], "{line}");
        let duration = solarity_systems::EntityOpacity::unit_entry_duration(
            0,
            0,
            0,
            0xf050_0000_0000_0001,
            (values[2] != 0).then_some(values[3] != 0),
            seat.map(|seat| seat.attachment_id()),
        );
        assert_eq!(duration, values[5] * 1000, "{line}");
        count += 1;
    }
    assert_eq!(count, 3072);
    Ok(())
}

#[test]
fn vehicle_passenger_model_admission_uses_attachment_and_keeps_selected_duration()
-> Result<(), Box<dyn Error>> {
    let fixture = crate::test_support::unit_models::fixture()?;
    let mut presentation = unit_presentation(&fixture)?;
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Passenger",
        Vec3::ZERO,
        0.0,
    ));
    add_unit(&mut world, 30, ObjectKind::Unit, 0)?;
    world.set_unit_vehicle(30, 1, 0.25);
    presentation.set_animation_scene_time(100);
    presentation.synchronize_creatures(Some(&world), |_| None)?;
    let parent = opacity(&presentation, 30)?;
    assert!(parent.transitioning());
    for (guid, seat) in [(31, 0), (32, 2), (33, 1), (34, -1)] {
        add_unit(&mut world, guid, ObjectKind::Unit, 0)?;
        world.update_movement(
            guid,
            WorldMovementState::new(
                0x200,
                WorldMovementSpeeds::new([0.; 9]),
                WorldMovementContext {
                    transport: Some(WorldMovementTransport {
                        guid: 30,
                        position: Vec3::ZERO,
                        orientation: 0.,
                        time_ms: 0,
                        seat,
                        interpolated_time_ms: None,
                    }),
                    ..Default::default()
                },
            ),
        )?;
    }
    presentation.synchronize_creatures(Some(&world), |_| None)?;
    let unattached = opacity(&presentation, 31)?;
    assert!(
        unattached.transitioning(),
        "negative attachment may fade with its parent"
    );
    assert_eq!(unattached.opacity(), 0.0);
    for guid in [32, 33, 34] {
        let passenger = opacity(&presentation, guid)?;
        assert!(
            !passenger.transitioning(),
            "bone-bound or unresolved seat publishes immediately"
        );
        assert_eq!(passenger.opacity(), 1.0);
    }
    unattached.advance(600);
    assert!((unattached.opacity() - 127.0 / 255.0).abs() < 1e-6);
    world.set_unit_vehicle(30, 0, 1.0);
    presentation.set_animation_scene_time(600);
    presentation.synchronize_creatures(Some(&world), |_| None)?;
    assert!(Rc::ptr_eq(&unattached, &opacity(&presentation, 31)?));
    assert!(
        unattached.transitioning(),
        "later seat changes cannot retime an admitted display"
    );
    unattached.advance(1100);
    assert_eq!(unattached.opacity(), 1.0);
    // A new identity must re-evaluate against the now unresolved vehicle row.
    world.remove_object(31)?;
    presentation.synchronize_creatures(Some(&world), |_| None)?;
    add_unit(&mut world, 31, ObjectKind::Unit, 0)?;
    world.update_movement(31, world.movement_state(32).ok_or("passenger movement")?)?;
    presentation.synchronize_creatures(Some(&world), |_| None)?;
    let replacement = opacity(&presentation, 31)?;
    assert!(!Rc::ptr_eq(&replacement, &unattached));
    assert_eq!(replacement.opacity(), 1.0);
    Ok(())
}

fn opacity(
    presentation: &crate::application::RuntimePlayerPresentation,
    guid: u64,
) -> Result<Rc<crate::application::entity_opacity::EntityOpacityOwner>, Box<dyn Error>> {
    presentation
        .resident_creature_frame_inputs()
        .iter()
        .find(|input| input.guid() == guid)
        .and_then(|input| input.unit_animation())
        .map(|owner| Rc::clone(owner.opacity_owner()))
        .ok_or_else(|| "passenger opacity owner".into())
}
