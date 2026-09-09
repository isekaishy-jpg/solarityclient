//! Logout messages consumed by build 12340's 6B08B0, 6B0900 and 6B2180.

use super::WorldServerPacket;
use crate::session::{WorldSessionError, WorldSessionStage};

/// Server-owned logout progress; completion preserves the authenticated socket.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldLogout {
    /// Response reason zero admits logout; a nonzero instant byte suppresses camping.
    Response {
        /// Zero accepts the request; every nonzero value uses ERR_LOGOUT_FAILED.
        reason: u32,
        /// Any nonzero native byte suppresses the countdown event.
        instant: bool,
    },
    /// The player has left the world and character selection may resume.
    Complete,
    /// Cancellation acknowledgment, interpreted using the native pending flag.
    CancelAcknowledged,
}

/// Empty client packet selected by the native logout and cancellation APIs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldLogoutRequest {
    /// 6B1930 normal request.
    Request,
    /// 6B18C0 cancellation request.
    Cancel,
    /// 6B21F0 bypasses the local pending guard.
    Force,
}

impl WorldServerPacket {
    /// Decodes the fields read by the three native logout callbacks.
    ///
    /// # Errors
    /// Returns a decode error when the response lacks its reason or instant byte.
    pub fn logout(&self) -> Result<Option<WorldLogout>, WorldSessionError> {
        Ok(match self.opcode() {
            0x4c => {
                let [a, b, c, d, instant, ..] = self.payload() else {
                    return Err(WorldSessionError::Decode {
                        stage: WorldSessionStage::Receive,
                        message: "SMSG_LOGOUT_RESPONSE requires five bytes".to_owned(),
                    });
                };
                Some(WorldLogout::Response {
                    reason: u32::from_le_bytes([*a, *b, *c, *d]),
                    instant: *instant != 0,
                })
            }
            0x4d => Some(WorldLogout::Complete),
            0x4f => Some(WorldLogout::CancelAcknowledged),
            _ => None,
        })
    }
}
