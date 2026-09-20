//! Whole-operation service boundaries preserve terrain publication and cache ownership.

use super::super::{
    ResidentGlobalWorldModel, ResidentTerrainMap, RuntimeTerrainError, TerrainRequest,
    load_low_detail, movement::ResidentMovementScene, streaming::ResidentTileLookup,
    tile_preparation::TilePreparation,
};
use super::{TerrainWorkerCompletion, TerrainWorkerSource, TerrainWorkerState};
use solarity_asset::{AssetMount, AssetStore, MapDefinition, TerrainMap};
use std::ops::ControlFlow;

/// Each state owns everything needed by the next complete asset operation.
enum TerrainStage {
    Start(TerrainWorkerSource),
    Mount(AssetMount),
    Map,
    LowDetail(TerrainMap),
    Content(TerrainMap),
    Tile(TerrainMap, Box<TilePreparation>),
}

/// The same admitted task and captures survive every service turn.
pub(in crate::application::terrain_coordinator) fn terrain_steps(
    source: TerrainWorkerSource,
    definition: MapDefinition,
    request: TerrainRequest,
    specular_textures: bool,
) -> impl FnMut(&solarity_cpu::JobContext<'_>) -> ControlFlow<TerrainWorkerCompletion> + Send {
    let mut stage = Some(TerrainStage::Start(source));
    let mut worker = None;
    move |context| {
        let current = stage.take().unwrap_or_else(|| {
            unreachable!("a completed terrain task cannot execute another step")
        });
        if context.is_cancelled() {
            // A not-yet-started reused bank belongs to this task too. Partial
            // mounting/decoding is retired here on the worker, never published.
            match current {
                TerrainStage::Start(TerrainWorkerSource::Ready(bank)) => worker = Some(bank),
                abandoned => drop(abandoned),
            }
            if let Some(worker) = worker.as_mut() {
                worker.collect_unused();
            }
            context.diagnostic_value("terrain.worker.withdrawn", 1);
            return ControlFlow::Break(TerrainWorkerCompletion {
                worker: worker.take(),
                result: Ok(None),
            });
        }
        match advance(
            current,
            &mut worker,
            &definition,
            request,
            specular_textures,
        ) {
            Ok(ControlFlow::Continue(next)) => {
                stage = Some(next);
                ControlFlow::Continue(())
            }
            result => {
                if let Some(worker) = worker.as_mut() {
                    worker.collect_unused();
                }
                let result = result.map(|done| match done {
                    ControlFlow::Break(resident) => Some(resident),
                    ControlFlow::Continue(_) => unreachable!("continuations return above"),
                });
                ControlFlow::Break(TerrainWorkerCompletion {
                    worker: worker.take(),
                    result,
                })
            }
        }
    }
}

/// Preserves WDT/WDL selection and first-error order without exposing partial tiles.
fn advance(
    stage: TerrainStage,
    worker: &mut Option<Box<TerrainWorkerState>>,
    definition: &MapDefinition,
    request: TerrainRequest,
    specular_textures: bool,
) -> Result<ControlFlow<ResidentTerrainMap, TerrainStage>, RuntimeTerrainError> {
    let stage = match stage {
        TerrainStage::Start(TerrainWorkerSource::Catalog(catalog)) => {
            return Ok(ControlFlow::Continue(TerrainStage::Mount(
                AssetStore::begin_mount(catalog)?,
            )));
        }
        TerrainStage::Start(TerrainWorkerSource::Ready(ready)) => {
            *worker = Some(ready);
            return Ok(ControlFlow::Continue(TerrainStage::Map));
        }
        TerrainStage::Mount(mount) => {
            return Ok(ControlFlow::Continue(match mount.advance()? {
                ControlFlow::Continue(next) => TerrainStage::Mount(next),
                ControlFlow::Break(assets) => {
                    *worker = Some(Box::new(TerrainWorkerState::new(assets)));
                    TerrainStage::Map
                }
            }));
        }
        ready => ready,
    };
    let worker = worker.as_mut().unwrap_or_else(|| {
        unreachable!("terrain decoding follows successful archive-bank ownership")
    });
    let next = match stage {
        TerrainStage::Map => {
            TerrainStage::LowDetail(TerrainMap::load(&mut worker.assets, definition)?)
        }
        TerrainStage::LowDetail(terrain) => {
            if worker
                .low_detail
                .as_ref()
                .is_none_or(|(id, _)| *id != request.map_id)
            {
                worker.low_detail = Some((
                    request.map_id,
                    load_low_detail(&mut worker.assets, &terrain)?,
                ));
            }
            TerrainStage::Content(terrain)
        }
        TerrainStage::Content(terrain) => {
            if let Some(placement) = terrain.global_world_model() {
                // WMO preparation still owns its nested decode/registration order.
                let global_world_model = ResidentGlobalWorldModel::prepare(
                    placement,
                    &mut worker.textures,
                    &mut worker.models,
                    &mut worker.world_models,
                    &mut worker.liquid_assets,
                    &mut worker.assets,
                )?;
                return Ok(ControlFlow::Break(resident(
                    worker,
                    terrain,
                    None,
                    Some(global_world_model),
                )));
            }
            if !terrain.tile(request.tile).exists() {
                return Err(RuntimeTerrainError::MissingPlayerTile {
                    map_id: request.map_id,
                    tile_x: request.tile.x(),
                    tile_y: request.tile.y(),
                });
            }
            let decoded = terrain.load_tile(&mut worker.assets, request.tile)?;
            TerrainStage::Tile(terrain, TilePreparation::new(decoded, specular_textures))
        }
        TerrainStage::Tile(terrain, pending) => {
            match pending.advance(
                &mut worker.ground_detail_assets,
                &mut worker.liquid_assets,
                &mut worker.textures,
                &mut worker.models,
                &mut worker.world_models,
                &mut worker.assets,
            )? {
                ControlFlow::Continue(next) => TerrainStage::Tile(terrain, next),
                ControlFlow::Break(tile) => {
                    return Ok(ControlFlow::Break(resident(
                        worker,
                        terrain,
                        Some(tile),
                        None,
                    )));
                }
            }
        }
        TerrainStage::Start(_) | TerrainStage::Mount(_) => {
            unreachable!("mount stages return before decoding")
        }
    };
    Ok(ControlFlow::Continue(next))
}

/// Neighbor membership and movement publication remain main-thread decisions.
fn resident(
    worker: &TerrainWorkerState,
    terrain: TerrainMap,
    tile: Option<super::super::ResidentTerrainTile>,
    global_world_model: Option<ResidentGlobalWorldModel>,
) -> ResidentTerrainMap {
    ResidentTerrainMap {
        terrain,
        low_detail: worker.low_detail.as_ref().and_then(|(_, map)| map.clone()),
        tile,
        global_world_model,
        nearby: Vec::new(),
        nearby_lookup: ResidentTileLookup::default(),
        movement: ResidentMovementScene::default(),
    }
}
