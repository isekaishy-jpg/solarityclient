//! Native client-control and local stand-state packet bodies.

use thiserror::Error;

/// A unit's server-selected client control bit (`SMSG_CLIENT_CONTROL_UPDATE`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldClientControlUpdate {
    /// Unit GUID decoded from the packet's packed representation.
    pub guid: u64,
    /// The native byte is interpreted as nonzero, rather than restricted to one.
    pub enabled: bool,
}

/// A player-control packet does not contain exactly its declared native fields.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("malformed {packet} body: expected {expected} bytes, found {actual}")]
pub struct WorldPlayerControlPacketError {
    packet: &'static str,
    expected: usize,
    actual: usize,
}

impl WorldClientControlUpdate {
    pub(super) fn decode(payload: &[u8]) -> Result<Self, WorldPlayerControlPacketError> {
        let Some(&mask) = payload.first() else {
            return Err(WorldPlayerControlPacketError {
                packet: "SMSG_CLIENT_CONTROL_UPDATE",
                expected: 2,
                actual: 0,
            });
        };
        let expected = 2 + mask.count_ones() as usize;
        if payload.len() != expected {
            return Err(WorldPlayerControlPacketError {
                packet: "SMSG_CLIENT_CONTROL_UPDATE",
                expected,
                actual: payload.len(),
            });
        }
        let mut bytes = [0; 8];
        let mut offset = 1;
        for (index, byte) in bytes.iter_mut().enumerate() {
            if mask & (1 << index) != 0 {
                *byte = payload[offset];
                offset += 1;
            }
        }
        Ok(Self {
            guid: u64::from_le_bytes(bytes),
            enabled: payload[offset] != 0,
        })
    }
}

pub(super) fn decode_stand_state(payload: &[u8]) -> Result<u8, WorldPlayerControlPacketError> {
    match payload {
        [state] => Ok(*state),
        _ => Err(WorldPlayerControlPacketError {
            packet: "SMSG_STANDSTATE_UPDATE",
            expected: 1,
            actual: payload.len(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_control_preserves_sparse_and_noncanonical_native_values()
    -> Result<(), WorldPlayerControlPacketError> {
        assert_eq!(
            WorldClientControlUpdate::decode(&[0xcc, 0x78, 0x56, 0x34, 0x12, 255])?,
            WorldClientControlUpdate {
                guid: 0x1234_0000_5678_0000,
                enabled: true
            }
        );
        assert_eq!(
            WorldClientControlUpdate::decode(&[0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0])?,
            WorldClientControlUpdate {
                guid: 0,
                enabled: false
            }
        );
        for body in [vec![], vec![0], vec![1, 8], vec![0, 1, 0], vec![0xff, 1]] {
            assert!(WorldClientControlUpdate::decode(&body).is_err(), "{body:?}");
        }
        for state in 0..=255 {
            assert_eq!(decode_stand_state(&[state])?, state);
        }
        assert!(decode_stand_state(&[]).is_err());
        assert!(decode_stand_state(&[1, 0]).is_err());
        Ok(())
    }
}
