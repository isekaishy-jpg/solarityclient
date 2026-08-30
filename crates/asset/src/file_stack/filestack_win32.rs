//! Deterministic archive discovery recovered from stock startup.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use crate::archive::{
    ArchiveDescriptor, ArchiveKind, ArchivePriority, AssetError, ClientDataRoot, Locale,
};

/// The first priority assigned to a patch by build 12340.
const FIRST_PATCH_PRIORITY: u16 = 64;

/// Consolidated WotLK archives and their exact stock base-table priorities.
///
/// Build 12340 starts at 63 and decrements for every non-alternate,
/// non-streaming row. The gaps therefore remain meaningful even though this
/// profile excludes the mutually exclusive split-archive layout.
const CONSOLIDATED_ARCHIVES: [(&str, u16, ArchiveKind); 10] = [
    ("expansion.MPQ", 49, ArchiveKind::Global),
    ("lichking.MPQ", 48, ArchiveKind::Global),
    ("common.MPQ", 47, ArchiveKind::Global),
    ("common-2.MPQ", 46, ArchiveKind::Global),
    ("{locale}/locale-{locale}.MPQ", 45, ArchiveKind::Localized),
    ("{locale}/speech-{locale}.MPQ", 44, ArchiveKind::Localized),
    (
        "{locale}/expansion-locale-{locale}.MPQ",
        43,
        ArchiveKind::Localized,
    ),
    (
        "{locale}/lichking-locale-{locale}.MPQ",
        42,
        ArchiveKind::Localized,
    ),
    (
        "{locale}/expansion-speech-{locale}.MPQ",
        41,
        ArchiveKind::Localized,
    ),
    (
        "{locale}/lichking-speech-{locale}.MPQ",
        40,
        ArchiveKind::Localized,
    ),
];

/// A validated, immutable archive list in highest-precedence-first order.
#[derive(Clone, Debug)]
pub struct ArchiveCatalog {
    data_root: ClientDataRoot,
    locale: Locale,
    descriptors: Vec<ArchiveDescriptor>,
}

impl ArchiveCatalog {
    /// Discovers the consolidated 3.3.5a layout and stock patch set.
    ///
    /// Numbered patches follow the executable's two-phase behavior: full
    /// relative paths sort descending, unnumbered global and localized patches
    /// are appended, and the resulting list is opened in reverse at priority
    /// 64 upward. The returned list is reordered only for resolution, with the
    /// highest stock priority first.
    ///
    /// # Errors
    ///
    /// Returns an error when a required consolidated archive is missing, an
    /// archive directory cannot be enumerated, or the computed patch band would
    /// overflow its priority representation.
    pub fn discover(data_root: ClientDataRoot, locale: Locale) -> Result<Self, AssetError> {
        let mut descriptors = discover_consolidated_archives(&data_root, locale)?;
        descriptors.extend(discover_patches(&data_root, locale)?);

        // All priorities are unique. Sorting makes the lookup contract explicit
        // without building a memory-heavy map of every file in every archive.
        descriptors.sort_by_key(|descriptor| std::cmp::Reverse(descriptor.priority()));

        Ok(Self {
            data_root,
            locale,
            descriptors,
        })
    }

    /// Returns the validated client data root.
    #[must_use]
    pub fn data_root(&self) -> &ClientDataRoot {
        &self.data_root
    }

    /// Returns the locale used to expand stock archive names.
    #[must_use]
    pub const fn locale(&self) -> Locale {
        self.locale
    }

    /// Returns archives in highest-precedence-first resolution order.
    #[must_use]
    pub fn descriptors(&self) -> &[ArchiveDescriptor] {
        &self.descriptors
    }

    /// Transfers the discovered archive list to the mount boundary.
    pub(crate) fn into_descriptors(self) -> Vec<ArchiveDescriptor> {
        self.descriptors
    }
}

/// Resolves each mandatory consolidated archive using Win32-style case folding.
fn discover_consolidated_archives(
    data_root: &ClientDataRoot,
    locale: Locale,
) -> Result<Vec<ArchiveDescriptor>, AssetError> {
    let mut descriptors = Vec::with_capacity(CONSOLIDATED_ARCHIVES.len());
    for (template, priority, kind) in CONSOLIDATED_ARCHIVES {
        let relative_name = template.replace("{locale}", locale.as_str());
        let relative_path = PathBuf::from(relative_name.replace('/', "\\"));
        let path = find_relative_file(data_root.as_path(), &relative_path)?.ok_or_else(|| {
            AssetError::MissingRequiredArchive {
                path: data_root.as_path().join(&relative_path),
            }
        })?;
        descriptors.push(ArchiveDescriptor::new(
            path,
            relative_path,
            ArchivePriority::new(priority),
            kind,
        ));
    }
    Ok(descriptors)
}

