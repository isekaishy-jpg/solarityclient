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
pub use cache::{BlpTextureCache, M2ModelCache, WmoModelCache};
pub use database::{
    AdvancedSoundEntry, AdvancedSoundEntryCatalog, AnimationDataCatalog, AnimationDataDefinition,
    AppearanceError, AreaDefinition, AreaTableCatalog, CharacterAppearanceCatalog,
    CharacterBaseCatalog, CharacterBaseInfo, CharacterClassCatalog, CharacterClassDefinition,
    CharacterCustomization, CharacterFacialHairStyle, CharacterFactionCatalog,
    CharacterFactionGroup, CharacterGeosetSelection, CharacterHairGeoset, CharacterModelAppearance,
    CharacterRace, CharacterRaceCatalog, CharacterSection, CharacterSectionKind,
    CharacterStartOutfit, CharacterStartOutfitCatalog, CharacterStartOutfitItem, CreatureCatalog,
    CreatureDisplayInfo, CreatureDisplayInfoExtra, CreatureFamilyCatalog, CreatureFamilyDefinition,
    CreatureModelAppearance, CreatureModelData, HelmetGeosetVisibility,
    HelmetGeosetVisibilityCatalog, InventoryType, ItemDefinition, ItemDefinitionCatalog,
    ItemDisplayCatalog, ItemDisplayInfo, ItemVisual, ItemVisualCatalog, ItemVisualEffect,
    LightCatalog, LightDefinition, LightParameter, LightSkybox, LiquidTypeCatalog,
    LiquidTypeDefinition, LoadingScreenCatalog, LoadingScreenDefinition, MapCatalog, MapDefinition,
    MapKind, PaperDollItemFrameCatalog, PaperDollItemFrameDefinition, ParticleColorCatalog,
    ParticleColorDefinition, RealmCategoryCatalog, RealmCategoryDefinition, RealmConfiguration,
    RealmConfigurationCatalog, SkyboxBlend, SoundAsset, SoundEmitterCatalog,
    SoundEmitterDefinition, SoundEntry, SoundEntryCatalog, SpellItemEnchantment, WdbcHeader,
    WdbcTable, WorldLightCondition, WorldLightQuery, WorldLightSample, WorldLightSampleError,
    exterior_light_direction,
};
pub use file_stack::{ArchiveCatalog, AssetRead, AssetStore, AssetStoreHandle, LocalizedDocument};
pub use model::{
    DecodedM2Model, M2AnimationSet, M2Attachment, M2Batch, M2BlendMode, M2Bone, M2Camera,
    M2CollisionMesh, M2ColorAnimation, M2Event, M2EventTrack, M2Interpolation, M2Light,
    M2LightKind, M2Material, M2ModelBounds, M2ParticleEmitter, M2ParticleLifetimeTrack,
    M2RibbonEmitter, M2Sequence, M2SequenceStorage, M2SkinProfile, M2Submesh, M2Texture,
    M2TextureKind, M2TextureTransform, M2TextureWeight, M2Track, M2TrackChannel, M2Vertex,
};
pub use shader::{BlsPermutation, BlsShaderStage, DecodedBlsShader};
pub use terrain::{
    DecodedTerrainTile, TERRAIN_ALPHA_MAP_BYTE_COUNT, TERRAIN_ALPHA_MAP_WIDTH,
    TERRAIN_SHADOW_MAP_BYTE_COUNT, TERRAIN_SHADOW_MAP_WIDTH, TerrainAlphaMap, TerrainChunk,
    TerrainChunkIndex, TerrainDoodadPlacement, TerrainLiquidChunk, TerrainLiquidLayer,
    TerrainLiquidTable, TerrainMap, TerrainShadowMap, TerrainSoundEmitter, TerrainTextureLayer,
    TerrainTile, TerrainTileIndex, TerrainWorldModelPlacement,
};
pub use texture::{BlpBlockCompression, BlpBlockMip, BlpTextureSource, DecodedBlpTexture};
pub use world_model::{
    DecodedWorldModel, DecodedWorldModelGroup, WorldModelBatch, WorldModelBatchClass,
    WorldModelBlendMode, WorldModelBspNode, WorldModelDoodad, WorldModelDoodadSet,
    WorldModelDoodadSetError, WorldModelLiquid, WorldModelLiquidVertex, WorldModelMaterial,
    WorldModelPolygon, WorldModelShader,
};
