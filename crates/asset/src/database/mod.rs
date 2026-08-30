//! Typed access to client database tables and data stores.
//!
//! This boundary follows the stock `DBClient.cpp`, `DBCache.cpp`, and
//! `WDataStore.cpp` family. Table schemas and validation belong here; game
//! behavior that consumes a table belongs to its owning domain crate.

mod appearance;
mod area;
mod c_data_store;
mod character;
mod creature;
mod db_cache;
mod db_cache_instances;
mod db_client;
mod item;
mod localized;
mod player_class;
mod race;
mod realm;
mod w_data_store;
mod wow_client_db;

pub use appearance::{
    AppearanceError, CharacterCustomization, CharacterGeosetSelection, CharacterModelAppearance,
    CharacterSectionKind, CreatureModelAppearance,
};
pub use area::{AreaDefinition, AreaTableCatalog};
pub use character::{
    CharacterAppearanceCatalog, CharacterFacialHairStyle, CharacterHairGeoset, CharacterSection,
};
pub use creature::{
    CreatureCatalog, CreatureDisplayInfo, CreatureDisplayInfoExtra, CreatureModelData,
};
pub use item::{
    HelmetGeosetVisibility, HelmetGeosetVisibilityCatalog, InventoryType, ItemDefinition,
    ItemDefinitionCatalog, ItemDisplayCatalog, ItemDisplayInfo,
};
pub use player_class::{CharacterClassCatalog, CharacterClassDefinition};
pub use race::{CharacterRace, CharacterRaceCatalog};
pub use realm::{
    RealmCategoryCatalog, RealmCategoryDefinition, RealmConfiguration, RealmConfigurationCatalog,
};
pub use wow_client_db::{WdbcHeader, WdbcTable};
