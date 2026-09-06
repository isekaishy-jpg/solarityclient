//! Unit_C's ordinary posture selector and primary completion responses.

/// Result of the posture stage in the ordered unit animation resolver.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnitStandAnimationDecision {
    /// Later animation stages may choose the unit's idle behavior.
    Continue,
    /// The posture stage consumes the request without replacing the primary.
    Retain,
    /// Request this base AnimationData behavior before tier/fallback resolution.
    Select(u16),
}

/// Result of an ordinary unit primary completion at `0x0073B510`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnitPrimaryAnimationCompletion {
    /// Re-enter the ordered unit animation resolver.
    Continue,
    /// Keep the completed corpse or water-death pose.
    Retain,
    /// Select a new behavior with an ordinary variation roll.
    Select(u16),
    /// Carry the outgoing death variation into the corpse sequence.
    SelectWithCurrentVariation(u16),
}

/// Resolves `0x0073F060` for a resident unit without a vehicle controller.
///
/// Death entry also executes `0x0073AF80`; `swimming` is the movement flag path.
/// Movement's separate water-height probe and non-animation side effects belong
/// to the caller. A repeated stand value does not re-enter the resolver.
#[must_use]
pub fn resolve_unit_stand_transition(
    stand: u8,
    previous_stand: u8,
    current_behavior: u16,
    swimming: bool,
    has_standup: bool,
) -> UnitStandAnimationDecision {
    use UnitStandAnimationDecision::{Continue, Retain, Select};
    if stand == previous_stand {
        return Retain;
    }
    match stand {
        7 if matches!(current_behavior, 1 | 6 | 131 | 132 | 466..=468 | 472) => Retain,
        7 => Select(if swimming { 131 } else { 466 }),
        9 => Select(201),
        0 if previous_stand == 9 => Select(if has_standup { 127 } else { 224 }),
        _ => Continue,
    }
}

/// Resolves the admitted posture stage at `0x0071E1F0` with all request bits set.
///
/// The caller owns animation-layer and mount admission. `previous_stand` is
/// the last posture processed by Unit_C, not the previous model sequence.
#[must_use]
pub fn resolve_unit_stand_animation(
    stand: u8,
    previous_stand: u8,
    current_behavior: u16,
    swimming: bool,
    has_death_transition: bool,
) -> UnitStandAnimationDecision {
    use UnitStandAnimationDecision::{Continue, Retain, Select};
    match stand {
        0 => match previous_stand {
            1 => Select(98),
            3 => Select(101),
            8 => Select(116),
            _ => Continue,
        },
        1 => Select(if previous_stand == 0 || current_behavior == 96 {
            96
        } else {
            97
        }),
        3 => Select(99 + u16::from(previous_stand != 0)),
        4..=6 => Select(98 + u16::from(stand)),
        7 if previous_stand == 7 => Retain,
        7 if swimming => Select(132),
        7 => Select(if has_death_transition { 472 } else { 6 }),
        8 => Select(114 + u16::from(previous_stand != 0)),
        _ => Continue,
    }
}

/// Resolves posture cases from `0x0073B510` after a primary timer completes.
///
/// `completed_behavior` comes from AnimationData's behavior column for the
/// actual resolved clip; a missing pose that fell back to Stand is not a sit
/// completion. `None` resumes the ordinary unit animation resolver.
#[must_use]
pub fn resolve_unit_stand_completion(completed_behavior: u16, stand: u8) -> Option<u16> {
    match completed_behavior {
        96 | 97 => Some(97 + u16::from(stand != 1)),
        99 | 100 => Some(100 + u16::from(stand != 3)),
        102 if stand == 4 => Some(102),
        103 if stand == 5 => Some(103),
        104 if stand == 6 => Some(104),
        114 | 115 => Some(115 + u16::from(stand != 8)),
        201 => Some(202),
        _ => None,
    }
}

/// Resolves ordinary posture, submerged, and death primary completions.
///
/// The caller supplies Unit_C's dead predicate and alternate death flag. Spell,
/// emote, and layered animation callbacks require their own admission state.
#[must_use]
pub fn resolve_unit_primary_animation_completion(
    completed_behavior: u16,
    stand: u8,
    dead: bool,
    alternate_death: bool,
) -> UnitPrimaryAnimationCompletion {
    use UnitPrimaryAnimationCompletion::{Continue, Retain, Select, SelectWithCurrentVariation};
    if let Some(animation) = resolve_unit_stand_completion(completed_behavior, stand) {
        return Select(animation);
    }
    match (completed_behavior, dead) {
        (1, true) => SelectWithCurrentVariation(6),
        (131, true) => SelectWithCurrentVariation(132),
        (466 | 467, true) => Select(468 - u16::from(alternate_death)),
        (468, true) => SelectWithCurrentVariation(472),
        (6 | 132 | 472, true) => Retain,
        _ => Continue,
    }
}
