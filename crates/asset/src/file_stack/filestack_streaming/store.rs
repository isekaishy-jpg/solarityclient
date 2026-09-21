//! Mounted archive lookup without an eager full-file index.

use crate::archive::{ArchiveDescriptor, AssetError, AssetPath, Locale, MountedArchive};

/// Bytes returned with the exact archive selected by stock precedence.
#[derive(Debug)]
pub struct AssetRead {
    bytes: crate::AssetBytes,
    source: ArchiveDescriptor,
}

impl AssetRead {
    /// Returns the decoded archive-entry bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Transfers ownership of the decoded bytes.
    #[must_use]
    pub fn into_bytes(self) -> crate::AssetBytes {
        self.bytes
    }

    /// Returns the archive selected by stock precedence.
    #[must_use]
    pub fn source(&self) -> &ArchiveDescriptor {
        &self.source
    }
}

/// The owner of mounted client archives and serialized archive read handles.
///
/// Lookup probes the mounted MPQs without an eager filename index or a shared
/// reader lock. Admitted reads inspect entry size before allocation, then preserve
/// the backend's path-based read and encryption-key selection.
pub struct AssetStore {
    pub(in crate::file_stack) read_budget: Option<crate::AssetReadBudget>,
    pub(super) identity: u64,
    pub(super) model_cache_service: crate::M2CacheService,
    pub(super) world_model_cache_service: crate::WmoCacheService,
    pub(super) namespace: crate::file_stack::AssetNamespaceId,
    pub(in crate::file_stack) data_root: crate::archive::ClientDataRoot,
    pub(super) locale: Locale,
    pub(super) existing_locales: Vec<Locale>,
    pub(super) archives: Vec<MountedArchive>,
}

impl AssetStore {
    /// Applies input admission only during this operation. Returned byte owners
    /// retain their own charges after the operation or the mounted reader ends.
    /// Nested operations restore the previous policy on success, error and unwind.
    pub fn with_read_budget<T>(
        &mut self,
        budget: &crate::AssetReadBudget,
        operation: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let previous = self.read_budget.replace(budget.clone());
        let scope = ReadScope {
            store: self,
            previous,
        };
        operation(&mut *scope.store)
    }
    /// A scoped demand class overrides the namespace's required-read default.
    /// Offline catalogs without configured storage remain explicitly unmetered.
    pub(crate) fn effective_read_budget(&self) -> Option<crate::AssetReadBudget> {
        self.read_budget.clone().or_else(|| {
            self.model_cache_service.storage().map(|storage| {
                crate::AssetReadBudget::for_service(
                    storage.clone(),
                    solarity_cpu::CpuService::Required,
                )
            })
        })
    }

    /// Joins maintenance without sharing mutable archive-reader state.
    pub fn model_cache_service(&self) -> &crate::M2CacheService {
        &self.model_cache_service
    }
    /// Borrow the namespace's shared root/group producer authority.
    #[must_use]
    pub fn world_model_cache_service(&self) -> &crate::WmoCacheService {
        &self.world_model_cache_service
    }

    /// Identifies this immutable mounted provider lifetime for retained caches.
    /// Remounting the same paths creates a new identity, including changed files.
    #[must_use]
    pub const fn identity(&self) -> u64 {
        self.identity
    }

    /// Identifies the immutable selection shared by mounts from the same catalog.
    #[must_use]
    pub const fn namespace(&self) -> crate::file_stack::AssetNamespaceId {
        self.namespace
    }

    /// Returns the locale whose archive set and localized tables are mounted.
    #[must_use]
    pub const fn locale(&self) -> Locale {
        self.locale
    }

    /// Returns every language pack found beside the mounted locale.
    #[must_use]
    pub fn existing_locales(&self) -> &[Locale] {
        &self.existing_locales
    }

    /// Returns mounted archive metadata in resolution order.
    pub fn archives(&self) -> impl ExactSizeIterator<Item = &ArchiveDescriptor> {
        self.archives.iter().map(MountedArchive::descriptor)
    }

    /// Reports whether the stock archive stack contains an exact virtual path.
    ///
    /// This performs the same ordered MPQ hash probes as [`Self::read`] without
    /// allocating or decompressing the selected entry.
    ///
    /// # Errors
    ///
    /// Returns an archive lookup error when a mounted MPQ cannot be queried.
    pub fn contains(&self, path: &AssetPath) -> Result<bool, AssetError> {
        for archive in &self.archives {
            if archive.contains(path)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Resolves and reads an arbitrary file through the complete stock stack.
    ///
    /// # Errors
    ///
    /// Returns a lookup or read error from the selected archive, or
    /// [`AssetError::AssetNotFound`] when no mounted archive contains the path.
    pub fn read(&mut self, path: &AssetPath) -> Result<AssetRead, AssetError> {
        let _profile_scope =
            solarity_profiling::profile!("asset.file_stack.filestack_streaming.read");
        let budget = self.effective_read_budget();
        for archive in &mut self.archives {
            let Some(bytes) = archive.read_if_present(path, budget.as_ref())? else {
                continue;
            };

            return Ok(AssetRead {
                bytes,
                source: archive.descriptor().clone(),
            });
        }

        Err(AssetError::AssetNotFound { path: path.clone() })
    }
}

/// A decoder unwind cannot leak a caller's byte policy into later operations.
struct ReadScope<'a> {
    store: &'a mut AssetStore,
    previous: Option<crate::AssetReadBudget>,
}

impl Drop for ReadScope<'_> {
    fn drop(&mut self) {
        self.store.read_budget = self.previous.take();
    }
}
