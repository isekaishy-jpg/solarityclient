//! Mounted archive lookup without an eager full-file index.

use crate::archive::{ArchiveDescriptor, AssetError, AssetPath, Locale, MountedArchive};
use crate::file_stack::ArchiveCatalog;

/// Bytes returned with the exact archive selected by stock precedence.
#[derive(Clone, Debug)]
pub struct AssetRead {
    bytes: Vec<u8>,
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
    pub fn into_bytes(self) -> Vec<u8> {
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
/// The stack performs at most one MPQ hash lookup per mounted archive. This
/// avoids both a coarse synchronization primitive and an eager index containing
/// millions of filenames. Runtime loading can later give this owner a dedicated
/// asset thread without changing the public archive model.
pub struct AssetStore {
    locale: Locale,
    archives: Vec<MountedArchive>,
}

impl AssetStore {
    /// Opens every discovered archive in resolution order.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::ArchiveOpen`] when any present stock archive is
    /// corrupt or unsupported.
    pub fn mount(catalog: ArchiveCatalog) -> Result<Self, AssetError> {
        let locale = catalog.locale();
        let descriptors = catalog.into_descriptors();
        let mut archives = Vec::with_capacity(descriptors.len());
        for descriptor in descriptors {
            archives.push(MountedArchive::open(descriptor)?);
        }
        Ok(Self { locale, archives })
    }

    /// Returns the locale whose archive set and localized tables are mounted.
    #[must_use]
    pub const fn locale(&self) -> Locale {
        self.locale
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
        for archive in &mut self.archives {
            if !archive.contains(path)? {
                continue;
            }

            let bytes = archive.read(path)?;
            return Ok(AssetRead {
                bytes,
                source: archive.descriptor().clone(),
            });
        }

        Err(AssetError::AssetNotFound { path: path.clone() })
    }
}
