//! Visual C++ 2005 `rand` behavior used directly by build 12340.

/// One thread's Visual C++ `_ptiddata._holdrand` state.
///
/// The stock executable links `_rand` at `0x0088B867`. Its per-thread-data
/// initializer writes `1` to `_holdrand`, and no `srand` implementation is
/// linked. Keeping this owner at the client-thread composition root allows
/// every future stock `rand` consumer to advance the same stream in call order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CrtRand {
    state: u32,
}

impl CrtRand {
    /// Creates the default CRT state installed for a newly initialized thread.
    #[must_use]
    pub const fn new() -> Self {
        Self { state: 1 }
    }

    /// Advances the exact Visual C++ recurrence and returns its 15-bit result.
    #[must_use]
    pub const fn next_u15(&mut self) -> u16 {
        self.state = self.state.wrapping_mul(214_013).wrapping_add(2_531_011);
        ((self.state >> 16) & 0x7fff) as u16
    }
}

impl Default for CrtRand {
    fn default() -> Self {
        Self::new()
    }
}
