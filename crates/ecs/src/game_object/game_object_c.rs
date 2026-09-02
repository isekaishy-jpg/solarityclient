//! Stock game-object presentation projected from the replicated field table.

use shipyard::Component;

/// Render-resource identity carried by `GAMEOBJECT_DISPLAYID`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Component)]
pub struct GameObjectPresentation {
    display_id: u32,
    state: u8,
}

impl GameObjectPresentation {
    /// Creates the typed game-object display view.
    #[must_use]
    pub const fn new(display_id: u32, state: u8) -> Self {
        Self { display_id, state }
    }

    /// Returns the referenced `GameObjectDisplayInfo.dbc` identifier.
    #[must_use]
    pub const fn display_id(self) -> u32 {
        self.display_id
    }

    /// Returns byte zero of build-12340 `GAMEOBJECT_BYTES_1`.
    #[must_use]
    pub const fn state(self) -> u8 {
        self.state
    }
}
