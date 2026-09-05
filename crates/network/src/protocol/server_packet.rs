//! Owned wrapper around one decrypted server packet.

use super::{
    AddonPolicyError, CharacterCreationError, CharacterCreationResult, CharacterDeletionError,
    CharacterDeletionResult, CharacterDirectory, CharacterDirectoryError, CharacterLoginRejection,
    CharacterRenameError, CharacterRenameResult, ObjectUpdateError, WorldActionButtonPacketError,
    WorldActionButtons, WorldAddonManifest, WorldAddonPolicy, WorldEntryPacketError,
    WorldLivenessPacketError, WorldLocation, WorldObjectUpdateBatch, WorldTimePacketError,
    WorldTimeSpeed, WorldTransfer, WorldTransferPacketError,
};

const SMSG_CHAR_CREATE: u16 = 0x003A;
const SMSG_CHAR_ENUM: u16 = 0x003B;
const SMSG_CHAR_DELETE: u16 = 0x003C;
const SMSG_CHARACTER_LOGIN_FAILED: u16 = 0x0041;
const SMSG_LOGIN_SETTIMESPEED: u16 = 0x0042;
const SMSG_LOGIN_VERIFY_WORLD: u16 = 0x0236;
const SMSG_CHAR_RENAME: u16 = 0x02C8;
const SMSG_ACTION_BUTTONS: u16 = 0x0129;
const SMSG_ADDON_INFO: u16 = 0x02EF;
const SMSG_TIME_SYNC_REQ: u16 = 0x0390;
const SMSG_PONG: u16 = 0x01DD;

/// A decoded world packet that retains unsupported payloads for later dispatch.
pub struct WorldServerPacket {
    opcode: u16,
    payload: Vec<u8>,
}

impl WorldServerPacket {
    pub(crate) const fn new(opcode: u16, payload: Vec<u8>) -> Self {
        Self { opcode, payload }
    }

    /// Returns the raw build-12340 opcode.
    #[must_use]
    pub const fn opcode(&self) -> u16 {
        self.opcode
    }

