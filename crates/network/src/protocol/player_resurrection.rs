//! Build-12340 resurrection offer and corpse recovery deadline packets.

use thiserror::Error;

/// Server-owned resurrection state, independent of the release timer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorldPlayerResurrection {
    /// 6DBD00 reads a full GUID, byte-counted name region and two flags.
    Offer {
        /// Source that must be echoed in CMSG_RESURRECT_RESPONSE.
        guid: u64,
        /// Empty names require the source's name-cache lookup.
        name: String,
        /// Native flag is an unsigned byte, tested for nonzero.
        sickness: u8,
        /// Native flag is an unsigned byte, tested for nonzero.
        timer: u8,
    },
    /// 5267D8 supplies milliseconds to 513A80.
    RecoveryDelay(u32),
}

/// A resurrection packet has a truncated field or invalid name.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("malformed player resurrection packet")]
pub struct WorldPlayerResurrectionPacketError;

impl WorldPlayerResurrection {
    pub(super) fn decode(
        opcode: u16,
        body: &[u8],
    ) -> Result<Option<Self>, WorldPlayerResurrectionPacketError> {
        let err = WorldPlayerResurrectionPacketError;
        match opcode {
            0x269 => Ok(Some(Self::RecoveryDelay(u32::from_le_bytes(
                body.get(..4).ok_or(err)?.try_into().map_err(|_| err)?,
            )))),
            0x15b => {
                let guid =
                    u64::from_le_bytes(body.get(..8).ok_or(err)?.try_into().map_err(|_| err)?);
                let length =
                    u32::from_le_bytes(body.get(8..12).ok_or(err)?.try_into().map_err(|_| err)?)
                        as usize;
                let flags = 12_usize.checked_add(length).ok_or(err)?;
                let sickness = *body.get(flags).ok_or(err)?;
                let timer = *body.get(flags + 1).ok_or(err)?;
                // 47B6B0 advances by the authored byte count, while the consumer
                // reads a C string at its start (including a zero-length region).
                let tail = body.get(12..).ok_or(err)?;
                let end = tail.iter().position(|&b| b == 0).ok_or(err)?;
                let name = std::str::from_utf8(&tail[..end])
                    .map_err(|_| err)?
                    .to_owned();
                Ok(Some(Self::Offer {
                    guid,
                    name,
                    sickness,
                    timer,
                }))
            }
            _ => Ok(None),
        }
    }
}
