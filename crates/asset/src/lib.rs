//! Stock asset discovery, decoding, and lifetime boundaries.

#[cfg(not(target_pointer_width = "64"))]
compile_error!("solarity-asset requires a 64-bit target for full-size and HD client data");

mod archive;
mod cache;
mod database;
mod file_stack;
mod model;
mod shader;
mod storage;
mod terrain;
mod texture;
mod world;
mod world_model;

pub use archive::{
    ArchiveDescriptor, ArchiveKind, ArchivePriority, AssetError, AssetPath, AssetPathViolation,
    ClientDataRoot, Locale,
};
pub use cache::{BlpTextureCache, M2ModelCache};
pub use database::{
    AppearanceError, AreaDefinition, AreaTableCatalog, CharacterAppearanceCatalog,
    CharacterClassCatalog, CharacterClassDefinition, CharacterCustomization,
    CharacterFacialHairStyle, CharacterGeosetSelection, CharacterHairGeoset,
    CharacterModelAppearance, CharacterRace, CharacterRaceCatalog, CharacterSection,
    CharacterSectionKind, CreatureCatalog, CreatureDisplayInfo, CreatureDisplayInfoExtra,
    CreatureModelAppearance, CreatureModelData, HelmetGeosetVisibility,
    HelmetGeosetVisibilityCatalog, InventoryType, ItemDefinition, ItemDefinitionCatalog,
    ItemDisplayCatalog, ItemDisplayInfo, MapCatalog, MapDefinition, MapKind, RealmCategoryCatalog,
    RealmCategoryDefinition, RealmConfiguration, RealmConfigurationCatalog, WdbcHeader, WdbcTable,
};
pub use file_stack::{ArchiveCatalog, AssetRead, AssetStore, AssetStoreHandle};
pub use model::{
    DecodedM2Model, M2AnimationSet, M2Batch, M2BlendMode, M2Bone, M2Interpolation, M2Material,
    M2Sequence, M2SequenceStorage, M2SkinProfile, M2Submesh, M2Texture, M2TextureKind, M2Track,
    M2TrackChannel, M2Vertex,
};
pub use shader::{BlsPermutation, BlsShaderStage, DecodedBlsShader};
pub use terrain::{
    DecodedTerrainTile, TERRAIN_ALPHA_MAP_BYTE_COUNT, TERRAIN_ALPHA_MAP_WIDTH, TerrainAlphaMap,
    TerrainChunk, TerrainChunkIndex, TerrainDoodadPlacement, TerrainMap, TerrainSoundEmitter,
    TerrainTextureLayer, TerrainTile, TerrainTileIndex, TerrainWorldModelPlacement,
};
pub use texture::{BlpTextureSource, DecodedBlpTexture};
