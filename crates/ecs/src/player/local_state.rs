//! Player_C's local stand state, separate from replicated UNIT_FIELD_BYTES_1.

use shipyard::Component;

/// Immediate local presentation state at native player offset `0x1920`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Component)]
pub struct PlayerLocalStandState(u8);

impl PlayerLocalStandState {
    /// Retains the state byte supplied by the native local or server owner.
    #[must_use]
    pub const fn new(state: u8) -> Self {
        Self(state)
    }
    /// Returns the locally effective state without coercing unknown wire values.
    #[must_use]
    pub const fn value(self) -> u8 {
        self.0
    }
}
