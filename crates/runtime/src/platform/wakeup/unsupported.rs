//! Explicit unsupported result until an OS has an approved native wait backend.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Instant;

use super::{WakeReason, WakeTicket};
use crate::platform::PlatformError;

/// Cannot be constructed on a platform without a backend.
pub(in crate::platform) struct WakeBridge {
    unavailable: Infallible,
}

impl WakeBridge {
    /// Reports the missing backend instead of substituting polling or sleeping.
    pub(in crate::platform) fn new() -> Result<Self, PlatformError> {
        Err(PlatformError::UnsupportedCoordinatorWait)
    }
    /// No unsupported owner can pass a health check.
    pub(in crate::platform) fn check(&self) -> Result<(), PlatformError> {
        match self.unavailable {}
    }
    /// An unsupported owner cannot expose a notifier.
    pub(in crate::platform) fn notifier(&self) -> Arc<dyn solarity_cpu::CoordinatorNotifier> {
        match self.unavailable {}
    }
    /// An unsupported owner cannot observe a wake generation.
    pub(in crate::platform) fn observe(&self) -> WakeTicket {
        match self.unavailable {}
    }
    /// An unsupported owner cannot enter a native wait.
    pub(in crate::platform) fn wait(
        &mut self,
        _: WakeTicket,
        _: Instant,
    ) -> Result<WakeReason, PlatformError> {
        match self.unavailable {}
    }
}