/// Reproduces the stock wildcard and reverse-open patch algorithm.
fn discover_patches(
    data_root: &ClientDataRoot,
    locale: Locale,
) -> Result<Vec<ArchiveDescriptor>, AssetError> {
    let mut stock_list = Vec::new();
    collect_numbered_patches(data_root.as_path(), None, locale, &mut stock_list)?;
    collect_numbered_patches(
        data_root.as_path(),
        Some(Path::new(locale.as_str())),
        locale,
        &mut stock_list,
    )?;

    // FUN_00401200 negates the client string comparison before these explicit
    // phase-one entries are appended.
    stock_list.sort_by_key(|entry| std::cmp::Reverse(stock_sort_key(entry)));
    append_explicit_patch(data_root.as_path(), None, locale, &mut stock_list)?;
    append_explicit_patch(
        data_root.as_path(),
        Some(Path::new(locale.as_str())),
        locale,
        &mut stock_list,
    )?;

    let mut descriptors = Vec::with_capacity(stock_list.len());
    for (offset, (path, relative_path)) in stock_list.into_iter().rev().enumerate() {
        let offset = u16::try_from(offset).map_err(|source| AssetError::ArchiveEnumeration {
            path: data_root.as_path().to_path_buf(),
            message: source.to_string(),
        })?;
        let priority = FIRST_PATCH_PRIORITY.checked_add(offset).ok_or_else(|| {
            AssetError::ArchiveEnumeration {
                path: data_root.as_path().to_path_buf(),
                message: "stock patch priority overflow".to_owned(),
            }
        })?;
        descriptors.push(ArchiveDescriptor::new(
            path,
            relative_path,
            ArchivePriority::new(priority),
            ArchiveKind::Patch,
        ));
    }
    Ok(descriptors)
}

/// Adds files matching stock's single-character numbered patch wildcard.
fn collect_numbered_patches(
    data_root: &Path,
    relative_directory: Option<&Path>,
    locale: Locale,
    output: &mut Vec<(PathBuf, PathBuf)>,
) -> Result<(), AssetError> {
    let directory = relative_directory.map_or_else(
        || data_root.to_path_buf(),
        |relative| data_root.join(relative),
    );
    let prefix = relative_directory.map_or("patch-", |_| "patch-");
    let localized_prefix = format!("patch-{}-", locale.as_str());
    let expected_prefix = relative_directory.map_or(prefix, |_| localized_prefix.as_str());

    for entry in read_directory(&directory)? {
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        if !matches_single_character_patch(file_name, expected_prefix) {
            continue;
        }

        let relative_path = relative_directory.map_or_else(
            || PathBuf::from(file_name),
            |relative| relative.join(file_name),
        );
        output.push((entry.path(), relative_path));
    }

    Ok(())
}

/// Adds an unnumbered patch if the exact stock name is present.
fn append_explicit_patch(
    data_root: &Path,
    relative_directory: Option<&Path>,
    locale: Locale,
    output: &mut Vec<(PathBuf, PathBuf)>,
) -> Result<(), AssetError> {
    let file_name = relative_directory.map_or_else(
        || "patch.MPQ".to_owned(),
        |_| format!("patch-{}.MPQ", locale.as_str()),
    );
    let relative_path = relative_directory.map_or_else(
        || PathBuf::from(&file_name),
        |relative| relative.join(&file_name),
    );
    if let Some(path) = find_relative_file(data_root, &relative_path)? {
        output.push((path, relative_path));
    }
    Ok(())
}

/// Matches `?` exactly as stock does: one character between prefix and `.MPQ`.
fn matches_single_character_patch(file_name: &str, prefix: &str) -> bool {
    let Some(suffix) = file_name.get(prefix.len()..) else {
        return false;
    };
    file_name
        .get(..prefix.len())
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(prefix))
        && suffix.len() == 5
        && suffix.is_ascii()
        && suffix[1..].eq_ignore_ascii_case(".MPQ")
}

/// Produces the case-insensitive full relative path compared by stock Win32 code.
fn stock_sort_key(entry: &(PathBuf, PathBuf)) -> String {
    format!("Data\\{}", entry.1.to_string_lossy().replace('/', "\\")).to_ascii_uppercase()
}

/// Resolves every component case-insensitively to model Win32 file lookup.
fn find_relative_file(root: &Path, relative: &Path) -> Result<Option<PathBuf>, AssetError> {
    let mut resolved = root.to_path_buf();
    for component in relative.components() {
        let expected = component.as_os_str();
        let Some(entry) = read_directory(&resolved)?
            .into_iter()
            .find(|entry| os_str_eq_ignore_ascii_case(&entry.file_name(), expected))
        else {
            return Ok(None);
        };
        resolved = entry.path();
    }

    Ok(resolved.is_file().then_some(resolved))
}

/// Reads a directory with stable asset-boundary error context.
fn read_directory(path: &Path) -> Result<Vec<fs::DirEntry>, AssetError> {
    let entries = fs::read_dir(path).map_err(|source| AssetError::ArchiveEnumeration {
        path: path.to_path_buf(),
        message: source.to_string(),
    })?;

    entries
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| AssetError::ArchiveEnumeration {
            path: path.to_path_buf(),
            message: source.to_string(),
        })
}

/// Applies ASCII case folding without requiring UTF-8 filesystem names.
fn os_str_eq_ignore_ascii_case(left: &OsStr, right: &OsStr) -> bool {
    left.to_string_lossy()
        .eq_ignore_ascii_case(&right.to_string_lossy())
}
