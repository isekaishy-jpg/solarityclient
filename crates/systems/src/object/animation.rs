//! Model-dependent generic GameObject requests from build-12340 `0x0070D1E0`.

use solarity_asset::M2AnimationSet;

/// A generic behavior's request before CM2Model applies AnimationData fallback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GameObjectAnimationRequest {
    animation_id: u16,
    frozen: bool,
    reuse_current: bool,
}

impl GameObjectAnimationRequest {
    /// Resolves missing open/close clips by authored presence, before DBC fallback.
    #[must_use]
    pub fn resolve(animations: &M2AnimationSet, animation_id: u16) -> Self {
        Self::from_presence(animation_id, |id| animations.has_model_animation(id))
    }

    fn from_presence(animation_id: u16, has: impl Fn(u16) -> bool) -> Self {
        let mut result = Self {
            animation_id,
            frozen: false,
            reuse_current: false,
        };
        if has(animation_id) {
            return result;
        }
        result.reuse_current = true;
        result.animation_id = match animation_id {
            146 if !has(148) => 147,
            147 if !has(146) => {
                if has(148) {
                    result.frozen = true;
                    148
                } else {
                    result.reuse_current = false;
                    0
                }
            }
            148 if !has(146) => {
                if has(150) {
                    150
                } else {
                    149
                }
            }
            149 if !has(148) => {
                if has(146) {
                    result.frozen = true;
                    146
                } else {
                    151
                }
            }
            _ => animation_id,
        };
        result
    }

    /// Returns the request passed to CM2Model's own fallback resolver.
    #[must_use]
    pub const fn animation_id(self) -> u16 {
        self.animation_id
    }

    /// Returns whether the behavior passes zero playback speed.
    #[must_use]
    pub const fn frozen(self) -> bool {
        self.frozen
    }

    /// Preserves an already active request on stock's missing-animation branch.
    /// The current ID is CM2Model's requested ID, before AnimationData fallback.
    #[must_use]
    pub const fn preserves_current(self, current_request: u16) -> bool {
        self.reuse_current && self.animation_id == current_request
    }
}

#[cfg(test)]
#[path = "../../tests/support/game_object_animation.rs"]
mod tests;
