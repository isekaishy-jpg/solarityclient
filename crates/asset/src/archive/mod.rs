//! MPQ archive discovery, patch ordering, and file access.
//!
//! The stock executable identifies this boundary through `SFile.cpp`,
//! `SFile2-Core.cpp`, and `SFileArchives.cpp`. Format decoding remains in the
//! modules that own each decoded asset.

mod dependency;
mod error;
mod types;

pub(crate) use dependency::MountedArchive;
pub use error::{AssetError, AssetPathViolation};
pub use types::{
    ArchiveDescriptor, ArchiveKind, ArchivePriority, AssetPath, ClientDataRoot, Locale,
};
