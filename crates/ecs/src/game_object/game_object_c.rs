//! Stock game-object presentation projected from the replicated field table.

use shipyard::Component;

/// Render-resource identity carried by `GAMEOBJECT_DISPLAYID`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Component)]
pub struct GameObjectPresentation {
    display_id: u32,
    flags: u32,
    bytes_1: u32,
}

impl GameObjectPresentation {
    /// Creates the typed game-object display view.
    #[must_use]
    pub const fn new(display_id: u32, state: u8) -> Self {
        Self::from_fields(display_id, 0, state as u32)
    }

    /// Creates the complete display, flags, and packed state-byte view.
    #[must_use]
    pub const fn from_fields(display_id: u32, flags: u32, bytes_1: u32) -> Self {
        Self {
            display_id,
            flags,
            bytes_1,
        }
    }

    /// Returns the referenced `GameObjectDisplayInfo.dbc` identifier.
    #[must_use]
    pub const fn display_id(self) -> u32 {
        self.display_id
    }

    /// Returns byte zero of build-12340 `GAMEOBJECT_BYTES_1`.
    #[must_use]
    pub const fn state(self) -> u8 {
        self.bytes_1 as u8
    }

    /// Returns the exact `GAMEOBJECT_FLAGS` word.
    #[must_use]
    pub const fn flags(self) -> u32 {
        self.flags
    }

    /// Returns the complete `GAMEOBJECT_BYTES_1` word.
    #[must_use]
    pub const fn bytes_1(self) -> u32 {
        self.bytes_1
    }

    /// Returns byte one, the stock GameObject type selecting its behavior owner.
    #[must_use]
    pub const fn object_type(self) -> u8 {
        (self.bytes_1 >> 8) as u8
    }

    /// Returns byte two, the authored art-kit selector.
    #[must_use]
    pub const fn art_kit(self) -> u8 {
        (self.bytes_1 >> 16) as u8
    }

    /// Returns byte three, the replicated animation progress.
    #[must_use]
    pub const fn animation_progress(self) -> u8 {
        (self.bytes_1 >> 24) as u8
    }
}
