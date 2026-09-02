//! External stock-compatibility tests for `ecs/movement` belong here.

use glam::Vec3;
use solarity_ecs::{
    ActiveWorld, GameObjectPresentation, ObjectKind, WorldBootstrap, WorldMapId,
    WorldMovementSpeeds, WorldMovementState, WorldTransform,
};
use std::error::Error;

/// Living movement flags and all speed modes survive ECS projection unchanged.
#[test]
fn active_world_retains_complete_living_movement_state() -> Result<(), Box<dyn Error>> {
    let guid = 0x0000_0000_0000_0042;
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        guid,
        "Mover",
        Vec3::ZERO,
        0.0,
    ));
    let speeds = WorldMovementSpeeds::new([
        2.5,
        7.0,
        4.5,
        4.72,
        2.5,
        7.0,
        4.5,
        std::f32::consts::PI,
        std::f32::consts::PI,
    ]);
    let transport_guid = 0xF110_0000_0000_002A;
    let movement = WorldMovementState::new(0x0000_0010_0020_0101, speeds, Some(transport_guid));

    world.update_movement(guid, movement)?;

    let retained = world
        .movement_state(guid)
        .ok_or("living movement component was not retained")?;
    assert_eq!(retained.flags(), 0x0000_0010_0020_0101);
    assert_eq!(retained.speeds().values(), speeds.values());
    assert_eq!(retained.transport_guid(), Some(transport_guid));
    assert_eq!(world.local_player_transport_guid(), Some(transport_guid));
    assert!(!world.is_local_player_transport_admitted());
    world.create_object(transport_guid, ObjectKind::GameObject, None, [])?;
    assert!(world.is_local_player_transport_admitted());
    assert!(!world.is_local_player_transport_presentable());
    world.update_transform(transport_guid, WorldTransform::new(Vec3::ZERO, 0.0))?;
    let transport = world
        .entity_by_guid(transport_guid)
        .ok_or("transport entity was absent")?;
    world
        .storage_mut()
        .add_component(transport, (GameObjectPresentation::new(42),));
    assert!(world.is_local_player_transport_presentable());
    Ok(())
}
