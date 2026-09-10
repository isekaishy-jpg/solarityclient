//! Build-12340 battlefield arena context (54AE40, SMSG_BATTLEFIELD_STATUS).

use thiserror::Error;

/// The battlefield-status fields consumed by the active arena-context owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldBattlefieldStatus {
    /// Native supports two queue slots and skips any other slot's entire body.
    Ignored,
    /// A zero battlefield GUID clears this queue.
    Cleared(u32),
    /// A nonzero battlefield GUID updates the queue status.
    Status {
        /// Queue slot, zero or one.
        queue: u32,
        /// Raw status; only three installs the active battlefield map.
        status: u32,
        /// Map carried by status two or three, including unknown DBC keys.
        map: Option<u32>,
    },
}

/// The packet ended inside a field required by its native status branch.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("malformed battlefield status packet")]
pub struct WorldBattlefieldStatusPacketError;

impl WorldBattlefieldStatus {
    pub(super) fn decode(
        opcode: u16,
        body: &[u8],
    ) -> Result<Option<Self>, WorldBattlefieldStatusPacketError> {
        if opcode != 0x2d4 {
            return Ok(None);
        }
        let word = |offset: usize| {
            body.get(offset..offset + 4)
                .and_then(|bytes| bytes.try_into().ok())
                .map(u32::from_le_bytes)
                .ok_or(WorldBattlefieldStatusPacketError)
        };
        let queue = word(0)?;
        if queue >= 2 {
            return Ok(Some(Self::Ignored));
        }
        if word(4)? | word(8)? == 0 {
            return Ok(Some(Self::Cleared(queue)));
        }
        // GUID, two level bytes, instance ID, rated byte, then status.
        let status = word(19)?;
        let required = match status {
            1 => 31, // Average wait and elapsed wait.
            2 => 39, // Map, instance GUID and invitation deadline.
            3 => 44, // Map, instance GUID, finish/elapsed times and team byte.
            _ => 23,
        };
        if body.len() < required {
            return Err(WorldBattlefieldStatusPacketError);
        }
        let map = if matches!(status, 2 | 3) {
            Some(word(23)?)
        } else {
            None
        };
        Ok(Some(Self::Status { queue, status, map }))
    }
}
