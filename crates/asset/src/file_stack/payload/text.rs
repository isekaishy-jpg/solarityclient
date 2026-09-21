//! UTF-8 source ownership reuses the encoded allocation and its admission.

use solarity_cpu::ByteReservation;
use std::{fmt, ops::Deref};

/// Validated source text retaining the original byte allocation's charge.
pub struct AssetText {
    text: String,
    // Release text before its reservation, including after shared owner retirement.
    _charge: Option<ByteReservation>,
}
impl AssetText {
    pub(super) fn new(text: String, charge: Option<ByteReservation>) -> Self {
        Self {
            text,
            _charge: charge,
        }
    }
    /// Borrows the validated source without another UTF-8 scan.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }
}
impl Deref for AssetText {
    type Target = str;
    fn deref(&self) -> &str {
        &self.text
    }
}
impl PartialEq for AssetText {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
    }
}
impl Eq for AssetText {}
impl fmt::Debug for AssetText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.text.fmt(formatter)
    }
}
