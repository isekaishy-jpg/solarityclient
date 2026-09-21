//! Ordered startup discovery, archive opening and typed catalog preparation.

use crate::application::{
    ApplicationError, character_directory::PreparedCharacterMetadata,
    realm_directory::RuntimeRealmMetadata,
};
use solarity_asset::*;
use solarity_cpu::{CpuError, CpuTaskStep, JobContext};
use solarity_network::{AddonManifestError, WorldAddon, WorldAddonManifest};
use solarity_ui::{AddonCatalog, AddonCatalogError, STANDARD_ADDON_CRC};
use std::ops::ControlFlow;
use thiserror::Error;

#[derive(Debug, Error)]
pub(super) enum StartupCatalogError {
    #[error(transparent)]
    Asset(#[from] AssetError),
    #[error(transparent)]
    Addon(#[from] AddonCatalogError),
    #[error(transparent)]
    Manifest(#[from] AddonManifestError),
}
impl From<StartupCatalogError> for ApplicationError {
    fn from(error: StartupCatalogError) -> Self {
        match error {
            StartupCatalogError::Asset(error) => Self::Asset(error),
            StartupCatalogError::Addon(error) => Self::AddonCatalog(error),
            StartupCatalogError::Manifest(error) => Self::AddonManifest(error),
        }
    }
}

// One declaration keeps typed storage, ordered steps and terminal publication
// together. These are concrete startup loads, not a second task scheduler.
macro_rules! catalogs {
    ($($field:ident: $kind:ty = $load:expr),+ $(,)?) => {
        pub(super) struct StartupCatalogs { $(pub(super) $field: $kind,)+ }
        #[derive(Default)]
        struct Pending { $($field: Option<$kind>,)+ }
        type Load = fn(&mut Pending, &mut AssetStore) -> Result<(), StartupCatalogError>;
        const LOADS: &[Load] = &[$(|pending, store| {
            pending.$field = Some(($load)(store)?);
            Ok(())
        }),+];
        impl Pending {
            fn finish(self) -> StartupCatalogs {
                StartupCatalogs { $($field: self.$field.unwrap_or_else(||
                    unreachable!(concat!("completed startup catalog: ", stringify!($field)))),)+ }
            }
        }
    };
}

// Preserve the original startup's catalog and first-error order. Each entry
// yields a worker turn; codecs inside an individual catalog remain indivisible.
catalogs! {
    spells: SpellEffectCatalog = SpellEffectCatalog::load,
    environmental: EnvironmentalDamageCatalog = EnvironmentalDamageCatalog::load,
    animations: AnimationDataCatalog = AnimationDataCatalog::load,
    realm_metadata: RuntimeRealmMetadata = RuntimeRealmMetadata::load,
    character_metadata: PreparedCharacterMetadata = PreparedCharacterMetadata::load,
    creatures: CreatureCatalog = CreatureCatalog::load,
    creature_families: CreatureFamilyCatalog = CreatureFamilyCatalog::load,
    vehicles: VehicleCatalog = VehicleCatalog::load,
    characters: CharacterAppearanceCatalog = CharacterAppearanceCatalog::load,
    races: CharacterRaceCatalog = CharacterRaceCatalog::load,
    helmet_visibility: HelmetGeosetVisibilityCatalog = HelmetGeosetVisibilityCatalog::load,
    start_outfits: CharacterStartOutfitCatalog = CharacterStartOutfitCatalog::load,
    item_definitions: ItemDefinitionCatalog = ItemDefinitionCatalog::load,
    item_displays: ItemDisplayCatalog = ItemDisplayCatalog::load,
    item_visuals: ItemVisualCatalog = ItemVisualCatalog::load,
    particle_colors: ParticleColorCatalog = ParticleColorCatalog::load,
    game_object_displays: GameObjectDisplayCatalog = GameObjectDisplayCatalog::load,
    transport_paths: TransportCatalog = TransportCatalog::load,
    addon_catalog: AddonCatalog = AddonCatalog::discover,
    maps: MapCatalog = MapCatalog::load,
    area_triggers: AreaTriggerCatalog = AreaTriggerCatalog::load,
    loading_screens: Option<LoadingScreenCatalog> = loading_screens,
    lights: LightCatalog = LightCatalog::load,
    weather: WeatherCatalog = WeatherCatalog::load,
    screen_effects: ScreenEffectCatalog = ScreenEffectCatalog::load,
    liquids: LiquidTypeCatalog = LiquidTypeCatalog::load,
}

fn loading_screens(store: &mut AssetStore) -> Result<Option<LoadingScreenCatalog>, AssetError> {
    match LoadingScreenCatalog::load(store) {
        Ok(catalog) => Ok(Some(catalog)),
        Err(AssetError::AssetNotFound { .. }) => Ok(None),
        Err(error) => Err(error),
    }
}

pub(super) struct PreparedStartup {
    pub(super) catalog: ArchiveCatalog,
    pub(super) assets: AssetStore,
    pub(super) catalogs: StartupCatalogs,
    pub(super) addon_manifest: WorldAddonManifest,
    pub(super) presentation:
        Result<super::startup_presentation::PreparedStartupPresentation, solarity_ui::FontError>,
    pub(super) ui: Result<PreparedStartupUi, solarity_ui::GlueError>,
}

pub(super) struct PreparedStartupUi {
    pub(super) glue: solarity_ui::GlueUiSources,
    pub(super) sound: crate::application::sound_coordinator::PreparedSoundSources,
}

enum Stage {
    Discover(ClientDataRoot, Locale),
    Mounting(AssetMount),
    Ready(AssetStore),
}

/// Discovery and all MPQ/DBC work happen only after worker admission. Private
/// partial catalogs are dropped on failure or cancellation without publication.
pub(super) fn prepare(
    root: ClientDataRoot,
    locale: Locale,
    shared: crate::application::texture_source_job::SharedTextureSources,
    pixel_extent: (u32, u32),
) -> impl FnMut(&JobContext<'_>) -> CpuTaskStep<Result<PreparedStartup, StartupCatalogError>> {
    let mut stage = Some(Stage::Discover(root, locale));
    let mut catalog = None;
    let mut pending = Pending::default();
    let mut index = 0;
    let read_budget = shared.read_budget();
    let budget = shared.budget.clone();
    let mut presentation = super::startup_presentation::Preparation::new(shared, pixel_extent);
    let mut addon_manifest = None;
    let mut presentation_result = None;
    let mut glue = None;
    move |context| {
        context.diagnostic_value("startup.catalog.step", index as u64);
        if context.is_cancelled() {
            return CpuTaskStep::Complete(Err(AssetError::from(CpuError::JobCancelled).into()));
        }
        let current = stage
            .take()
            .unwrap_or_else(|| unreachable!("one startup source owner"));
        let result = match current {
            Stage::Discover(root, locale) => (|| {
                let discovered = ArchiveCatalog::discover(root, locale)?;
                discovered
                    .model_cache_service()
                    .configure_storage(budget.clone())?;
                let mount = AssetStore::begin_mount(discovered.clone())?;
                catalog = Some(discovered);
                Ok(Stage::Mounting(mount))
            })(),
            Stage::Mounting(mount) => mount
                .advance()
                .map(|next| match next {
                    ControlFlow::Continue(mount) => Stage::Mounting(mount),
                    ControlFlow::Break(store) => Stage::Ready(store),
                })
                .map_err(StartupCatalogError::from),
            Stage::Ready(mut assets) => {
                if let Some(load) = LOADS.get(index) {
                    let result =
                        assets.with_read_budget(&read_budget, |store| load(&mut pending, store));
                    index += 1;
                    result.map(|()| Stage::Ready(assets))
                } else {
                    if addon_manifest.is_none() {
                        match manifest(
                            pending
                                .addon_catalog
                                .as_ref()
                                .unwrap_or_else(|| unreachable!("all catalogs loaded")),
                        ) {
                            Ok(value) => addon_manifest = Some(value),
                            Err(error) => return CpuTaskStep::Complete(Err(error)),
                        }
                        stage = Some(Stage::Ready(assets));
                        return CpuTaskStep::Continue;
                    }
                    if presentation_result.is_none() {
                        match presentation.step(&mut assets) {
                            CpuTaskStep::Continue => {}
                            CpuTaskStep::Wait(edge) => {
                                stage = Some(Stage::Ready(assets));
                                return CpuTaskStep::Wait(edge);
                            }
                            CpuTaskStep::Complete(result) => presentation_result = Some(result),
                        }
                        stage = Some(Stage::Ready(assets));
                        return CpuTaskStep::Continue;
                    }
                    if glue.is_none() {
                        glue = Some(assets.with_read_budget(&read_budget, |store| {
                            solarity_ui::GlueUiSources::load(store, false)
                        }));
                        stage = Some(Stage::Ready(assets));
                        return CpuTaskStep::Continue;
                    }
                    let ui = glue
                        .take()
                        .unwrap_or_else(|| unreachable!("Glue sources prepared"))
                        .map(|glue| PreparedStartupUi {
                            glue,
                            sound: assets.with_read_budget(
                                &read_budget,
                                crate::application::sound_coordinator::PreparedSoundSources::load,
                            ),
                        });
                    return CpuTaskStep::Complete(Ok(PreparedStartup {
                        catalog: catalog
                            .take()
                            .unwrap_or_else(|| unreachable!("discovery precedes catalogs")),
                        assets,
                        catalogs: std::mem::take(&mut pending).finish(),
                        addon_manifest: addon_manifest
                            .take()
                            .unwrap_or_else(|| unreachable!("manifest precedes presentation")),
                        presentation: presentation_result
                            .take()
                            .unwrap_or_else(|| unreachable!("presentation prepared")),
                        ui,
                    }));
                }
            }
        };
        match result {
            Ok(next) => {
                stage = Some(next);
                CpuTaskStep::Continue
            }
            Err(error) => CpuTaskStep::Complete(Err(error)),
        }
    }
}

fn manifest(catalog: &AddonCatalog) -> Result<WorldAddonManifest, StartupCatalogError> {
    Ok(WorldAddonManifest::new(
        catalog
            .addons()
            .iter()
            .map(|addon| {
                WorldAddon::new(
                    addon.name(),
                    addon.is_initially_enabled(),
                    if addon.is_signed() {
                        STANDARD_ADDON_CRC
                    } else {
                        0
                    },
                    0,
                )
            })
            .collect::<Result<Vec<_>, _>>()?,
    )?)
}

#[cfg(test)]
#[path = "../../../tests/application/startup_catalogs.rs"]
mod tests;