    /// Returns the stock name for protocol packets implemented at this boundary.
    #[must_use]
    pub const fn name(&self) -> Option<&'static str> {
        match self.opcode {
            SMSG_CHAR_CREATE => Some("SMSG_CHAR_CREATE"),
            SMSG_CHAR_ENUM => Some("SMSG_CHAR_ENUM"),
            SMSG_CHAR_DELETE => Some("SMSG_CHAR_DELETE"),
            0x003E => Some("SMSG_NEW_WORLD"),
            0x003F => Some("SMSG_TRANSFER_PENDING"),
            0x0040 => Some("SMSG_TRANSFER_ABORTED"),
            SMSG_CHARACTER_LOGIN_FAILED => Some("SMSG_CHARACTER_LOGIN_FAILED"),
            SMSG_LOGIN_SETTIMESPEED => Some("SMSG_LOGIN_SETTIMESPEED"),
            0x00A9 => Some("SMSG_UPDATE_OBJECT"),
            SMSG_PONG => Some("SMSG_PONG"),
            0x01F6 => Some("SMSG_COMPRESSED_UPDATE_OBJECT"),
            0x01EE => Some("SMSG_AUTH_RESPONSE"),
            SMSG_LOGIN_VERIFY_WORLD => Some("SMSG_LOGIN_VERIFY_WORLD"),
            SMSG_CHAR_RENAME => Some("SMSG_CHAR_RENAME"),
            SMSG_ACTION_BUTTONS => Some("SMSG_ACTION_BUTTONS"),
            SMSG_ADDON_INFO => Some("SMSG_ADDON_INFO"),
            SMSG_TIME_SYNC_REQ => Some("SMSG_TIME_SYNC_REQ"),
            _ => None,
        }
    }

    /// Returns the decrypted packet body exactly as received.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Decodes an active-world transfer or returns `None` for another opcode.
    ///
    /// Initial character login consumes verify-world before reaching this
    /// boundary; an active session gives that opcode its same-map guard.
    ///
    /// # Errors
    ///
    /// Returns [`WorldTransferPacketError`] when the wire fields are malformed.
    pub fn world_transfer(&self) -> Result<Option<WorldTransfer>, WorldTransferPacketError> {
        WorldTransfer::decode(self.opcode, &self.payload)
    }

    /// Decodes `SMSG_CHAR_ENUM`, or returns `None` for another retained opcode.
    ///
    /// # Errors
    ///
    /// Returns [`CharacterDirectoryError`] when a character packet is malformed.
    pub fn character_directory(
        &self,
    ) -> Result<Option<CharacterDirectory>, CharacterDirectoryError> {
        if self.opcode != SMSG_CHAR_ENUM {
            return Ok(None);
        }
        CharacterDirectory::decode(&self.payload).map(Some)
    }

    /// Decodes `SMSG_CHAR_CREATE`, or returns `None` for another opcode.
    ///
    /// # Errors
    ///
    /// Returns [`CharacterCreationError`] unless the response contains exactly
    /// one known build-12340 creation result.
    pub fn character_creation_result(
        &self,
    ) -> Result<Option<CharacterCreationResult>, CharacterCreationError> {
        if self.opcode != SMSG_CHAR_CREATE {
            return Ok(None);
        }
        CharacterCreationResult::decode(&self.payload).map(Some)
    }

    /// Decodes `SMSG_CHAR_DELETE`, or returns `None` for another opcode.
    ///
    /// # Errors
    ///
    /// Returns [`CharacterDeletionError`] unless the response contains exactly
    /// one known build-12340 deletion result.
    pub fn character_deletion_result(
        &self,
    ) -> Result<Option<CharacterDeletionResult>, CharacterDeletionError> {
        if self.opcode != SMSG_CHAR_DELETE {
            return Ok(None);
        }
        CharacterDeletionResult::decode(&self.payload).map(Some)
    }

    /// Decodes `SMSG_CHAR_RENAME`, or returns `None` for another opcode.
    ///
    /// # Errors
    ///
    /// Returns [`CharacterRenameError`] when the result byte or success-only
    /// GUID and C string do not have the exact build-12340 representation.
    pub fn character_rename_result(
        &self,
    ) -> Result<Option<CharacterRenameResult>, CharacterRenameError> {
        if self.opcode != SMSG_CHAR_RENAME {
            return Ok(None);
        }
        CharacterRenameResult::decode(&self.payload).map(Some)
    }

    /// Decodes positional `SMSG_ADDON_INFO` policy against the sent manifest.
    ///
    /// Returns `None` for another retained opcode. The manifest must be the
    /// exact ordered manifest used to authenticate this world session.
    ///
    /// # Errors
    ///
    /// Returns [`AddonPolicyError`] when an add-on policy packet is malformed.
    pub fn addon_policy(
        &self,
        manifest: &WorldAddonManifest,
    ) -> Result<Option<WorldAddonPolicy>, AddonPolicyError> {
        if self.opcode != SMSG_ADDON_INFO {
            return Ok(None);
        }
        WorldAddonPolicy::decode(&self.payload, manifest).map(Some)
    }

    /// Decodes `SMSG_LOGIN_VERIFY_WORLD`, or returns `None` for another opcode.
    ///
    /// # Errors
    ///
    /// Returns [`WorldEntryPacketError`] when the location body is not exactly
    /// the build-12340 20-byte representation.
    pub fn world_location(&self) -> Result<Option<WorldLocation>, WorldEntryPacketError> {
        if self.opcode != SMSG_LOGIN_VERIFY_WORLD {
            return Ok(None);
        }
        WorldLocation::decode(&self.payload).map(Some)
    }

    /// Decodes `SMSG_CHARACTER_LOGIN_FAILED`, or returns `None` for another opcode.
    ///
    /// # Errors
    ///
    /// Returns [`WorldEntryPacketError`] unless the reason body is exactly one byte.
    pub fn character_login_rejection(
        &self,
    ) -> Result<Option<CharacterLoginRejection>, WorldEntryPacketError> {
        if self.opcode != SMSG_CHARACTER_LOGIN_FAILED {
            return Ok(None);
        }
        CharacterLoginRejection::decode(&self.payload).map(Some)
    }

    /// Decodes `SMSG_LOGIN_SETTIMESPEED`, or returns `None` for another opcode.
    ///
    /// # Errors
    ///
    /// Returns [`WorldTimePacketError`] unless the body is the exact valid
    /// build-12340 clock, advancement rate, and holiday-offset representation.
    pub fn world_time_speed(&self) -> Result<Option<WorldTimeSpeed>, WorldTimePacketError> {
        if self.opcode != SMSG_LOGIN_SETTIMESPEED {
            return Ok(None);
        }
        WorldTimeSpeed::decode(&self.payload).map(Some)
    }

    /// Decodes `SMSG_ACTION_BUTTONS`, or returns `None` for another opcode.
    ///
    /// # Errors
    ///
    /// Returns [`WorldActionButtonPacketError`] unless the packet is an exact
    /// 144-slot image or the stock one-byte clear representation.
    pub fn action_buttons(
        &self,
    ) -> Result<Option<WorldActionButtons>, WorldActionButtonPacketError> {
        if self.opcode != SMSG_ACTION_BUTTONS {
            return Ok(None);
        }
        WorldActionButtons::decode(&self.payload).map(Some)
    }

    /// Decodes normal or zlib-compressed object updates.
    ///
    /// Returns `None` for another retained opcode.
    ///
    /// # Errors
    ///
    /// Returns [`ObjectUpdateError`] when framing, compression, movement, or
    /// update-mask data is malformed or exceeds the explicit packet bound.
    pub fn object_updates(&self) -> Result<Option<WorldObjectUpdateBatch>, ObjectUpdateError> {
        if !matches!(self.opcode, 0x00A9 | 0x01F6) {
            return Ok(None);
        }
        WorldObjectUpdateBatch::decode(self.opcode, &self.payload).map(Some)
    }

    /// Decodes the echoed ping sequence from `SMSG_PONG`.
    ///
    /// # Errors
    ///
    /// Returns [`WorldLivenessPacketError`] unless the body is exactly four bytes.
    pub fn pong_sequence(&self) -> Result<Option<u32>, WorldLivenessPacketError> {
        if self.opcode != SMSG_PONG {
            return Ok(None);
        }
        super::liveness::decode_u32(&self.payload, "SMSG_PONG").map(Some)
    }

    /// Decodes the counter from `SMSG_TIME_SYNC_REQ`.
    ///
    /// # Errors
    ///
    /// Returns [`WorldLivenessPacketError`] unless the body is exactly four bytes.
    pub fn time_sync_counter(&self) -> Result<Option<u32>, WorldLivenessPacketError> {
        if self.opcode != SMSG_TIME_SYNC_REQ {
            return Ok(None);
        }
        super::liveness::decode_u32(&self.payload, "SMSG_TIME_SYNC_REQ").map(Some)
    }
}

impl std::fmt::Debug for WorldServerPacket {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WorldServerPacket")
            .field("opcode", &format_args!("{:#06X}", self.opcode))
            .field("name", &self.name())
            .field("payload_bytes", &self.payload.len())
            .finish_non_exhaustive()
    }
}
