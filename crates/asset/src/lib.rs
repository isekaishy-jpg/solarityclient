//! Stock asset discovery, decoding, and lifetime boundaries.

#[cfg(not(target_pointer_width = "64"))]
compile_error!("solarity-asset requires a 64-bit target for full-size and HD client data");

mod archive;
mod cache;
mod database;
mod file_stack;
mod model;
mod storage;
mod terrain;
mod texture;
mod world;
mod world_model;

pub use archive::{
    ArchiveDescriptor, ArchiveKind, ArchivePriority, AssetError, AssetPath, AssetPathViolation,
    ClientDataRoot, Locale,
};
pub use database::{
    CharacterAppearanceCatalog, CharacterFacialHairStyle, CharacterHairGeoset, CharacterSection,
    CreatureCatalog, CreatureDisplayInfo, CreatureDisplayInfoExtra, CreatureModelData, WdbcHeader,
    WdbcTable,
};
pub use file_stack::{ArchiveCatalog, AssetRead, AssetStore};
pub use texture::DecodedBlpTexture;
