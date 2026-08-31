//! Stock animation-tier remapping and DBC fallback traversal.

use solarity_asset::AnimationDataCatalog;
use solarity_ecs::UnitAnimationTier;

use super::UnitLocomotionAnimation;

/// Build 12340 allocates one visited slot for every AnimationData identifier.
const STOCK_ANIMATION_COUNT: usize = 506;

/// One model sequence selected after tier remapping and authored fallbacks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnitModelAnimation {
    animation_id: u16,
}

impl UnitModelAnimation {
    /// Preserves the requested identifier when a model has no sequence table.
    #[must_use]
    pub const fn static_request(requested: UnitLocomotionAnimation) -> Self {
        Self {
            animation_id: requested.animation_id(),
        }
    }

    /// Returns the exact animation identifier present in the selected M2.
    #[must_use]
    pub const fn animation_id(self) -> u16 {
        self.animation_id
    }
}

/// Resolves a base unit animation against one model's available sequences.
///
/// The order follows the build-12340 resolver at `Wow.exe` address
/// `0x007176F0`: map the current behavior into the unit tier, test the model,
/// traverse `AnimationData.Fallback`, try tiered Stand, and finally descend
/// the fixed tier-parent relation. The callback must report whether the M2
/// supplies at least one sequence for the exact identifier.
#[must_use]
pub fn resolve_unit_model_animation<F>(
    catalog: &AnimationDataCatalog,
    requested: UnitLocomotionAnimation,
    mut tier: UnitAnimationTier,
    mut has_sequence: F,
) -> Option<UnitModelAnimation>
where
    F: FnMut(u16) -> bool,
{
    let requested = u32::from(requested.animation_id());
    loop {
        let mut visited = [false; STOCK_ANIMATION_COUNT];
        let mut behavior = requested;
        loop {
            if let Some(candidate) = catalog.tiered_definition(behavior, tier as u32)
                && let Ok(animation_id) = u16::try_from(candidate.id())
                && has_sequence(animation_id)
            {
                return Some(UnitModelAnimation { animation_id });
            }

            let index = usize::try_from(behavior).ok()?;
            if index >= STOCK_ANIMATION_COUNT || visited[index] {
                break;
            }
            let definition = match catalog.definition(behavior) {
                Some(definition) => definition,
                None => break,
            };
            let fallback = definition.fallback_id();
            if fallback == behavior {
                break;
            }
            visited[index] = true;
            behavior = fallback;
        }

        if let Some(stand) = catalog.tiered_definition(0, tier as u32)
            && let Ok(animation_id) = u16::try_from(stand.id())
            && has_sequence(animation_id)
        {
            return Some(UnitModelAnimation { animation_id });
        }

        let parent = fallback_tier(tier);
        if parent == tier {
            return None;
        }
        tier = parent;
    }
}

/// Returns build 12340's fixed parent for one exhausted animation tier.
const fn fallback_tier(tier: UnitAnimationTier) -> UnitAnimationTier {
    match tier {
        UnitAnimationTier::Hover => UnitAnimationTier::Fly,
        UnitAnimationTier::Ground
        | UnitAnimationTier::Swim
        | UnitAnimationTier::Fly
        | UnitAnimationTier::Submerged => UnitAnimationTier::Ground,
    }
}
