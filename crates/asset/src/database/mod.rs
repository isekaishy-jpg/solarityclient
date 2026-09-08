//! Typed access to client database tables and data stores.
//!
//! This boundary follows the stock `DBClient.cpp`, `DBCache.cpp`, and
//! `WDataStore.cpp` family. Table schemas and validation belong here; game
//! behavior that consumes a table belongs to its owning domain crate.

mod animation;
mod appearance;
mod area;
mod area_trigger;
mod c_data_store;
mod character;
mod character_base;
mod character_faction;
mod character_outfit;
mod combat_stat;
mod creature;
mod db_cache;
mod db_cache_instances;
mod db_client;
mod environmental_damage;
mod game_object;
mod item;
mod light;
mod liquid_material;
mod loading_screen;
mod localized;
mod map;
mod map_difficulty;
mod movement_sound;
mod paper_doll;
mod particle_color;
mod player_class;
mod race;
mod realm;
mod sound;
mod sound_advanced;
mod sound_environment;
mod spell_effect;
mod spell_name;
pub use spell_effect::{SpellEffectCatalog, SpellEffectDefinition};
mod spell_visual_effect;
mod transport;
mod ui_sound;
mod w_data_store;
mod wow_client_db;

pub use animation::{AnimationDataCatalog, AnimationDataDefinition};
pub use appearance::{
    AppearanceError, CharacterCustomization, CharacterGeosetSelection, CharacterModelAppearance,
    CharacterSectionKind, CreatureModelAppearance,
};
pub use area::{AreaDefinition, AreaTableCatalog};
pub use area_trigger::{AreaTriggerCatalog, AreaTriggerDefinition, AreaTriggerShape};
pub use character::{
    CharacterAppearanceCatalog, CharacterFacialHairStyle, CharacterHairGeoset, CharacterSection,
};
pub use character_base::{CharacterBaseCatalog, CharacterBaseInfo};
pub use character_faction::{
    CharacterFactionCatalog, CharacterFactionGroup, FactionTemplateDefinition,
};
pub use character_outfit::{
    CharacterStartOutfit, CharacterStartOutfitCatalog, CharacterStartOutfitItem,
};
pub use combat_stat::CombatStatCatalog;
pub use creature::{
    CreatureCatalog, CreatureDisplayInfo, CreatureDisplayInfoExtra, CreatureFamilyCatalog,
    CreatureFamilyDefinition, CreatureModelData,
};
pub use environmental_damage::{EnvironmentalDamageCatalog, EnvironmentalVisualKit};
pub use game_object::{GameObjectDisplayCatalog, GameObjectDisplayInfo};
pub use item::{
    HelmetGeosetVisibility, HelmetGeosetVisibilityCatalog, InventoryType, ItemDefinition,
    ItemDefinitionCatalog, ItemDisplayCatalog, ItemDisplayInfo, ItemVisual, ItemVisualCatalog,
    ItemVisualEffect, SpellItemEnchantment,
};
pub use light::{
    LightCatalog, LightDefinition, LightParameter, LightSkybox, ModelLightColors, SkyboxBlend,
    WorldLightCondition, WorldLightQuery, WorldLightSample, WorldLightSampleError,
    exterior_light_direction, exterior_light_direction_at,
};
pub use liquid_material::{LiquidMaterialCatalog, LiquidMaterialDefinition};
pub use loading_screen::{LoadingScreenCatalog, LoadingScreenDefinition};
pub use map::{MapCatalog, MapDefinition, MapKind};
pub use map_difficulty::MapDifficultyCatalog;
pub use movement_sound::{CreatureMovementSounds, MovementSoundCatalog};
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
    AreaSoundReferences, LiquidTypeCatalog, LiquidTypeDefinition, SoundAmbienceDefinition,
    SoundEmitterCatalog, SoundEmitterDefinition, WorldChunkSoundKey, WorldModelAreaCatalog,
    WorldModelAreaDefinition, WorldModelAreaKey, WorldStateZoneSound, ZoneIntroMusicDefinition,
    ZoneMusicDefinition, ZoneSoundCatalog, ZoneSoundOverrideCatalog,
};
pub use spell_name::SpellNameCatalog;
pub use spell_visual_effect::{SpellVisualEffectCatalog, SpellVisualEffectDefinition};
pub use transport::{
    TaxiPathNode, TransportAnimationNode, TransportCatalog, TransportPhysicsRecord,
    TransportRotationNode,
};
pub use ui_sound::{UiSoundLookup, UiSoundLookupCatalog};
pub use wow_client_db::{WdbcHeader, WdbcTable};
