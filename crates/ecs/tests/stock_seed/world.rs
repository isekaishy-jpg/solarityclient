//! External stock-compatibility tests for active-world ownership.

use std::error::Error;

use glam::Vec3;
use solarity_ecs::{
    ActiveWorld, LocalPlayer, ObjectGuid, PlayerIdentity, WorldBootstrap, WorldMapId,
    WorldTransform,
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
    let local_player = world.local_player();
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
