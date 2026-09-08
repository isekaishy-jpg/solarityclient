//! Attack-owner wire layouts consumed by build-12340 `756800`.

use thiserror::Error;

/// An authoritative attack start or stop on one unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldUnitAttack {
    /// 143 carries two unpacked GUIDs.
    Start {
        /// Owning unit GUID.
        attacker: u64,
        /// Selected attack GUID; zero is preserved.
        target: u64,
    },
    /// 144/261 carry two packed GUIDs and one unmodified stop word.
    Stop {
        /// Owning unit GUID.
        attacker: u64,
        /// Previous target supplied for the native stop callback.
        target: u64,
        /// Native stop word, used by the target-facing path when nonzero.
        stopped: u32,
    },
}

/// An attack packet is truncated or has trailing bytes.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("malformed unit attack packet")]
pub struct WorldUnitAttackPacketError;

impl WorldUnitAttack {
    /// Returns the owning unit's full GUID.
    #[must_use]
    pub const fn attacker(self) -> u64 {
        match self {
            Self::Start { attacker, .. } | Self::Stop { attacker, .. } => attacker,
        }
    }

    /// Returns the native attack-owner value after this notification.
    #[must_use]
    pub const fn retained_target(self) -> u64 {
        match self {
            Self::Start { target, .. } => target,
            Self::Stop { .. } => 0,
        }
    }

    pub(super) fn decode(
        opcode: u16,
        body: &[u8],
    ) -> Result<Option<Self>, WorldUnitAttackPacketError> {
        if !matches!(opcode, 0x143 | 0x144 | 0x261) {
            return Ok(None);
        }
        let mut cursor = 0;
        let mut read = |count| {
            let bytes = body
                .get(cursor..cursor + count)
                .ok_or(WorldUnitAttackPacketError)?;
            cursor += count;
            Ok::<_, WorldUnitAttackPacketError>(bytes)
        };
        let value = if opcode == 0x143 {
            Self::Start {
                attacker: u64::from_le_bytes(
                    read(8)?
                        .try_into()
                        .map_err(|_| WorldUnitAttackPacketError)?,
                ),
                target: u64::from_le_bytes(
                    read(8)?
                        .try_into()
                        .map_err(|_| WorldUnitAttackPacketError)?,
                ),
            }
        } else {
            let mut packed = || {
                let mask = read(1)?[0];
                let mut guid = [0; 8];
                for (index, byte) in guid.iter_mut().enumerate() {
                    if mask & (1 << index) != 0 {
                        *byte = read(1)?[0];
                    }
                }
                Ok::<_, WorldUnitAttackPacketError>(u64::from_le_bytes(guid))
            };
            let attacker = packed()?;
            let target = packed()?;
            let stopped = u32::from_le_bytes(
                read(4)?
                    .try_into()
                    .map_err(|_| WorldUnitAttackPacketError)?,
            );
            Self::Stop {
                attacker,
                target,
                stopped,
            }
        };
        if cursor != body.len() {
            return Err(WorldUnitAttackPacketError);
        }
        Ok(Some(value))
    }
}
