//! Wound admission and secondary bone routing from native `736640`.

#[cfg(test)]
#[path = "../../tests/movement/unit_wound.rs"]
mod tests;

/// Retained Unit_C/model state observed at wound admission.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnitWoundAnimationInput {
    /// The original caller's critical-wound argument.
    pub critical: bool,
    /// Retained attack owner at Unit_C +A20, independent of selection/target.
    pub attack_target_guid: u64,
    /// Movement_C +44, reached both through Unit_C +D8 and its +788 embedding.
    pub movement_flags: u32,
    /// Replicated low stand-state byte.
    pub stand: u8,
    /// A mount model is currently bound.
    pub mounted: bool,
    /// Primary model animation ID, before AnimationData behavior lookup.
    pub primary_animation: u16,
    /// AnimationData behavior for the primary model sequence.
    pub primary_behavior: u16,
    /// Upper-body primary animation when bound, otherwise the model primary.
    pub current_animation: u16,
    /// AnimationData behavior for the currently selected model layer.
    pub current_behavior: u16,
    /// Semantic bone 4, then 6, then the root fallback when neither exists.
    pub upper_body_key_bone: Option<u16>,
    /// The shared native dead predicate has admitted death/feign/posture.
    pub dead: bool,
    /// The model exists and has completed residency.
    pub model_ready: bool,
    /// Bound Creature template flags, if a template exists.
    pub template_flags: Option<u32>,
    /// At least one active visual kit carries instance flag 4.
    pub effect_blocks_wound: bool,
    /// The unit has the vehicle controller excluded by 722180.
    pub vehicle_blocks_wound: bool,
    /// Movement_C +48, the high word of the live movement flags.
    pub secondary_movement_flags: u16,
    /// Flags on the current spline handler, including its finished bit.
    pub movement_handler_flags: u32,
}

/// A wound occupies the secondary slot; it never replaces the primary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnitWoundAnimation {
    /// Requested base wound behavior before unit tier/model fallback.
    pub animation: u16,
    /// `None` addresses the model root (native key -1).
    pub key_bone: Option<u16>,
}

/// Routes an admitted normal/critical wound before model/tier resolution.
#[must_use]
pub fn resolve_unit_wound_animation(input: UnitWoundAnimationInput) -> Option<UnitWoundAnimation> {
    if input.dead
        || !input.model_ready
        || input.vehicle_blocks_wound
        || input.effect_blocks_wound
        || input
            .template_flags
            .map_or(input.current_animation == 121, |flags| flags & 8 != 0)
    {
        return None;
    }
    // 71C6C0's handler 200 branch skips the ordinary state tests.
    let handler = input.movement_handler_flags;
    let special_movement = input.movement_flags & 0x40000000 != 0
        || if handler & 0x600 == 0x200 {
            false
        } else {
            handler & 0x2400 == 0x2000
                || input.movement_flags & 0x400 != 0
                || input.secondary_movement_flags & 4 != 0
        };
    let animation = if input.critical {
        10
    } else if input.attack_target_guid != 0 {
        9
    } else {
        8
    };
    let full_body = animation == 8
        && !input.mounted
        && input.movement_flags & 0x2e0100f == 0
        && !special_movement
        && !matches!(input.stand, 1 | 4..=6)
        && input.current_behavior != 98;
    let key_bone =
        if full_body || input.primary_animation == 0 || matches!(input.primary_behavior, 25..=29) {
            None
        } else {
            input.upper_body_key_bone
        };
    Some(UnitWoundAnimation {
        animation,
        key_bone,
    })
}
