//! Encoded buffers retain admission while decoders borrow or own their bytes.

use solarity_cpu::{ByteReservation, CpuError};
use std::{
    fmt,
    ops::{Deref, DerefMut},
};

/// Encoded source bytes with an optional admission owned by the same allocation.
/// Slice mutation cannot grow capacity or detach its charge. Offline readers can
/// use the same representation without an executor admission policy.
pub struct AssetBytes {
    bytes: Vec<u8>,
    // Fields drop in declaration order: release the allocation before its charge.
    charge: Option<ByteReservation>,
}

impl AssetBytes {
    /// Wraps an explicitly unmetered read from the tooling/main-reader API.
    pub(crate) fn unmetered(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            charge: None,
        }
    }

    /// Reconciles the decoder's actual output capacity before publication. The
    /// declared entry size was admitted before calling the external decoder.
    pub(crate) fn admitted(bytes: Vec<u8>, mut charge: ByteReservation) -> Result<Self, CpuError> {
        if let Err(error) = charge.resize(bytes.capacity()) {
            drop(bytes);
            return Err(error);
        }
        Ok(Self {
            bytes,
            charge: Some(charge),
        })
    }

    /// Validates UTF-8 in place and keeps this allocation's admission with the text.
    /// # Errors
    /// Returns the encoding failure after releasing invalid bytes and their charge.
    pub fn into_text(self) -> Result<super::AssetText, std::str::Utf8Error> {
        let Self { bytes, charge } = self;
        match String::from_utf8(bytes) {
            Ok(text) => Ok(super::AssetText::new(text, charge)),
            Err(error) => {
                let encoding = error.utf8_error();
                drop(error);
                Err(encoding)
            }
        }
    }

    /// Borrows encoded content without changing allocation ownership.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }

    /// Capacity charged while the bytes remain alive, including after a worker returns.
    #[must_use]
    pub fn admitted_bytes(&self) -> usize {
        self.charge.as_ref().map_or(0, ByteReservation::bytes)
    }
}

impl AsRef<[u8]> for AssetBytes {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl Deref for AssetBytes {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.bytes
    }
}

impl DerefMut for AssetBytes {
    fn deref_mut(&mut self) -> &mut [u8] {
        &mut self.bytes
    }
}

impl<T: AsRef<[u8]> + ?Sized> PartialEq<T> for AssetBytes {
    fn eq(&self, other: &T) -> bool {
        self.bytes.as_slice() == other.as_ref()
    }
}

impl Eq for AssetBytes {}

impl fmt::Debug for AssetBytes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AssetBytes")
            .field("length", &self.bytes.len())
            .field("capacity", &self.bytes.capacity())
            .field("admitted_bytes", &self.admitted_bytes())
            .finish()
    }
}

impl std::borrow::Borrow<[u8]> for AssetBytes {
    fn borrow(&self) -> &[u8] {
        &self.bytes
    }
}
