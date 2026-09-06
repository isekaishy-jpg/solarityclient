//! Stock implementation responsibility recovered from `MovementShared.cpp`.

use solarity_ecs::WorldMovementState;

const MOVEMENT_FORWARD: u64 = 0x0000_0000_0001;
const MOVEMENT_BACKWARD: u64 = 0x0000_0000_0002;
const MOVEMENT_STRAFE_LEFT: u64 = 0x0000_0000_0004;
const MOVEMENT_STRAFE_RIGHT: u64 = 0x0000_0000_0008;
const MOVEMENT_WALKING: u64 = 0x0000_0000_0100;
const MOVEMENT_SWIMMING: u64 = 0x0000_0020_0000;
const MOVEMENT_FLYING: u64 = 0x0000_0200_0000;

const TRANSLATION_MASK: u64 =
    MOVEMENT_FORWARD | MOVEMENT_BACKWARD | MOVEMENT_STRAFE_LEFT | MOVEMENT_STRAFE_RIGHT;
const AQUATIC_MASK: u64 = MOVEMENT_SWIMMING | MOVEMENT_FLYING;

/// One base `AnimationData.dbc` identifier selected from living movement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnitLocomotionAnimation {
    animation_id: u16,
}

impl UnitLocomotionAnimation {
    /// The ordinary stationary animation selected when no movement exists yet.
    pub const STAND: Self = Self::new(0);
    /// The rider pose selected while a separate mount model owns locomotion.
    pub const MOUNT: Self = Self::new(91);

    /// Creates a base unit behavior request before tier and model fallback.
    #[must_use]
    pub const fn new(animation_id: u16) -> Self {
        Self { animation_id }
    }

    /// Returns the exact build-12340 `AnimationData.dbc` identifier.
    #[must_use]
    pub const fn animation_id(self) -> u16 {
        self.animation_id
    }
}

/// Selects the base locomotion animation from build-12340 movement flags.
///
/// The ordering and IDs follow the selector recovered at `Wow.exe` address
/// `0x00717050` and its earlier falling gate in the dispatcher at `0x00724500`.
/// Flying and swimming both select the 41--45 base family here; behavior-tier
/// remapping is a later stock concern and is deliberately not invented here.
#[must_use]
pub const fn resolve_unit_locomotion_animation(
    movement: WorldMovementState,
) -> UnitLocomotionAnimation {
    let flags = movement.flags();
    let vertical_speed = match movement.context().falling {
        Some(fall) => fall.vertical_speed,
        None => 0.0,
    };
    if super::unit_animation::unit_movement_is_airborne(flags as u32, vertical_speed) {
        return UnitLocomotionAnimation::new(40);
    }

    let translating = flags & TRANSLATION_MASK != 0;
    let aquatic = flags & AQUATIC_MASK != 0;
    if !translating {
        return if aquatic {
            UnitLocomotionAnimation::new(41)
        } else {
            UnitLocomotionAnimation::STAND
        };
    }

    if aquatic {
        if flags & MOVEMENT_STRAFE_LEFT != 0 {
            return UnitLocomotionAnimation::new(43);
        }
        if flags & MOVEMENT_STRAFE_RIGHT != 0 {
            return UnitLocomotionAnimation::new(44);
        }
        return if flags & MOVEMENT_BACKWARD != 0 {
            UnitLocomotionAnimation::new(45)
        } else {
            UnitLocomotionAnimation::new(42)
        };
    }

    if flags & MOVEMENT_BACKWARD != 0 {
        return UnitLocomotionAnimation::new(13);
    }
    if flags & MOVEMENT_WALKING != 0 {
        UnitLocomotionAnimation::new(4)
    } else {
        UnitLocomotionAnimation::new(5)
    }
}
