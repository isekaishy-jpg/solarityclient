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
pub use cache::M2ModelCache;
pub use database::{
    AppearanceError, CharacterAppearanceCatalog, CharacterCustomization, CharacterFacialHairStyle,
    CharacterGeosetSelection, CharacterHairGeoset, CharacterModelAppearance, CharacterSection,
    CharacterSectionKind, CreatureCatalog, CreatureDisplayInfo, CreatureDisplayInfoExtra,
    CreatureModelAppearance, CreatureModelData, WdbcHeader, WdbcTable,
};
pub use file_stack::{ArchiveCatalog, AssetRead, AssetStore};
pub use model::{
    DecodedM2Model, M2Batch, M2BlendMode, M2Material, M2SkinProfile, M2Submesh, M2Texture,
    M2TextureKind, M2Vertex,
};
pub use texture::DecodedBlpTexture;
