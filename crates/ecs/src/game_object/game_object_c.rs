//! Stock game-object presentation projected from the replicated field table.

use shipyard::Component;

/// Render-resource identity carried by `GAMEOBJECT_DISPLAYID`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Component)]
pub struct GameObjectPresentation {
    display_id: u32,
    flags: u32,
    bytes_1: u32,
    dynamic: u32,
    transport_period_ms: u32,
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
            dynamic: 0,
            transport_period_ms: 0,
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

    /// Supplies absolute update word 14, independently of the packed state bytes.
    #[must_use]
    pub const fn with_dynamic_word(mut self, dynamic: u32) -> Self {
        self.dynamic = dynamic;
        self
    }

    /// Returns the exact `GAMEOBJECT_DYNAMIC` word, including its low flags.
    #[must_use]
    pub const fn dynamic_word(self) -> u32 {
        self.dynamic
    }

    /// Supplies absolute GAMEOBJECT_LEVEL word 16, used as the MO route period.
    #[must_use]
    pub const fn with_transport_period_ms(mut self, period_ms: u32) -> Self {
        self.transport_period_ms = period_ms;
        self
    }

    /// Returns the server's route period, including an explicitly supplied zero.
    #[must_use]
    pub const fn transport_period_ms(self) -> u32 {
        self.transport_period_ms
    }

    /// Returns the supplied sequence fraction; `0xFFFF` means no supplied seek.
    ///
    /// This ushort is distinct from byte three of `GAMEOBJECT_BYTES_1`.
    #[must_use]
    pub const fn sequence_progress(self) -> Option<u16> {
        let progress = (self.dynamic >> 16) as u16;
        if progress == u16::MAX {
            None
        } else {
            Some(progress)
        }
    }
}
