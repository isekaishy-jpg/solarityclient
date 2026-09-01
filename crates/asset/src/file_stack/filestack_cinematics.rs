//! Explicit locale-loose resolution for stock AVI cinematics.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

const CINEMATIC_ASSET_PREFIX: &str = "INTERFACE\\CINEMATICS\\";

impl AssetStore {
    /// Resolves one stock extensionless movie identity to its locale-loose AVI.
    ///
    /// Build 12340 receives names such as `Interface\\Cinematics\\Logo_1024`
    /// from `MovieFrame.lua`, appends `.avi`, and opens them below the selected
    /// `Data\\<locale>` tree. Cinematics are not an arbitrary loose-file
    /// fallback for the MPQ namespace.
    ///
    /// # Errors
    ///
    /// Returns an asset-path error outside the cinematic namespace, a scoped
    /// filesystem error while walking the locale tree, or `AssetNotFound` when
    /// the exact locale AVI is absent.
    pub fn cinematic_file_path(&self, movie: &AssetPath) -> Result<PathBuf, AssetError> {
        let relative = cinematic_relative_path(movie)?;
        let archive_identity = cinematic_archive_identity(movie)?;
        let locale_root = self.data_root.as_path().join(self.locale().as_str());
        find_relative_path(&locale_root, &relative)?.ok_or(AssetError::AssetNotFound {
            path: archive_identity,
        })
    }
}

/// Converts the extensionless script identity into a locale-root relative AVI.
fn cinematic_relative_path(movie: &AssetPath) -> Result<PathBuf, AssetError> {
    let path = movie.as_str();
    let Some(_suffix) = path.strip_prefix(CINEMATIC_ASSET_PREFIX) else {
        return Err(AssetError::InvalidAssetPath {
            path: path.to_owned(),
            violation: crate::archive::AssetPathViolation::NotArchiveRelative,
        });
    };
    if path.ends_with(".AVI") {
        return Err(AssetError::InvalidAssetPath {
            path: path.to_owned(),
            violation: crate::archive::AssetPathViolation::InvalidComponent,
        });
    }
    Ok(PathBuf::from(format!("{}.avi", path.replace('\\', "/"))))
}

fn cinematic_archive_identity(movie: &AssetPath) -> Result<AssetPath, AssetError> {
    // Appending an ASCII extension to a previously validated path preserves
    // every AssetPath invariant.
    AssetPath::new(format!("{}.AVI", movie.as_str()))
}

/// Resolves every component with Win32 case folding without broadening scope.
fn find_relative_path(root: &Path, relative: &Path) -> Result<Option<PathBuf>, AssetError> {
    let mut resolved = root.to_path_buf();
    for component in relative.components() {
        let expected = component.as_os_str().to_string_lossy();
        let entries = match fs::read_dir(&resolved) {
            Ok(entries) => entries,
            Err(source) if source.kind() == ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(AssetError::CinematicLookup {
                    path: resolved,
                    message: source.to_string(),
                });
            }
        };
        let mut matched = None;
        for entry in entries {
            let entry = entry.map_err(|source| AssetError::CinematicLookup {
                path: resolved.clone(),
                message: source.to_string(),
            })?;
            if entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(&expected)
            {
                matched = Some(entry.path());
                break;
            }
        }
        let Some(path) = matched else {
            return Ok(None);
        };
        resolved = path;
    }
    Ok(resolved.is_file().then_some(resolved))
}
