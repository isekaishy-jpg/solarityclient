//! Finite jobs exclusively own their archive and model/texture cache bank.

use super::super::{RuntimePlayerError, RuntimePlayerPresentation, RuntimePlayerSharedCatalogs};
use super::AppearanceWorkerCache;
use crate::application::player_coordinator::population_worker;
use crate::application::unit_animation::UnitAnimationScene;
use solarity_asset::{ArchiveCatalog, AssetStoreHandle};
use solarity_rendering::CharacterComponentTextureLevel;
use std::sync::Arc;

/// Worker-local caches retain expensive decode results across finite jobs.
pub(in crate::application::player_coordinator) fn with_worker_presentation<T>(
    catalog: ArchiveCatalog,
    catalogs: RuntimePlayerSharedCatalogs,
    component_texture_level: CharacterComponentTextureLevel,
    worker_cache: &mut AppearanceWorkerCache,
    prepare: impl FnOnce(&mut RuntimePlayerPresentation) -> Result<T, RuntimePlayerError>,
) -> Result<T, RuntimePlayerError> {
    worker_cache.mount(&catalog)?;
    let store = worker_cache
        .store
        .take()
        .unwrap_or_else(|| unreachable!("mounted worker bank owns its store"));
    let models = std::mem::take(&mut worker_cache.models);
    let textures = std::mem::take(&mut worker_cache.textures);
    let mut presentation = RuntimePlayerPresentation {
        passenger_frames: crate::application::unit_passenger::UnitPassengerFrames::new(Arc::clone(
            &catalogs.vehicles,
        )),
        vehicles: catalogs.vehicles,
        unit_animations: UnitAnimationScene::default(),
        camera_opacity_subject: Default::default(),
        arena_map: false,
        animation_mouse_turning: false,
        assets: AssetStoreHandle::new(store),
        animations: catalogs.animations,
        creatures: catalogs.creatures,
        creature_families: catalogs.creature_families,
        characters: catalogs.characters,
        races: catalogs.races,
        helmet_visibility: catalogs.helmet_visibility,
        start_outfits: catalogs.start_outfits,
        item_definitions: catalogs.item_definitions,
        item_displays: catalogs.item_displays,
        item_visuals: catalogs.item_visuals,
        particle_colors: catalogs.particle_colors,
        models,
        textures,
        component_texture_level,
        resident: None,
        local_dimensions: None,
        creatures_resident: Vec::new(),
        remote_players: Vec::new(),
        creature_worker: population_worker::PopulationWorker::new(),
        remote_worker: population_worker::PopulationWorker::new(),
        local_worker: population_worker::PopulationWorker::new(),
        glue_character: None,
        requested_glue_character: None,
        glue_worker_catalog: None,
        glue_worker_cache: Some(AppearanceWorkerCache::default()),
        pending_glue_character: None,
        failed_glue_character: None,
    };
    let result = prepare(&mut presentation);

    // The appearance operation collects only at completion; retries retain their source prefix.
    let assets = presentation.assets;
    let models = std::mem::take(&mut presentation.models);
    let textures = std::mem::take(&mut presentation.textures);
    let store = assets.try_into_store();
    worker_cache.models = models;
    worker_cache.textures = textures;
    match store {
        Ok(store) => worker_cache.store = Some(store),
        Err(_assets) => return Err(RuntimePlayerError::SharedGlueWorkerAssetStore),
    }
    result
}
