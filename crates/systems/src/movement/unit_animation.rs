//! Ordinary unit movement stages recovered from build-12340 Unit_C.

/// Result of one admitted movement animation stage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnitMovementAnimationDecision {
    /// Continue the ordinary unit resolver.
    Continue,
    /// Keep the current primary sequence.
    Retain,
    /// Request a base AnimationData behavior through model/tier fallback.
    Select(u16),
}

/// The nonspline falling admission at `723350`.
#[must_use]
pub const fn unit_movement_is_airborne(flags: u32, initial_vertical_speed: f32) -> bool {
    flags & 0x1000 != 0 && (flags & 0x2000 != 0 || initial_vertical_speed != 0.0)
}

/// `724200` retains an existing jump/fall family while airborne.
#[must_use]
pub const fn resolve_unit_airborne_animation(
    current_behavior: u16,
) -> UnitMovementAnimationDecision {
    match current_behavior {
        37..=40 | 467 => UnitMovementAnimationDecision::Retain,
        _ => UnitMovementAnimationDecision::Select(40),
    }
}

/// Ordinary `73D2B0` landing without a vehicle or alternate death controller.
///
/// `forced` includes the caller's force argument and the retained forced-fall
/// unit bit. `slow` is `Movement::GetSpeed(0) <= 2 * walk_speed` at `716FA0`.
#[must_use]
pub const fn resolve_unit_landing_animation(
    previous_flags: u32,
    flags: u32,
    forced: bool,
    slow: bool,
) -> UnitMovementAnimationDecision {
    use UnitMovementAnimationDecision::{Continue, Retain, Select};
    if !forced && previous_flags & 0x2000 == 0 {
        return if (flags ^ previous_flags) & 0x40f == 0 {
            Retain
        } else {
            Continue
        };
    }
    if flags & 0x2200000 == 0 {
        if flags & 0xf == 0 {
            return Select(39);
        }
        if flags & 0x102 == 0 && !slow {
            return Select(187);
        }
    }
    Continue
}

/// Ground turn stage `71E180`, after higher-priority movement/combat stages.
///
/// The caller supplies the retained procedural turn bits and attack/landing
/// admission. Spline and vehicle controllers must be admitted separately.
#[must_use]
pub const fn resolve_unit_turn_animation(
    flags: u32,
    secondary_flags: u16,
    procedural_flags: u32,
    blocked: bool,
) -> Option<u16> {
    if blocked
        || flags & 0x42200400 != 0
        || secondary_flags & 4 != 0
        || (flags & 0x30 == 0 && procedural_flags & 0x1800 == 0)
    {
        return None;
    }
    Some(if flags & 0x10 != 0 || procedural_flags & 0x800 != 0 {
        11
    } else {
        12
    })
}

/// Movement branches of primary completion `73B510`.
#[must_use]
pub const fn resolve_unit_movement_animation_completion(
    behavior: u16,
    flags: u32,
) -> UnitMovementAnimationDecision {
    match behavior {
        37 | 38 if flags & 0x2000000 == 0 => UnitMovementAnimationDecision::Select(38),
        40 => UnitMovementAnimationDecision::Select(40),
        _ => UnitMovementAnimationDecision::Continue,
    }
}
