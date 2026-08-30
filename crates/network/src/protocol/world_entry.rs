//! Selected-character result packets at the transition into the active world.

use thiserror::Error;

/// Authoritative initial map and transform from `SMSG_LOGIN_VERIFY_WORLD`.
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
                "login world location must contain exactly 20 bytes",
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

    /// Returns the initial world X coordinate.
    #[must_use]
    pub const fn x(self) -> f32 {
        self.x
    }

    /// Returns the initial world Y coordinate.
    #[must_use]
    pub const fn y(self) -> f32 {
        self.y
    }

    /// Returns the initial world Z coordinate.
    #[must_use]
    pub const fn z(self) -> f32 {
        self.z
    }

    /// Returns the initial facing orientation in radians.
    #[must_use]
    pub const fn orientation(self) -> f32 {
        self.orientation
    }
}

/// Known build-12340 rejection codes for `SMSG_CHARACTER_LOGIN_FAILED`.
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

/// Exact one-byte result returned when selected-character login fails.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharacterLoginRejection {
    result_code: u8,
}

impl CharacterLoginRejection {
    pub(crate) fn decode(payload: &[u8]) -> Result<Self, WorldEntryPacketError> {
        let [result_code] = payload else {
            return Err(WorldEntryPacketError::new(
                payload.len(),
                "character login rejection must contain exactly one byte",
            ));
        };
        Ok(Self {
            result_code: *result_code,
        })
    }

    /// Returns the exact build-12340 `WorldResult` byte.
    #[must_use]
    pub const fn result_code(self) -> u8 {
        self.result_code
    }

    /// Returns a typed reason for character-login result codes.
    #[must_use]
    pub const fn reason(self) -> Option<CharacterLoginRejectionReason> {
        Some(match self.result_code {
            0x4E => CharacterLoginRejectionReason::NoWorld,
            0x4F => CharacterLoginRejectionReason::DuplicateCharacter,
            0x50 => CharacterLoginRejectionReason::NoInstances,
            0x51 => CharacterLoginRejectionReason::Failed,
            0x52 => CharacterLoginRejectionReason::Disabled,
            0x53 => CharacterLoginRejectionReason::NoCharacter,
            0x54 => CharacterLoginRejectionReason::LockedForTransfer,
            0x55 => CharacterLoginRejectionReason::LockedByBilling,
            0x56 => CharacterLoginRejectionReason::LockedByMobileAuctionHouse,
            _ => return None,
        })
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
