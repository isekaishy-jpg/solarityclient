//! External stock-compatibility tests for `systems/movement` belong here.

use solarity_ecs::{WorldMovementSpeeds, WorldMovementState};
use solarity_systems::resolve_unit_locomotion_animation;

const SPEEDS: WorldMovementSpeeds =
    WorldMovementSpeeds::new([2.5, 7.0, 4.5, 4.72, 2.5, 7.0, 4.5, 3.0, 3.0]);

fn animation(flags: u64) -> u16 {
    resolve_unit_locomotion_animation(WorldMovementState::new(flags, SPEEDS)).animation_id()
}

/// The recovered selector preserves stock's common ground locomotion IDs.
#[test]
fn ground_locomotion_uses_stock_selector_order() {
    assert_eq!(animation(0), 0);
    assert_eq!(animation(0x0000_0000_0001), 5);
    assert_eq!(animation(0x0000_0000_0101), 4);
    assert_eq!(animation(0x0000_0000_0002), 13);
    assert_eq!(animation(0x0000_0000_1001), 40);
}

/// Swimming and flying share the base 41--45 family before tier remapping.
#[test]
fn aquatic_locomotion_preserves_direction_precedence() {
    for environment in [0x0000_0020_0000, 0x0000_0200_0000] {
        assert_eq!(animation(environment), 41);
        assert_eq!(animation(environment | 0x1), 42);
        assert_eq!(animation(environment | 0x2), 45);
        assert_eq!(animation(environment | 0x4), 43);
        assert_eq!(animation(environment | 0x8), 44);
        assert_eq!(animation(environment | 0xC), 43);
    }
}
