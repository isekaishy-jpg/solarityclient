//! External stock-compatibility tests for active-world ownership.

use std::error::Error;

use glam::Vec3;
use solarity_ecs::{
    ActiveWorld, LocalPlayer, ObjectFields, ObjectGuid, ObjectKind, PlayerIdentity,
    PlayerViewState, UnitAnimationTier, UnitPresentation, UnitSheathState, WorldBootstrap,
    WorldMapId, WorldStateError, WorldTransform,
};

#[test]
fn game_object_admission_order_and_identity_survive_guid_reuse() -> Result<(), Box<dyn Error>> {
    let bootstrap = WorldBootstrap::new(WorldMapId::new(0), 7, "Local", Vec3::ZERO, 0.0);
    let mut world = ActiveWorld::enter(bootstrap.clone());
    for (guid, kind) in [
        (90, ObjectKind::GameObject),
        (20, ObjectKind::Unit),
        (10, ObjectKind::GameObject),
    ] {
        world.create_object(guid, kind, None, [])?;
    }
    let original = world.visible_game_objects().collect::<Vec<_>>();
    assert_eq!(
        original.iter().map(|id| id.guid()).collect::<Vec<_>>(),
        [90, 10]
    );
    world.create_object(90, ObjectKind::GameObject, None, [])?;
    assert_eq!(world.visible_game_objects().collect::<Vec<_>>(), original);
    world.remove_object(90)?;
    world.create_object(90, ObjectKind::GameObject, None, [])?;
    let recreated = world.visible_game_objects().collect::<Vec<_>>();
    assert_eq!(
        recreated.iter().map(|id| id.guid()).collect::<Vec<_>>(),
        [10, 90]
    );
    assert_eq!(recreated[0], original[1]);
    assert_ne!(recreated[1], original[0]);
    world.remove_object(20)?;
    world.remove_object(90)?;
    world.remove_object(10)?;
    assert_eq!(world.visible_game_objects().count(), 0);
    world.create_object(5, ObjectKind::GameObject, None, [])?;
    assert_eq!(
        world
            .visible_game_objects()
            .map(|id| id.guid())
            .collect::<Vec<_>>(),
        [5]
    );
    let old_local = world.object_identity(7).ok_or("lost local identity")?;
    let mut replacement = ActiveWorld::enter(bootstrap);
    replacement.create_object(5, ObjectKind::GameObject, None, [])?;
    assert_ne!(replacement.object_identity(7), Some(old_local));
    assert_ne!(replacement.object_identity(5), world.object_identity(5));
    Ok(())
}

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
    assert_eq!(world.local_player_view()?, PlayerViewState::STOCK_VIEW_2);
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

/// Visible-unit queries include players and creatures but no other object kind.
#[test]
fn visible_unit_queries_follow_guid_lifecycle() -> Result<(), Box<dyn Error>> {
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        30,
        "Local",
        Vec3::ZERO,
        0.0,
    ));
    world.create_object(
        20,
        ObjectKind::Unit,
        Some(WorldTransform::new(Vec3::X, 1.0)),
        [],
    )?;
    world.create_object(10, ObjectKind::GameObject, None, [])?;
    world.create_object(30, ObjectKind::Player, None, [])?;
    let presentation = UnitPresentation::new(
        7,
        7,
        0,
        0,
        UnitAnimationTier::Ground,
        UnitSheathState::Unarmed,
    );
    let entity = world
        .entity_by_guid(20)
        .ok_or(WorldStateError::UnknownObject { guid: 20 })?;
    world.storage_mut().add_component(entity, (presentation,));

    assert_eq!(world.visible_unit_guids(), [20, 30]);
    assert_eq!(
        world.object_transform(20).map(|value| value.position()),
        Some(Vec3::X)
    );
    assert_eq!(world.unit_presentation(20), Some(presentation));
    world.remove_object(20)?;
    assert_eq!(world.visible_unit_guids(), [30]);
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
    let original_transform = world.object_transform(creature_guid);
    assert_eq!(
        world.create_object(
            creature_guid,
            ObjectKind::GameObject,
            Some(WorldTransform::new(Vec3::splat(100.0), 2.0)),
            [(24, 50), (81, 54_321)],
        )?,
        creature
    );
    let fields = world.storage().get::<&ObjectFields>(creature)?;
    assert_eq!(fields.get(24), 50);
    assert_eq!(fields.get(80), 12_345);
    assert_eq!(fields.get(81), 54_321);
    drop(fields);
    assert_eq!(world.object_kind(creature_guid), Some(ObjectKind::Unit));
    assert_eq!(world.object_transform(creature_guid), original_transform);

    world.remove_object(creature_guid)?;
    assert_eq!(world.entity_by_guid(creature_guid), None);
    assert!(matches!(
        world.update_fields(creature_guid, [(24, 1)]),
        Err(WorldStateError::UnknownObject { .. })
    ));
    Ok(())
}
