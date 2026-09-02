//! Build-12340 character-rename request and response vocabulary.

use thiserror::Error;

/// Validated fields written by build 12340's `CMSG_CHAR_RENAME`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacterRename {
    name: String,
}

impl CharacterRename {
    /// Retains one stock-sized C string for an enumerated character.
    ///
    /// # Errors
    ///
    /// Returns [`CharacterRenameError`] when `name` is empty, contains a NUL,
    /// or exceeds the 47-byte stock response buffer boundary.
    pub fn new(name: String) -> Result<Self, CharacterRenameError> {
        if name.is_empty() {
            return Err(CharacterRenameError::InvalidName {
                result: CharacterNameResult::NO_NAME,
            });
        }
        if name.as_bytes().contains(&0) || name.len() > 47 {
            return Err(CharacterRenameError::InvalidName {
                result: CharacterNameResult::TOO_LONG,
            });
        }
        Ok(Self { name })
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }
}

/// One build-12340 local character-name validation result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharacterNameResult(u8);

impl CharacterNameResult {
    /// The submitted name was empty.
    pub const NO_NAME: Self = Self(0x59);
    /// The submitted name exceeded a stock protocol boundary.
    pub const TOO_LONG: Self = Self(0x5B);

    /// Returns the exact numeric `WorldResult` value.
    #[must_use]
    pub const fn result_code(self) -> u8 {
        self.0
    }

    /// Returns the build-12340 localization token associated with this result.
    #[must_use]
    pub const fn message_token(self) -> &'static str {
        character_name_message_token(self.0)
    }
}

/// One exact `SMSG_CHAR_RENAME` result and its success-only identity fields.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacterRenameResult {
    result_code: u8,
    guid: Option<u64>,
    name: Option<String>,
}

impl CharacterRenameResult {
    pub(crate) fn decode(payload: &[u8]) -> Result<Self, CharacterRenameError> {
        let Some((&result_code, remainder)) = payload.split_first() else {
            return Err(CharacterRenameError::InvalidResponseSize { actual: 0 });
        };
        if result_code == 0 {
            if remainder.len() < 9 {
                return Err(CharacterRenameError::TruncatedSuccess {
                    actual: payload.len(),
                });
            }
            let mut guid_bytes = [0_u8; 8];
            guid_bytes.copy_from_slice(&remainder[..8]);
            let guid = u64::from_le_bytes(guid_bytes);
            let name_bytes = &remainder[8..];
            let Some(terminator) = name_bytes.iter().position(|byte| *byte == 0) else {
                return Err(CharacterRenameError::UnterminatedName);
            };
            if terminator + 1 != name_bytes.len() {
                return Err(CharacterRenameError::TrailingSuccessData);
            }
            if terminator > 47 {
                return Err(CharacterRenameError::NameTooLong { actual: terminator });
            }
            let name = std::str::from_utf8(&name_bytes[..terminator])
                .map_err(|_source| CharacterRenameError::InvalidNameEncoding)?
                .to_owned();
            if guid == 0 || name.is_empty() {
                return Err(CharacterRenameError::InvalidSuccessIdentity);
            }
            return Ok(Self {
                result_code,
                guid: Some(guid),
                name: Some(name),
            });
        }
        if payload.len() != 1 {
            return Err(CharacterRenameError::InvalidResponseSize {
                actual: payload.len(),
            });
        }
        if result_code != 0x32 && !(0x58..=0x67).contains(&result_code) {
            return Err(CharacterRenameError::InvalidResult { result_code });
        }
        Ok(Self {
            result_code,
            guid: None,
            name: None,
        })
    }

    /// Returns the exact server result byte (`RESPONSE_SUCCESS` is zero).
    #[must_use]
    pub const fn result_code(&self) -> u8 {
        self.result_code
    }

    /// Reports whether the world renamed the character.
    #[must_use]
    pub const fn is_success(&self) -> bool {
        self.result_code == 0
    }

    /// Returns the renamed character GUID on success.
    #[must_use]
    pub const fn guid(&self) -> Option<u64> {
        self.guid
    }

    /// Returns the server-normalized character name on success.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Returns the build-12340 localization token for a failed rename.
    #[must_use]
    pub const fn message_token(&self) -> &'static str {
        character_name_message_token(self.result_code)
    }
}

const fn character_name_message_token(result_code: u8) -> &'static str {
    match result_code {
        0x32 => "CHAR_CREATE_NAME_IN_USE",
        0x57 => "CHAR_NAME_SUCCESS",
        0x59 => "CHAR_NAME_NO_NAME",
        0x5A => "CHAR_NAME_TOO_SHORT",
        0x5B => "CHAR_NAME_TOO_LONG",
        0x5C => "CHAR_NAME_INVALID_CHARACTER",
        0x5D => "CHAR_NAME_MIXED_LANGUAGES",
        0x5E => "CHAR_NAME_PROFANE",
        0x5F => "CHAR_NAME_RESERVED",
        0x60 => "CHAR_NAME_INVALID_APOSTROPHE",
        0x61 => "CHAR_NAME_MULTIPLE_APOSTROPHES",
        0x62 => "CHAR_NAME_THREE_CONSECUTIVE",
        0x63 => "CHAR_NAME_INVALID_SPACE",
        0x64 => "CHAR_NAME_CONSECUTIVE_SPACES",
        0x65 => "CHAR_NAME_RUSSIAN_CONSECUTIVE_SILENT_CHARACTERS",
        0x66 => "CHAR_NAME_RUSSIAN_SILENT_CHARACTER_AT_BEGINNING_OR_END",
        0x67 => "CHAR_NAME_DECLENSION_DOESNT_MATCH_BASE_NAME",
        _ => "CHAR_NAME_FAILURE",
    }
}

/// Invalid rename input or malformed authoritative response.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum CharacterRenameError {
    /// A local request failed a stock structural name check.
    #[error("character rename failed local validation with result {result:?}")]
    InvalidName {
        /// Exact local result suitable for stock dialog presentation.
        result: CharacterNameResult,
    },
    /// `SMSG_CHAR_RENAME` did not have the failure-only one-byte shape.
    #[error("SMSG_CHAR_RENAME body has invalid size {actual}")]
    InvalidResponseSize {
        /// Received body length.
        actual: usize,
    },
    /// A success response omitted its GUID or C string.
    #[error("successful SMSG_CHAR_RENAME body is truncated at {actual} bytes")]
    TruncatedSuccess {
        /// Received body length.
        actual: usize,
    },
    /// A success response omitted the name terminator.
    #[error("successful SMSG_CHAR_RENAME name is not NUL-terminated")]
    UnterminatedName,
    /// A success response contained bytes after the name terminator.
    #[error("successful SMSG_CHAR_RENAME has trailing data")]
    TrailingSuccessData,
    /// A success response exceeded stock's 47-byte name buffer boundary.
    #[error("successful SMSG_CHAR_RENAME name has {actual} bytes instead of at most 47")]
    NameTooLong {
        /// Received name byte count.
        actual: usize,
    },
    /// A success response name was not UTF-8.
    #[error("successful SMSG_CHAR_RENAME name is not UTF-8")]
    InvalidNameEncoding,
    /// A success response carried an empty name or zero GUID.
    #[error("successful SMSG_CHAR_RENAME has an empty character identity")]
    InvalidSuccessIdentity,
    /// A failure byte was outside the build-12340 character-name range.
    #[error("SMSG_CHAR_RENAME result code {result_code} is invalid")]
    InvalidResult {
        /// Rejected result byte.
        result_code: u8,
    },
}
