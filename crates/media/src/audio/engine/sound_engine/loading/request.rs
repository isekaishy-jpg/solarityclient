//! Exact sound selection and a monotonic signal for reservation ownership.

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use solarity_asset::AssetPath;

use super::super::SoundEngineError;

/// Process-unique identity for a selected sound awaiting resource admission.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SoundLoadHandle(u64);

/// Exact selected path that a worker may read without access to sound or RNG state.
#[derive(Clone, Debug)]
pub struct SoundLoadRequest {
    pub(super) handle: SoundLoadHandle,
    pub(super) path: AssetPath,
    pending: Arc<AtomicBool>,
}

/// Only the engine's pending reservation owns this signal's live interval.
/// Dropping it covers completion, cancellation, failed admission and shutdown.
pub(super) struct PendingLoadLifetime(Arc<AtomicBool>);

impl Drop for PendingLoadLifetime {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl SoundLoadRequest {
    /// Issues one non-reusable handle and its unique pending owner together.
    pub(super) fn new(path: AssetPath) -> Result<(Self, PendingLoadLifetime), SoundEngineError> {
        static NEXT_LOAD_ID: AtomicU64 = AtomicU64::new(1);
        let id = NEXT_LOAD_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| SoundEngineError::LoadCapacity)?;
        let pending = Arc::new(AtomicBool::new(true));
        let lifetime = PendingLoadLifetime(Arc::clone(&pending));
        Ok((
            Self {
                handle: SoundLoadHandle(id),
                path,
                pending,
            },
            lifetime,
        ))
    }

    /// Returns the identity used for completion and cancellation.
    #[must_use]
    pub const fn handle(&self) -> SoundLoadHandle {
        self.handle
    }

    /// Returns the one selected archive path; workers must not choose another variation.
    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Reports whether the engine still owns this exact pending reservation.
    /// Clones share one signal; checking it requires no pending-voice search or
    /// allocation. It never authorizes playback: completion still validates the
    /// handle and live engine policy at the ordered admission boundary.
    #[must_use]
    pub fn is_pending(&self) -> bool {
        self.pending.load(Ordering::Acquire)
    }
}
