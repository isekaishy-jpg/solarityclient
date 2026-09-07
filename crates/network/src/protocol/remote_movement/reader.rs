//! Checked wire reads shared by the incoming unit movement packet family.

use thiserror::Error;

/// Invalid server movement packet with its exact failing byte offset.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("movement packet at byte {offset}: {message}")]
pub struct MovementPacketError {
    /// Byte offset relative to the decrypted packet body.
    pub offset: usize,
    /// Stable field/bounds explanation.
    pub message: &'static str,
}

/// A cursor that bounds all count-dependent reads before allocation.
pub(super) struct MovementReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> MovementReader<'a> {
    pub(super) const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }
    pub(super) const fn error(&self, message: &'static str) -> MovementPacketError {
        MovementPacketError {
            offset: self.offset,
            message,
        }
    }
    pub(super) fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }

    /// Copies one fixed-size wire scalar after checking its full span.
    fn bytes<const N: usize>(&mut self) -> Result<[u8; N], MovementPacketError> {
        let bytes = self
            .bytes
            .get(self.offset..)
            .and_then(|rest| rest.get(..N))
            .ok_or_else(|| self.error("truncated field"))?;
        let mut value = [0; N];
        value.copy_from_slice(bytes);
        self.offset += N;
        Ok(value)
    }
    pub(super) fn byte(&mut self) -> Result<u8, MovementPacketError> {
        Ok(self.bytes::<1>()?[0])
    }
    pub(super) fn word(&mut self) -> Result<u32, MovementPacketError> {
        Ok(u32::from_le_bytes(self.bytes()?))
    }
    pub(super) fn short(&mut self) -> Result<u16, MovementPacketError> {
        Ok(u16::from_le_bytes(self.bytes()?))
    }
    pub(super) fn float(&mut self) -> Result<f32, MovementPacketError> {
        Ok(f32::from_bits(self.word()?))
    }
    pub(super) fn guid(&mut self) -> Result<u64, MovementPacketError> {
        Ok(u64::from_le_bytes(self.bytes()?))
    }
    pub(super) fn point(&mut self) -> Result<[f32; 3], MovementPacketError> {
        Ok([self.float()?, self.float()?, self.float()?])
    }

    /// Native `0076DC20` GUID mask, without assuming dense GUID bytes.
    pub(super) fn packed_guid(&mut self) -> Result<u64, MovementPacketError> {
        let mask = self.byte()?;
        let mut guid = 0;
        for index in 0..8 {
            if mask & (1 << index) != 0 {
                guid |= u64::from(self.byte()?) << (index * 8);
            }
        }
        Ok(guid)
    }

    /// Rejects trailing data at a complete packet boundary.
    pub(super) fn finish(self) -> Result<(), MovementPacketError> {
        if self.remaining() == 0 {
            Ok(())
        } else {
            Err(self.error("trailing data"))
        }
    }
}
