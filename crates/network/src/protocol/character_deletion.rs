//! Dependency-neutral character-deletion result vocabulary.

use thiserror::Error;

/// One exact `SMSG_CHAR_DELETE` world-result code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharacterDeletionResult(u8);

impl CharacterDeletionResult {
    /// Build 12340's successful deletion result.
    pub const SUCCESS: Self = Self(71);

    pub(crate) fn decode(payload: &[u8]) -> Result<Self, CharacterDeletionError> {
        let [result_code] = payload else {
            return Err(CharacterDeletionError::InvalidResponseSize {
                actual: payload.len(),
            });
        };
        if !(70..=75).contains(result_code) {
            return Err(CharacterDeletionError::InvalidResult {
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

    /// Reports whether the world deleted the character.
    #[must_use]
    pub const fn is_success(self) -> bool {
        self.0 == Self::SUCCESS.0
    }

    /// Returns the build-12340 localization token associated with this result.
    #[must_use]
    pub const fn message_token(self) -> &'static str {
        match self.0 {
            70 => "CHAR_DELETE_IN_PROGRESS",
            71 => "CHAR_DELETE_SUCCESS",
            72 => "CHAR_DELETE_FAILED",
            73 => "CHAR_DELETE_FAILED_LOCKED_FOR_TRANSFER",
            74 => "CHAR_DELETE_FAILED_GUILD_LEADER",
            75 => "CHAR_DELETE_FAILED_ARENA_CAPTAIN",
            _ => "CHAR_DELETE_FAILED",
        }
    }
}

/// A malformed authoritative character-deletion result.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum CharacterDeletionError {
    /// `SMSG_CHAR_DELETE` was not its exact one-byte representation.
    #[error("SMSG_CHAR_DELETE body has {actual} bytes instead of 1")]
    InvalidResponseSize {
        /// Received body length.
        actual: usize,
    },
    /// The one-byte response is outside build 12340's deletion-result range.
    #[error("SMSG_CHAR_DELETE result code {result_code} is invalid")]
    InvalidResult {
        /// Rejected result byte.
        result_code: u8,
    },
}
