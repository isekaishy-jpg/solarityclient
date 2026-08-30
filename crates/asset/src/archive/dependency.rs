//! Private `wow-mpq` adapter used by the stock archive stack.

use wow_mpq::Archive;

use crate::archive::{ArchiveDescriptor, AssetError, AssetPath};

/// An opened archive whose dependency type never crosses the crate facade.
pub(crate) struct MountedArchive {
    descriptor: ArchiveDescriptor,
    archive: Archive,
}

impl MountedArchive {
    /// Opens one discovered archive and attaches its stable public metadata.
    pub(crate) fn open(descriptor: ArchiveDescriptor) -> Result<Self, AssetError> {
        let archive =
            Archive::open(descriptor.path()).map_err(|source| AssetError::ArchiveOpen {
                path: descriptor.path().to_path_buf(),
                message: source.to_string(),
            })?;

        Ok(Self {
            descriptor,
            archive,
        })
    }

    /// Returns metadata used for precedence reporting.
    pub(crate) fn descriptor(&self) -> &ArchiveDescriptor {
        &self.descriptor
    }

    /// Probes the MPQ hash table without enumerating or eagerly indexing files.
    pub(crate) fn contains(&self, path: &AssetPath) -> Result<bool, AssetError> {
        self.archive
            .find_file(path.as_str())
            .map(|entry| entry.is_some())
            .map_err(|source| AssetError::ArchiveLookup {
                archive: self.descriptor.path().to_path_buf(),
                asset: path.clone(),
                message: source.to_string(),
            })
    }

    /// Reads a previously selected file entry from this archive.
    pub(crate) fn read(&mut self, path: &AssetPath) -> Result<Vec<u8>, AssetError> {
        self.archive
            .read_file(path.as_str())
            .map_err(|source| AssetError::ArchiveRead {
                archive: self.descriptor.path().to_path_buf(),
                asset: path.clone(),
                message: source.to_string(),
            })
    }
}
