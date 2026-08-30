//! Stock implementation responsibility recovered from `Player_C.cpp` and `Player_C.h`.

use shipyard::Component;

/// Player name and stable character identity state.
#[derive(Clone, Debug, Eq, PartialEq, Component)]
pub struct PlayerIdentity {
    name: String,
}

impl PlayerIdentity {
    pub(crate) fn new(name: String) -> Self {
        Self { name }
    }

    /// Returns the server-provided character display name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Marker identifying the one player controlled by this client.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Component)]
pub struct LocalPlayer;
