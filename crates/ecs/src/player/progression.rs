//! Authoritative local-player progression projected from private update fields.

use shipyard::Component;

/// Experience values exposed by build-12340's player progression APIs.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Component)]
pub struct PlayerProgression {
    experience: u32,
    next_level_experience: u32,
}

impl PlayerProgression {
    /// Creates a complete view of the two adjacent player XP words.
    #[must_use]
    pub const fn new(experience: u32, next_level_experience: u32) -> Self {
        Self {
            experience,
            next_level_experience,
        }
    }

    /// Returns current experience within the player's level.
    #[must_use]
    pub const fn experience(self) -> u32 {
        self.experience
    }

    /// Returns experience required to complete the player's level.
    #[must_use]
    pub const fn next_level_experience(self) -> u32 {
        self.next_level_experience
    }
}
