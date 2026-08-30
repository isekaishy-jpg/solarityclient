//! Stable asset-boundary failures independent of the selected MPQ decoder.

use std::path::PathBuf;

use thiserror::Error;

use crate::archive::AssetPath;

/// The rejected invariant of an archive-relative asset path.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AssetPathViolation {
    /// A lookup cannot name the empty path.
    #[error("path is empty")]
    Empty,
    /// Stock MPQ hash paths for this client are ASCII byte strings.
    #[error("path is not ASCII")]
    NonAscii,
    /// A NUL cannot be part of the MPQ filename passed to the dependency.
    #[error("path contains a NUL byte")]
    ContainsNul,
    /// The path must be relative and must name a file.
    #[error("path is not archive-relative")]
    NotArchiveRelative,
    /// Empty, current-directory, and parent-directory components are invalid.
    #[error("path contains an invalid component")]
    InvalidComponent,
}

/// A failure at the public client-asset boundary.
#[derive(Debug, Error)]
pub enum AssetError {
    /// The supplied client data root does not resolve to a directory.
    #[error("invalid client data root {path}: {message}")]
    InvalidDataRoot {
        /// The caller-supplied or resolved path.
        path: PathBuf,
        /// Filesystem context kept independent of a concrete I/O error type.
        message: String,
    },
    /// The requested locale is not present in stock's build-12340 locale table.
    #[error("unsupported client locale {value}")]
    UnsupportedLocale {
        /// The rejected locale token.
        value: String,
    },
    /// The requested file path violates MPQ lookup invariants.
    #[error("invalid asset path {path}: {violation}")]
    InvalidAssetPath {
        /// The rejected path.
        path: String,
        /// The stable rejected invariant.
        violation: AssetPathViolation,
    },
    /// A mandatory archive in the selected stock layout is absent.
    #[error("missing required client archive {path}")]
    MissingRequiredArchive {
        /// The expected archive path.
        path: PathBuf,
    },
    /// A directory needed for stock archive enumeration could not be read.
    #[error("failed to enumerate client archive directory {path}: {message}")]
    ArchiveEnumeration {
        /// The directory being enumerated.
        path: PathBuf,
        /// Filesystem context.
        message: String,
    },
    /// The stock loose `Interface\\AddOns` directory could not be enumerated.
    #[error("failed to enumerate client AddOn directory {path}: {message}")]
    AddonEnumeration {
        /// Concrete install-relative directory being enumerated.
        path: PathBuf,
        /// Filesystem context.
        message: String,
    },
    /// A selected loose AddOn file could not be read.
    #[error("failed to read loose client AddOn file {path}: {message}")]
    AddonRead {
        /// Concrete install-relative file selected by the AddOn stack.
        path: PathBuf,
        /// Filesystem context.
        message: String,
    },
    /// An archive was present but could not be opened.
    #[error("failed to open client archive {path}: {message}")]
    ArchiveOpen {
        /// The concrete archive path.
        path: PathBuf,
        /// Dependency context without exposing its error type.
        message: String,
    },
    /// An archive hash lookup failed before absence could be established.
    #[error("failed to search client archive {archive} for {asset}: {message}")]
    ArchiveLookup {
        /// The concrete archive path.
        archive: PathBuf,
        /// The normalized requested path.
        asset: AssetPath,
        /// Dependency context without exposing its error type.
        message: String,
    },
    /// A selected archive entry could not be decoded.
    #[error("failed to read {asset} from client archive {archive}: {message}")]
    ArchiveRead {
        /// The concrete archive path.
        archive: PathBuf,
        /// The normalized requested path.
        asset: AssetPath,
        /// Dependency context without exposing its error type.
        message: String,
    },
    /// No mounted stock archive contains the requested asset.
    #[error("client asset not found: {path}")]
    AssetNotFound {
        /// The normalized requested path.
        path: AssetPath,
    },
    /// A resolved client database does not contain a valid stock WDBC table.
    #[error("failed to decode client database {path}: {message}")]
    DatabaseDecode {
        /// The normalized database asset path.
        path: AssetPath,
        /// Decoder or structural validation context.
        message: String,
    },
    /// A resolved client texture is not a decodable stock BLP image.
    #[error("failed to decode client texture {path}: {message}")]
    TextureDecode {
        /// The normalized texture asset path.
        path: AssetPath,
        /// Parser or pixel-conversion context.
        message: String,
    },
    /// A resolved model or one of its external skin profiles is malformed.
    #[error("failed to decode client model {path}: {message}")]
    ModelDecode {
        /// The normalized model or skin-profile asset path.
        path: AssetPath,
        /// Parser or structural validation context.
        message: String,
    },
    /// A resolved stock shader library is malformed or uses another profile.
    #[error("failed to decode client shader {path}: {message}")]
    ShaderDecode {
        /// Normalized BLS asset path.
        path: AssetPath,
        /// Structural or shader-profile validation context.
        message: String,
    },
}
