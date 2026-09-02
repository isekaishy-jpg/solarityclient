//! Typed access to client database tables and data stores.
//!
//! This boundary follows the stock `DBClient.cpp`, `DBCache.cpp`, and
//! `WDataStore.cpp` family. Table schemas and validation belong here; game
//! behavior that consumes a table belongs to its owning domain crate.

mod animation;
mod appearance;
mod area;
mod c_data_store;
mod character;
mod character_base;
mod character_faction;
mod character_outfit;
mod creature;
mod db_cache;
mod db_cache_instances;
mod db_client;
mod item;
mod light;
mod loading_screen;
mod localized;
mod map;
mod paper_doll;
mod particle_color;
mod player_class;
mod race;
mod realm;
mod sound;
mod sound_advanced;
mod sound_environment;
mod w_data_store;
mod wow_client_db;

pub use animation::{AnimationDataCatalog, AnimationDataDefinition};
pub use appearance::{
    AppearanceError, CharacterCustomization, CharacterGeosetSelection, CharacterModelAppearance,
    CharacterSectionKind, CreatureModelAppearance,
};
pub use area::{AreaDefinition, AreaTableCatalog};
pub use character::{
    CharacterAppearanceCatalog, CharacterFacialHairStyle, CharacterHairGeoset, CharacterSection,
};
pub use character_base::{CharacterBaseCatalog, CharacterBaseInfo};
pub use character_faction::{CharacterFactionCatalog, CharacterFactionGroup};
pub use character_outfit::{
    CharacterStartOutfit, CharacterStartOutfitCatalog, CharacterStartOutfitItem,
};
pub use creature::{
    CreatureCatalog, CreatureDisplayInfo, CreatureDisplayInfoExtra, CreatureModelData,
};
pub use item::{
    HelmetGeosetVisibility, HelmetGeosetVisibilityCatalog, InventoryType, ItemDefinition,
    ItemDefinitionCatalog, ItemDisplayCatalog, ItemDisplayInfo, ItemVisual, ItemVisualCatalog,
    ItemVisualEffect, SpellItemEnchantment,
};
pub use light::{
    LightCatalog, LightDefinition, LightParameter, LightSkybox, SkyboxBlend, WorldLightCondition,
    WorldLightQuery, WorldLightSample, WorldLightSampleError, exterior_light_direction,
};
pub use loading_screen::{LoadingScreenCatalog, LoadingScreenDefinition};
pub use map::{MapCatalog, MapDefinition, MapKind};
pub use paper_doll::{PaperDollItemFrameCatalog, PaperDollItemFrameDefinition};
pub use particle_color::{ParticleColorCatalog, ParticleColorDefinition};
pub use player_class::{CharacterClassCatalog, CharacterClassDefinition};
pub use race::{CharacterRace, CharacterRaceCatalog};
pub use realm::{
    RealmCategoryCatalog, RealmCategoryDefinition, RealmConfiguration, RealmConfigurationCatalog,
};
pub use sound::{SoundAsset, SoundEntry, SoundEntryCatalog};
pub use sound_advanced::{AdvancedSoundEntry, AdvancedSoundEntryCatalog};
pub use sound_environment::{
    LiquidTypeCatalog, LiquidTypeDefinition, SoundEmitterCatalog, SoundEmitterDefinition,
};
pub use wow_client_db::{WdbcHeader, WdbcTable};
