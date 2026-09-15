//! Each selected appearance attempt returns to the service queue before another starts.

use super::super::{
    GlueCharacterWorkerFailure, GlueCharacterWorkerRequest, ResidentGlueCharacterKey,
    ResidentGlueCharacterModel, RuntimePlayerError, RuntimePlayerSharedCatalogs,
};
use super::{GlueCharacterWorkerCache, GlueWorkerCompletion, with_worker_presentation};
use solarity_asset::ArchiveCatalog;
use solarity_rendering::CharacterComponentTextureLevel;
use std::{
    ops::ControlFlow,
    sync::{Arc, Mutex},
};

/// Completes a Glue character on the same private asset owner used by population jobs.
fn prepare_glue_character_on_worker(
    catalog: ArchiveCatalog,
    catalogs: RuntimePlayerSharedCatalogs,
    component_texture_level: CharacterComponentTextureLevel,
    key: ResidentGlueCharacterKey,
    worker_cache: &mut GlueCharacterWorkerCache,
) -> Result<ResidentGlueCharacterModel, RuntimePlayerError> {
    with_worker_presentation(
        catalog,
        catalogs,
        component_texture_level,
        worker_cache,
        |presentation| {
            presentation.requested_glue_character = Some(key.clone());
            match &key {
                ResidentGlueCharacterKey::Creation(preview) => {
                    presentation.synchronize_character_creation(Some(preview))?;
                }
                ResidentGlueCharacterKey::Selection(preview) => {
                    presentation.synchronize_character_selection(Some(preview))?;
                }
            }
            presentation
                .glue_character
                .take()
                .ok_or(RuntimePlayerError::MissingGlueCharacterWorkerResult)
        },
    )
}

/// Owns the cache through every yield and returns it on every ordinary terminal result.
pub(in crate::application::player_coordinator) fn glue_steps(
    catalog: ArchiveCatalog,
    catalogs: RuntimePlayerSharedCatalogs,
    request: Arc<Mutex<Option<GlueCharacterWorkerRequest>>>,
    cache: GlueCharacterWorkerCache,
) -> impl FnMut() -> ControlFlow<GlueWorkerCompletion> + Send {
    let mut cache = Some(cache);
    move || {
        let step = coalesced_step(&request, |work| {
            prepare_glue_character_on_worker(
                catalog.clone(),
                catalogs.clone(),
                work.component_texture_level,
                work.key.clone(),
                cache.as_mut().unwrap_or_else(|| {
                    unreachable!("only an unfinished appearance task owns its cache")
                }),
            )
        });
        step.map_break(|result| GlueWorkerCompletion {
            cache: cache.take().unwrap_or_else(|| {
                unreachable!("terminal appearance publication returns its cache once")
            }),
            result: result.map(|resident| {
                resident.map(|(key, mut resident)| {
                    resident.apply_transform_key(key);
                    resident
                })
            }),
        })
    }
}

/// Reads selected input once, runs one preparation outside metadata locks, and revalidates it.
/// A changed residency yields even on failure; facing-only changes reuse the prepared result.
/// The caller owns result disposal and the cache, including when the request is withdrawn.
fn coalesced_step<T>(
    request: &Mutex<Option<GlueCharacterWorkerRequest>>,
    prepare: impl FnOnce(&GlueCharacterWorkerRequest) -> Result<T, RuntimePlayerError>,
) -> ControlFlow<Result<Option<(ResidentGlueCharacterKey, T)>, GlueCharacterWorkerFailure>> {
    let Some(work) = request
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
    else {
        return ControlFlow::Break(Ok(None));
    };
    let result = prepare(&work);
    let current = request
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    match current {
        Some(current) if current.same_residency(&work) => ControlFlow::Break(
            result
                .map(|resident| Some((current.key, resident)))
                .map_err(|source| GlueCharacterWorkerFailure {
                    key: work.key,
                    source,
                }),
        ),
        Some(_) => ControlFlow::Continue(()),
        None => ControlFlow::Break(Ok(None)),
    }
}

#[cfg(test)]
#[path = "../../../../tests/application/glue_worker_steps.rs"]
mod tests;
