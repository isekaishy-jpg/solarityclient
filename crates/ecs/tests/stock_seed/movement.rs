//! External stock-compatibility tests for `ecs/movement` belong here.

use glam::Vec3;
use solarity_ecs::{
    ActiveWorld, WorldBootstrap, WorldMapId, WorldMovementSpeeds, WorldMovementState,
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
    let movement = WorldMovementState::new(0x0000_0010_0020_0101, speeds);

    world.update_movement(guid, movement)?;

    let retained = world
        .movement_state(guid)
        .ok_or("living movement component was not retained")?;
    assert_eq!(retained.flags(), 0x0000_0010_0020_0101);
    assert_eq!(retained.speeds().values(), speeds.values());
    Ok(())
}
