//! Archive opening yields only between complete dependency-owned MPQ handles.

use std::{ops::ControlFlow, vec::IntoIter};

use super::AssetStore;
use crate::{ArchiveCatalog, ArchiveDescriptor, AssetError, archive::MountedArchive};

/// Owns a partially opened stack without exposing it for resource lookup.
/// Each advance opens at most one archive. The dependency's single archive open
/// remains indivisible; callers choose where to schedule the next advance.
pub struct AssetMount {
    store: AssetStore,
    remaining: IntoIter<ArchiveDescriptor>,
}

impl AssetMount {
    /// Opens the next archive in exact resolution order and transfers the stack
    /// only after every descriptor succeeds. Consuming the continuation prevents
    /// double publication; cancellation releases all already-opened handles.
    ///
    /// # Errors
    /// Returns the first archive-open error, exactly as [`AssetStore::mount`].
    pub fn advance(mut self) -> Result<ControlFlow<AssetStore, Self>, AssetError> {
        let _profile = solarity_profiling::profile!("asset.archive.mount_step");
        if let Some(descriptor) = self.remaining.next() {
            self.store.archives.push(MountedArchive::open(descriptor)?);
        }
        if self.remaining.len() == 0 {
            Ok(ControlFlow::Break(self.store))
        } else {
            Ok(ControlFlow::Continue(self))
        }
    }
}

impl AssetStore {
    /// Reserves metadata and a mounted-handle identity without opening files.
    /// The immutable namespace, precedence and locale come from this catalog.
    ///
    /// # Errors
    /// Returns [`AssetError::IdentityExhausted`] if no distinct mounted-handle
    /// identity can be issued.
    pub fn begin_mount(catalog: ArchiveCatalog) -> Result<AssetMount, AssetError> {
        let namespace = catalog.namespace();
        let model_cache_service = catalog.model_cache_service();
        let identity = crate::file_stack::namespace::next_identity()?;
        let data_root = catalog.data_root().clone();
        let locale = catalog.locale();
        let existing_locales = catalog.existing_locales().to_vec();
        let descriptors = catalog.into_descriptors();
        let archives = Vec::with_capacity(descriptors.len());
        Ok(AssetMount {
            store: Self {
                identity,
                namespace,
                model_cache_service,
                data_root,
                locale,
                existing_locales,
                archives,
            },
            remaining: descriptors.into_iter(),
        })
    }

    /// Opens every discovered archive in resolution order on the caller.
    /// Worker services can use [`Self::begin_mount`] to yield between archives.
    ///
    /// # Errors
    /// Returns [`AssetError::ArchiveOpen`] when any present stock archive is
    /// corrupt or unsupported, or [`AssetError::IdentityExhausted`] if no distinct
    /// mounted-handle identity can be issued.
    pub fn mount(catalog: ArchiveCatalog) -> Result<Self, AssetError> {
        let mut pending = Self::begin_mount(catalog)?;
        loop {
            match pending.advance()? {
                ControlFlow::Continue(next) => pending = next,
                ControlFlow::Break(store) => return Ok(store),
            }
        }
    }
}
