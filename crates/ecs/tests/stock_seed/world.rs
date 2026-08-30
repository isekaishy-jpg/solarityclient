//! External stock-compatibility tests for active-world ownership.

use std::error::Error;

use glam::Vec3;
use solarity_ecs::{
    ActiveWorld, LocalPlayer, ObjectFields, ObjectGuid, ObjectKind, PlayerIdentity, WorldBootstrap,
    WorldMapId, WorldStateError, WorldTransform,
};

/// World entry creates one indexed local player from authoritative login facts.
#[test]
fn world_entry_owns_the_initial_local_player() -> Result<(), Box<dyn Error>> {
    let position = Vec3::new(5_812.25, 647.5, 647.9);
    let world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        0xF130_0000_0000_0042,
        "Solarion",
        position,
        1.75,
    ));

    assert_eq!(world.map_id().value(), 571);
    assert_eq!(world.local_player_transform()?.position(), position);
    let local_player = world.local_player();
    assert_eq!(world.local_player_guid()?, 0xF130_0000_0000_0042);
    assert_eq!(
        world.entity_by_guid(0xF130_0000_0000_0042),
        Some(local_player)
    );
    assert_eq!(world.entity_by_guid(7), None);
    assert_eq!(
        world.storage().get::<&ObjectGuid>(local_player)?.value(),
        0xF130_0000_0000_0042
    );
    assert_eq!(
        world.storage().get::<&PlayerIdentity>(local_player)?.name(),
        "Solarion"
    );
    assert_eq!(
        world
            .storage()
            .get::<&WorldTransform>(local_player)?
            .position(),
        position
    );
    assert_eq!(
        world
            .storage()
            .get::<&WorldTransform>(local_player)?
            .orientation(),
        1.75
    );
    let _local_marker = world.storage().get::<&LocalPlayer>(local_player)?;
    Ok(())
}

/// Create, sparse values, movement, and out-of-range updates preserve GUID lifecycle.
#[test]
fn object_updates_mutate_indexed_entities_in_server_order() -> Result<(), Box<dyn Error>> {
    let local_guid = 0xF130_0000_0000_0042;
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        local_guid,
        "Solarion",
        Vec3::ZERO,
        0.0,
    ));
    world.create_object(
        local_guid,
        ObjectKind::Player,
        Some(WorldTransform::new(Vec3::new(1.0, 2.0, 3.0), 0.5)),
        [(2, 0x19), (68, 0x4D0C)],
    )?;
    let local_fields = world.storage().get::<&ObjectFields>(world.local_player())?;
    assert_eq!(local_fields.get(2), 0x19);
    assert_eq!(local_fields.get(68), 0x4D0C);
    drop(local_fields);

    let creature_guid = 0xF130_0000_0000_0099;
    let creature = world.create_object(
        creature_guid,
        ObjectKind::Unit,
        Some(WorldTransform::new(Vec3::new(4.0, 5.0, 6.0), 1.0)),
        [(2, 0x09), (24, 100)],
    )?;
    world.update_fields(creature_guid, [(24, 75), (80, 12_345)])?;
    world.update_transform(
        creature_guid,
        WorldTransform::new(Vec3::new(7.0, 8.0, 9.0), 1.5),
    )?;
    let fields = world.storage().get::<&ObjectFields>(creature)?;
    assert_eq!(fields.get(24), 75);
    assert_eq!(fields.get(80), 12_345);
    drop(fields);
    assert!(matches!(
        world.create_object(creature_guid, ObjectKind::Unit, None, []),
        Err(WorldStateError::DuplicateObject { .. })
    ));

    world.remove_object(creature_guid)?;
    assert_eq!(world.entity_by_guid(creature_guid), None);
    assert!(matches!(
        world.update_fields(creature_guid, [(24, 1)]),
        Err(WorldStateError::UnknownObject { .. })
    ));
    Ok(())
}
