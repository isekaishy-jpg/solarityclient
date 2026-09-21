//! The domain chooses an admission class; archive code only accounts byte ownership.

use solarity_cpu::{
    ByteReservation, CpuError, CpuService, CpuStorageBudget, CpuStorageClass, CpuStorageKind,
};

/// Explicit admission policy for encoded inputs read during one loading turn.
/// This covers returned source buffers, not decoded models or third-party codec scratch.
#[derive(Clone)]
pub struct AssetReadBudget {
    storage: CpuStorageBudget,
    class: CpuStorageClass,
}

impl AssetReadBudget {
    /// Binds source input ownership to the service's current demand class.
    /// Required retirement drains cannot borrow speculative admission headroom.
    #[must_use]
    pub fn for_service(storage: CpuStorageBudget, service: CpuService) -> Self {
        let class = match service {
            CpuService::Required | CpuService::Retirement => CpuStorageClass::Required,
            CpuService::Speculative => CpuStorageClass::Speculative,
        };
        Self { storage, class }
    }

    pub(crate) fn storage(&self) -> &CpuStorageBudget {
        &self.storage
    }

    pub(crate) fn class(&self) -> CpuStorageClass {
        self.class
    }

    /// Admit the declared source size before the archive decoder allocates it.
    pub(crate) fn reserve(&self, bytes: u64) -> Result<ByteReservation, CpuError> {
        let bytes = usize::try_from(bytes).map_err(|_| CpuError::StorageSizeOverflow)?;
        self.storage
            .reserve(self.class, CpuStorageKind::Result, bytes)
    }
}
