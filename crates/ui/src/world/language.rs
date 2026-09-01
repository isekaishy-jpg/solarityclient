//! Localized player-language identity used by native chat queries.

/// One language selected by the active player's build-12340 language state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiPlayerLanguage {
    id: u32,
    name: String,
}

impl UiPlayerLanguage {
    /// Creates a localized language projection from its client table row.
    #[must_use]
    pub fn new(id: u32, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
        }
    }

    /// Returns the client language identifier used on chat packets.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the selected-locale display name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}
