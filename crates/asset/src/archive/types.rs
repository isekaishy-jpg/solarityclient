//! Validated values used by stock archive discovery and lookup.

use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::archive::{AssetError, AssetPathViolation};

/// A validated path to the client's `Data` directory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientDataRoot(PathBuf);

impl ClientDataRoot {
    /// Validates and canonicalizes a client data directory.
    ///
    /// Archive-set validation is deliberately deferred to discovery so this
    /// type remains useful for reporting a specific missing archive.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::InvalidDataRoot`] when the path does not resolve
    /// to a directory.
    pub fn new(path: impl AsRef<Path>) -> Result<Self, AssetError> {
        let supplied_path = path.as_ref();
        let canonical_path =
            supplied_path
                .canonicalize()
                .map_err(|source| AssetError::InvalidDataRoot {
                    path: supplied_path.to_path_buf(),
                    message: source.to_string(),
                })?;

        if !canonical_path.is_dir() {
            return Err(AssetError::InvalidDataRoot {
                path: canonical_path,
                message: "path is not a directory".to_owned(),
            });
        }

        Ok(Self(canonical_path))
    }

    /// Returns the canonical filesystem path.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for ClientDataRoot {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

/// A locale identifier present in the build-12340 executable's probe table.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Locale {
    /// German.
    DeDe,
    /// English (Great Britain).
    EnGb,
    /// English (United States).
    EnUs,
    /// Spanish (Spain).
    EsEs,
    /// French.
    FrFr,
    /// Korean.
    KoKr,
    /// Simplified Chinese.
    ZhCn,
    /// Traditional Chinese.
    ZhTw,
    /// Legacy English client data for mainland China.
    EnCn,
    /// Legacy English client data for Taiwan.
    EnTw,
    /// Spanish (Mexico).
    EsMx,
    /// Russian.
    RuRu,
}

impl Locale {
    /// Returns the four-byte directory and archive-name token used by stock.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DeDe => "deDE",
            Self::EnGb => "enGB",
            Self::EnUs => "enUS",
            Self::EsEs => "esES",
            Self::FrFr => "frFR",
            Self::KoKr => "koKR",
            Self::ZhCn => "zhCN",
            Self::ZhTw => "zhTW",
            Self::EnCn => "enCN",
            Self::EnTw => "enTW",
            Self::EsMx => "esMX",
            Self::RuRu => "ruRU",
        }
    }
}

impl fmt::Display for Locale {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for Locale {
    type Err = AssetError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "deDE" => Ok(Self::DeDe),
            "enGB" => Ok(Self::EnGb),
            "enUS" => Ok(Self::EnUs),
            "esES" => Ok(Self::EsEs),
            "frFR" => Ok(Self::FrFr),
            "koKR" => Ok(Self::KoKr),
            "zhCN" => Ok(Self::ZhCn),
            "zhTW" => Ok(Self::ZhTw),
            "enCN" => Ok(Self::EnCn),
            "enTW" => Ok(Self::EnTw),
            "esMX" => Ok(Self::EsMx),
            "ruRU" => Ok(Self::RuRu),
            _ => Err(AssetError::UnsupportedLocale {
                value: value.to_owned(),
            }),
        }
    }
}

/// A normalized, archive-relative MPQ file path.
///
/// MPQ hashing is ASCII case-insensitive. Storing one uppercase representation
/// avoids allocating a normalized string at every archive probe.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AssetPath(String);

impl AssetPath {
    /// Validates and normalizes a stock client-internal path.
    ///
    /// Both slash forms are accepted because Blizzard data references contain
    /// both, while the MPQ hash boundary uses backslashes.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::InvalidAssetPath`] for empty, absolute, non-ASCII,
    /// NUL-containing, or traversal paths.
    pub fn new(path: impl AsRef<str>) -> Result<Self, AssetError> {
        let original = path.as_ref();
        let violation = validate_asset_path(original);
        if let Some(violation) = violation {
            return Err(AssetError::InvalidAssetPath {
                path: original.to_owned(),
                violation,
            });
        }

        Ok(Self(original.replace('/', "\\").to_ascii_uppercase()))
    }

    /// Returns the normalized MPQ hash path.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AssetPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl AsRef<str> for AssetPath {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// Validates only invariants shared by every stock MPQ file lookup.
fn validate_asset_path(path: &str) -> Option<AssetPathViolation> {
    if path.is_empty() {
        return Some(AssetPathViolation::Empty);
    }
    if !path.is_ascii() {
        return Some(AssetPathViolation::NonAscii);
    }
    if path.contains('\0') {
        return Some(AssetPathViolation::ContainsNul);
    }
    if path.starts_with(['/', '\\'])
        || path.as_bytes().get(1) == Some(&b':')
        || path.ends_with(['/', '\\'])
    {
        return Some(AssetPathViolation::NotArchiveRelative);
    }

    if path
        .split(['/', '\\'])
        .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return Some(AssetPathViolation::InvalidComponent);
    }

    None
}

/// The precedence band assigned to an archive by stock startup.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ArchivePriority(u16);

impl ArchivePriority {
    /// Creates a priority recovered from the stock archive startup table.
    #[must_use]
    pub(crate) const fn new(value: u16) -> Self {
        Self(value)
    }

    /// Returns the stock priority value.
    #[must_use]
    pub const fn value(self) -> u16 {
        self.0
    }
}

/// The stock role of a mounted archive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArchiveKind {
    /// Global expansion or common client data.
    Global,
    /// Locale-specific data or speech.
    Localized,
    /// A global or locale-specific patch archive.
    Patch,
}

/// Public metadata for an archive without exposing the MPQ implementation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveDescriptor {
    path: PathBuf,
    relative_path: PathBuf,
    priority: ArchivePriority,
    kind: ArchiveKind,
}

impl ArchiveDescriptor {
    /// Creates metadata after stock discovery has selected a concrete file.
    #[must_use]
    pub(crate) fn new(
        path: PathBuf,
        relative_path: PathBuf,
        priority: ArchivePriority,
        kind: ArchiveKind,
    ) -> Self {
        Self {
            path,
            relative_path,
            priority,
            kind,
        }
    }

    /// Returns the absolute archive path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the path relative to the client `Data` directory.
    #[must_use]
    pub fn relative_path(&self) -> &Path {
        &self.relative_path
    }

    /// Returns the stock precedence value.
    #[must_use]
    pub const fn priority(&self) -> ArchivePriority {
        self.priority
    }

    /// Returns the archive's stock role.
    #[must_use]
    pub const fn kind(&self) -> ArchiveKind {
        self.kind
    }
}
