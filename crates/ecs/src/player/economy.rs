//! Authoritative player currency projected from private update fields.

use shipyard::Component;

/// Copper carried by the player in `PLAYER_FIELD_COINAGE`.
///
/// The build-12340 wire field is one unsigned 32-bit word. Keeping that exact
/// representation avoids unit conversions in packet and FrameXML hot paths.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Component)]
pub struct PlayerMoney(u32);

impl PlayerMoney {
    /// Creates a currency value from the server-provided copper count.
    #[must_use]
    pub const fn new(copper: u32) -> Self {
        Self(copper)
    }

    /// Returns the exact server-provided copper count.
    #[must_use]
    pub const fn copper(self) -> u32 {
        self.0
    }
}
