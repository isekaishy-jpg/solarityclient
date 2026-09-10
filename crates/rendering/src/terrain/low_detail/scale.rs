//! Registered horizon distance policy, independent of the ordinary camera clip.

/// Effective `horizonFarclipScale` after the native `78D7C0` callback.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldHorizonScale(f32);

impl WorldHorizonScale {
    /// Applies stock's inclusive three-to-six clamp to a finite CVar value.
    /// Returns `None` when the input cannot form a finite projection.
    #[must_use]
    pub fn new(value: f32) -> Option<Self> {
        value.is_finite().then(|| Self(value.clamp(3.0, 6.0)))
    }

    /// Returns the multiplier applied to the effective world viewing distance.
    #[must_use]
    pub const fn value(self) -> f32 {
        self.0
    }
}

impl Default for WorldHorizonScale {
    fn default() -> Self {
        // 78E61D registers "4.0" and 78D7C0 calls 77F4A0 to replace ADEECC.
        // Its image initializer of one is not the registered runtime default.
        Self(4.0)
    }
}
