//! Selected-character result packets at the transition into the active world.

use thiserror::Error;

/// Authoritative map and transform from login verification or world replacement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldLocation {
    map_id: u32,
    x: f32,
    y: f32,
    z: f32,
    orientation: f32,
}

impl WorldLocation {
    pub(crate) fn decode(payload: &[u8]) -> Result<Self, WorldEntryPacketError> {
        if payload.len() != 20 {
            return Err(WorldEntryPacketError::new(
                payload.len(),
                "world location must contain exactly 20 bytes",
            ));
        }
        Ok(Self {
            map_id: read_u32(payload, 0),
            x: read_f32(payload, 4),
            y: read_f32(payload, 8),
            z: read_f32(payload, 12),
            orientation: read_f32(payload, 16),
        })
    }

    /// Returns the authoritative Map.dbc identifier to load.
    #[must_use]
    pub const fn map_id(self) -> u32 {
        self.map_id
    }

    /// Returns the authoritative world X coordinate.
    #[must_use]
    pub const fn x(self) -> f32 {
        self.x
    }

    /// Returns the authoritative world Y coordinate.
    #[must_use]
    pub const fn y(self) -> f32 {
        self.y
    }

    /// Returns the authoritative world Z coordinate.
    #[must_use]
    pub const fn z(self) -> f32 {
        self.z
    }

    /// Returns the authoritative facing orientation in radians.
    #[must_use]
    pub const fn orientation(self) -> f32 {
        self.orientation
    }
}

/// Known build-12340 reason bytes for `SMSG_CHARACTER_LOGIN_FAILED`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CharacterLoginRejectionReason {
    /// No world server can accept the character.
    NoWorld,
    /// The character is already present in the world.
    DuplicateCharacter,
    /// No instance server is available.
    NoInstances,
    /// The world rejected the character for an unspecified reason.
    Failed,
    /// Character login is disabled.
    Disabled,
    /// The selected character no longer exists.
    NoCharacter,
    /// The character is locked for transfer.
    LockedForTransfer,
    /// The character is locked by billing state.
    LockedByBilling,
    /// The character is locked by mobile auction-house use.
    LockedByMobileAuctionHouse,
}

/// Exact one-byte reason returned when selected-character login fails.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharacterLoginRejection {
    reason_code: u8,
}

impl CharacterLoginRejection {
    pub(crate) fn decode(payload: &[u8]) -> Result<Self, WorldEntryPacketError> {
        let [reason_code] = payload else {
            return Err(WorldEntryPacketError::new(
                payload.len(),
                "character login rejection must contain exactly one byte",
            ));
        };
        Ok(Self {
            reason_code: *reason_code,
        })
    }

    /// Returns the exact build-12340 `LoginFailureReason` byte.
    #[must_use]
    pub const fn reason_code(self) -> u8 {
        self.reason_code
    }

    /// Returns a typed reason for character-login result codes.
    #[must_use]
    pub const fn reason(self) -> Option<CharacterLoginRejectionReason> {
        Some(match self.reason_code {
            0 => CharacterLoginRejectionReason::Failed,
            1 => CharacterLoginRejectionReason::NoWorld,
            2 => CharacterLoginRejectionReason::DuplicateCharacter,
            3 => CharacterLoginRejectionReason::NoInstances,
            4 => CharacterLoginRejectionReason::Disabled,
            5 => CharacterLoginRejectionReason::NoCharacter,
            6 => CharacterLoginRejectionReason::LockedForTransfer,
            7 => CharacterLoginRejectionReason::LockedByBilling,
            8 => CharacterLoginRejectionReason::LockedByMobileAuctionHouse,
            _ => return None,
        })
    }

    /// Returns the build-12340 localization token associated with this result.
    ///
    /// Stock callback `0x006B2070` converts wire reasons one through eight to
    /// `WorldResult` values `0x4E..=0x56`. Reason zero and unknown values use
    /// the same generic token as stock's default callback arm (`0x51`). The
    /// response-token table consumed by Glue starts at `0x00AD91F0`.
    #[must_use]
    pub const fn message_token(self) -> &'static str {
        match self.reason_code {
            1 => "CHAR_LOGIN_NO_WORLD",
            2 => "CHAR_LOGIN_DUPLICATE_CHARACTER",
            3 => "CHAR_LOGIN_NO_INSTANCES",
            4 => "CHAR_LOGIN_DISABLED",
            5 => "CHAR_LOGIN_NO_CHARACTER",
            6 => "CHAR_LOGIN_LOCKED_FOR_TRANSFER",
            7 => "CHAR_LOGIN_LOCKED_BY_BILLING",
            8 => "CHAR_LOGIN_LOCKED_BY_MOBILE_AH",
            _ => "CHAR_LOGIN_FAILED",
        }
    }
}

/// A malformed selected-character result packet.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("malformed world-entry packet at byte {offset}: {message}")]
pub struct WorldEntryPacketError {
    offset: usize,
    message: &'static str,
}

impl WorldEntryPacketError {
    const fn new(offset: usize, message: &'static str) -> Self {
        Self { offset, message }
    }

    /// Returns the byte offset or invalid packet extent.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }

    /// Returns a stable description of the rejected field.
    #[must_use]
    pub const fn message(&self) -> &'static str {
        self.message
    }
}

fn read_u32(payload: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        payload[offset],
        payload[offset + 1],
        payload[offset + 2],
        payload[offset + 3],
    ])
}

fn read_f32(payload: &[u8], offset: usize) -> f32 {
    f32::from_bits(read_u32(payload, offset))
}
