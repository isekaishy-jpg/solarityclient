//! One shared primary-source gate for Glue, NPC and remote-player appearance jobs.

mod admission;
mod pending;
mod work;

pub(in crate::application::player_coordinator) use pending::AppearanceTask;

use super::super::RuntimePlayerSharedCatalogs;
use solarity_asset::{ArchiveCatalog, AssetPath};
use solarity_rendering::CharacterComponentTextureLevel;

/// Frozen source identity and catalogs; the exclusive cache moves only after admission.
pub(in crate::application::player_coordinator) struct AppearanceRequest {
    pub catalog: ArchiveCatalog,
    pub catalogs: RuntimePlayerSharedCatalogs,
    pub level: CharacterComponentTextureLevel,
    pub model_path: AssetPath,
}
