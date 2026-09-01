//! Dependency-neutral character-creation request and result vocabulary.

use thiserror::Error;
use wow_world_messages::wrath::{Class, Gender, Race};

/// Validated fields written by build 12340's `CMSG_CHAR_CREATE`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacterCreation {
    name: String,
    race: Race,
    class: Class,
    gender: Gender,
    appearance: [u8; 5],
}

impl CharacterCreation {
    /// Validates protocol identities and retains the authored appearance bytes.
    ///
    /// # Errors
    ///
    /// Returns [`CharacterCreationError`] for an unknown race, class, or gender
    /// identifier. Name policy remains authoritative on the world server.
    pub fn new(
        name: String,
        race_id: u8,
        class_id: u8,
        gender_id: u8,
        appearance: [u8; 5],
    ) -> Result<Self, CharacterCreationError> {
        let race = race_id
            .try_into()
            .map_err(|_source| CharacterCreationError::InvalidRace { race_id })?;
        let class = class_id
            .try_into()
            .map_err(|_source| CharacterCreationError::InvalidClass { class_id })?;
        let gender = gender_id
            .try_into()
            .map_err(|_source| CharacterCreationError::InvalidGender { gender_id })?;
        Ok(Self {
            name,
            race,
            class,
            gender,
            appearance,
        })
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) const fn race(&self) -> Race {
        self.race
    }

    pub(crate) const fn class(&self) -> Class {
        self.class
    }

    pub(crate) const fn gender(&self) -> Gender {
        self.gender
    }

    pub(crate) const fn appearance(&self) -> [u8; 5] {
        self.appearance
    }
}

/// One exact `SMSG_CHAR_CREATE` world-result code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharacterCreationResult(u8);

impl CharacterCreationResult {
    /// Build 12340's successful creation result.
    pub const SUCCESS: Self = Self(47);

    pub(crate) fn decode(payload: &[u8]) -> Result<Self, CharacterCreationError> {
        let [result_code] = payload else {
            return Err(CharacterCreationError::InvalidResponseSize {
                actual: payload.len(),
            });
        };
        if !(46..=69).contains(result_code) {
            return Err(CharacterCreationError::InvalidResult {
                result_code: *result_code,
            });
        }
        Ok(Self(*result_code))
    }

    /// Returns the exact numeric `WorldResult` value.
    #[must_use]
    pub const fn result_code(self) -> u8 {
        self.0
    }

    /// Reports whether the world accepted the character.
    #[must_use]
    pub const fn is_success(self) -> bool {
        self.0 == Self::SUCCESS.0
    }

    /// Returns the build-12340 localization token associated with this result.
    #[must_use]
    pub const fn message_token(self) -> &'static str {
        match self.0 {
            46 => "CHAR_CREATE_IN_PROGRESS",
            47 => "CHAR_CREATE_SUCCESS",
            48 => "CHAR_CREATE_ERROR",
            49 => "CHAR_CREATE_FAILED",
            50 => "CHAR_CREATE_NAME_IN_USE",
            51 => "CHAR_CREATE_DISABLED",
            52 => "CHAR_CREATE_PVP_TEAMS_VIOLATION",
            53 => "CHAR_CREATE_SERVER_LIMIT",
            54 => "CHAR_CREATE_ACCOUNT_LIMIT",
            55 => "CHAR_CREATE_SERVER_QUEUE",
            56 => "CHAR_CREATE_ONLY_EXISTING",
            57 => "CHAR_CREATE_EXPANSION",
            58 => "CHAR_CREATE_EXPANSION_CLASS",
            59 => "CHAR_CREATE_LEVEL_REQUIREMENT",
            60 => "CHAR_CREATE_UNIQUE_CLASS_LIMIT",
            61 => "CHAR_CREATE_CHARACTER_IN_GUILD",
            62 => "CHAR_CREATE_RESTRICTED_RACECLASS",
            63 => "CHAR_CREATE_CHARACTER_CHOOSE_RACE",
            64 => "CHAR_CREATE_CHARACTER_ARENA_LEADER",
            65 => "CHAR_CREATE_CHARACTER_DELETE_MAIL",
            66 => "CHAR_CREATE_CHARACTER_SWAP_FACTION",
            67 => "CHAR_CREATE_CHARACTER_RACE_ONLY",
            68 => "CHAR_CREATE_CHARACTER_GOLD_LIMIT",
            69 => "CHAR_CREATE_FORCE_LOGIN",
            _ => "CHAR_CREATE_ERROR",
        }
    }
}

/// Invalid creation input or malformed authoritative result.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum CharacterCreationError {
    /// The request carries no known build-12340 race.
    #[error("character creation race ID {race_id} is invalid")]
    InvalidRace {
        /// Rejected protocol identifier.
        race_id: u8,
    },
    /// The request carries no known build-12340 class.
    #[error("character creation class ID {class_id} is invalid")]
    InvalidClass {
        /// Rejected protocol identifier.
        class_id: u8,
    },
    /// The request carries neither the male nor female protocol gender.
    #[error("character creation gender ID {gender_id} is invalid")]
    InvalidGender {
        /// Rejected protocol identifier.
        gender_id: u8,
    },
    /// `SMSG_CHAR_CREATE` was not its exact one-byte representation.
    #[error("SMSG_CHAR_CREATE body has {actual} bytes instead of 1")]
    InvalidResponseSize {
        /// Received body length.
        actual: usize,
    },
    /// The one-byte response is outside build 12340's creation-result range.
    #[error("SMSG_CHAR_CREATE result code {result_code} is invalid")]
    InvalidResult {
        /// Rejected result byte.
        result_code: u8,
    },
}
