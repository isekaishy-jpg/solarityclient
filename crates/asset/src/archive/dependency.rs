//! Private `wow-mpq` adapter used by the stock archive stack.

use wow_mpq::{Archive, Error as MpqError};

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

    /// Admits declared input storage before reading a selected entry when budgeted.
    pub(crate) fn read_if_present(
        &mut self,
        path: &AssetPath,
        budget: Option<&crate::AssetReadBudget>,
    ) -> Result<Option<crate::AssetBytes>, AssetError> {
        let charge = if let Some(budget) = budget {
            let Some(entry) = self.archive.find_file(path.as_str()).map_err(|source| {
                AssetError::ArchiveRead {
                    archive: self.descriptor.path().to_path_buf(),
                    asset: path.clone(),
                    message: source.to_string(),
                }
            })?
            else {
                return Ok(None);
            };
            // The backend can return the stored buffer directly for raw entries.
            // Preserve path-based reads: index-only reads use synthetic names and
            // would change the encryption key. This admission adds a metadata probe.
            Some(
                budget
                    .reserve(entry.file_size.max(entry.compressed_size))
                    .map_err(|source| AssetError::ReadAdmission {
                        asset: path.clone(),
                        source,
                    })?,
            )
        } else {
            None
        };
        match self.archive.read_file(path.as_str()) {
            Ok(bytes) => Ok(Some(match charge {
                Some(charge) => crate::AssetBytes::admitted(bytes, charge).map_err(|source| {
                    AssetError::ReadAdmission {
                        asset: path.clone(),
                        source,
                    }
                })?,
                None => crate::AssetBytes::unmetered(bytes),
            })),
            Err(MpqError::FileNotFound(_)) => Ok(None),
            Err(source) => Err(AssetError::ArchiveRead {
                archive: self.descriptor.path().to_path_buf(),
                asset: path.clone(),
                message: source.to_string(),
            }),
        }
    }
}
