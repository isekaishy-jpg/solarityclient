//! Explicit loose/MPQ resolution for stock `Interface\\AddOns` content.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

const ADDON_ASSET_PREFIX: &str = "INTERFACE\\ADDONS\\";

impl AssetStore {
    /// Enumerates direct stock AddOn directories with Windows case folding.
    ///
    /// An absent `Interface\\AddOns` directory represents an install with no
    /// discoverable modules. Case-only duplicate names are rejected because
    /// the stock Windows namespace cannot address them independently.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::AddonEnumeration`] when the install directory
    /// cannot be inspected or contains a non-Unicode or ambiguous identity.
    pub fn addon_names(&self) -> Result<Vec<String>, AssetError> {
        let install_root = install_root(&self.data_root)?;
        let relative = Path::new("Interface").join("AddOns");
        let Some(addon_root) = find_relative_path(install_root, &relative)? else {
            return Ok(Vec::new());
        };
        let entries = fs::read_dir(&addon_root).map_err(|source| AssetError::AddonEnumeration {
            path: addon_root.clone(),
            message: source.to_string(),
        })?;
        let mut names = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| AssetError::AddonEnumeration {
                path: addon_root.clone(),
                message: source.to_string(),
            })?;
            let file_type = entry
                .file_type()
                .map_err(|source| AssetError::AddonEnumeration {
                    path: entry.path(),
                    message: source.to_string(),
                })?;
            if !file_type.is_dir() {
                continue;
            }
            let name =
                entry
                    .file_name()
                    .into_string()
                    .map_err(|name| AssetError::AddonEnumeration {
                        path: entry.path(),
                        message: format!("AddOn directory name {name:?} is not Unicode"),
                    })?;
            names.push(name);
        }
        names.sort_by_key(|name| name.to_ascii_lowercase());
        if let Some(duplicate) = names
            .windows(2)
            .find(|pair| pair[0].eq_ignore_ascii_case(&pair[1]))
        {
            return Err(AssetError::AddonEnumeration {
                path: addon_root,
                message: format!(
                    "case-only duplicate AddOn directories {:?} and {:?}",
                    duplicate[0], duplicate[1]
                ),
            });
        }
        Ok(names)
    }

    /// Reports whether the explicit stock AddOn stack contains one path.
    ///
    /// Loose install content takes precedence only inside
    /// `Interface\\AddOns`; the mounted MPQs remain the second source.
    ///
    /// # Errors
    ///
    /// Returns an asset error for a path outside the AddOn namespace or when
    /// filesystem/archive lookup fails.
    pub fn contains_addon_file(&self, path: &AssetPath) -> Result<bool, AssetError> {
        let relative = addon_relative_path(path)?;
        let install_root = install_root(&self.data_root)?;
        if find_relative_path(install_root, &relative)?.is_some() {
            return Ok(true);
        }
        self.contains(path)
    }

    /// Resolves one stock AddOn file through loose-first then MPQ precedence.
    ///
    /// # Errors
    ///
    /// Returns an asset error for invalid namespace, unreadable loose content,
    /// archive failure, or absence from both permitted sources.
    pub fn read_addon_file(&mut self, path: &AssetPath) -> Result<Vec<u8>, AssetError> {
        let relative = addon_relative_path(path)?;
        let install_root = install_root(&self.data_root)?;
        if let Some(loose_path) = find_relative_path(install_root, &relative)? {
            return fs::read(&loose_path).map_err(|source| AssetError::AddonRead {
                path: loose_path,
                message: source.to_string(),
            });
        }
        self.read(path)
            .map(crate::file_stack::AssetRead::into_bytes)
    }
}

/// Maps a normalized AddOn asset to its install-root-relative path.
fn addon_relative_path(path: &AssetPath) -> Result<PathBuf, AssetError> {
    if path.as_str().strip_prefix(ADDON_ASSET_PREFIX).is_none() {
        return Err(AssetError::InvalidAssetPath {
            path: path.as_str().to_owned(),
            violation: crate::archive::AssetPathViolation::NotArchiveRelative,
        });
    }
    Ok(PathBuf::from(path.as_str().replace('\\', "/")))
}

/// Returns the install directory that directly contains `Data` and `Interface`.
fn install_root(data_root: &crate::archive::ClientDataRoot) -> Result<&Path, AssetError> {
    data_root
        .as_path()
        .parent()
        .ok_or_else(|| AssetError::InvalidDataRoot {
            path: data_root.as_path().to_path_buf(),
            message: "client Data directory has no install parent".to_owned(),
        })
}

/// Resolves every component case-insensitively to match Win32 lookup rules.
fn find_relative_path(root: &Path, relative: &Path) -> Result<Option<PathBuf>, AssetError> {
    let mut resolved = root.to_path_buf();
    for component in relative.components() {
        let expected = component.as_os_str().to_string_lossy();
        let entries = match fs::read_dir(&resolved) {
            Ok(entries) => entries,
            Err(source) if source.kind() == ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(AssetError::AddonEnumeration {
                    path: resolved,
                    message: source.to_string(),
                });
            }
        };
        let mut matched = None;
        for entry in entries {
            let entry = entry.map_err(|source| AssetError::AddonEnumeration {
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
    Ok(Some(resolved))
}
