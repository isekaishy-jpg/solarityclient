//! Whole-operation service boundaries preserve terrain publication and cache ownership.

use super::super::{
    ResidentGlobalWorldModel, ResidentTerrainMap, RuntimeTerrainError, TerrainRequest,
    load_low_detail,
    m2_residency::ResidentM2SceneBuilder,
    movement::ResidentMovementScene,
    streaming::ResidentTileLookup,
    tile_preparation::{SharedTerrainSources, TilePreparation},
    world_model_residency::WorldModelPreparation,
};
use super::{TerrainWorkerCompletion, TerrainWorkerSource, TerrainWorkerState};
use solarity_asset::{AssetMount, AssetStore, MapDefinition, TerrainMap};
use solarity_cpu::CpuTaskStep;
use std::ops::ControlFlow;

/// Each state owns everything needed by the next complete asset operation.
enum TerrainStage {
    Start(TerrainWorkerSource),
    Mount(AssetMount),
    Map,
    LowDetail(TerrainMap),
    Content(TerrainMap),
    Tile(TerrainMap, Box<TilePreparation>),
    Global(TerrainMap, Box<GlobalPreparation>),
}

/// Global WMO admission retains its nested M2 builder across readiness edges.
struct GlobalPreparation {
    models: Box<WorldModelPreparation>,
    m2: ResidentM2SceneBuilder,
    dependency: Option<solarity_cpu::CpuTaskDependency>,
}

/// The same admitted task and captures survive every service turn.
pub(in crate::application::terrain_coordinator) fn terrain_steps(
    source: TerrainWorkerSource,
    definition: MapDefinition,
    request: TerrainRequest,
    specular_textures: bool,
    shared: SharedTerrainSources,
) -> impl FnMut(&solarity_cpu::JobContext<'_>) -> CpuTaskStep<TerrainWorkerCompletion> + Send {
    let mut stage = Some(TerrainStage::Start(source));
    let mut worker: Option<Box<TerrainWorkerState>> = None;
    move |context| {
        let mut current = stage.take().unwrap_or_else(|| {
            unreachable!("a completed terrain task cannot execute another step")
        });
        if context.is_cancelled() {
            // Consumer withdrawal cannot abandon a shared root already claimed
            // by this task. Drain that finite producer without building the tile.
            if let Some(bank) = worker.as_mut() {
                let retired = match &mut current {
                    TerrainStage::Tile(_, tile) => tile.retire_source_step(&mut bank.assets),
                    TerrainStage::Global(_, global) => {
                        global.models.retire_source_step(&mut bank.assets)
                    }
                    _ => true,
                };
                if !retired {
                    stage = Some(current);
                    return CpuTaskStep::Continue;
                }
            }
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
            return CpuTaskStep::Complete(TerrainWorkerCompletion {
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
            &shared,
        ) {
            Ok(ControlFlow::Continue(mut next)) => {
                let dependency = match &mut next {
                    TerrainStage::Tile(_, tile) => tile.take_dependency(),
                    TerrainStage::Global(_, global) => global.dependency.take(),
                    _ => None,
                };
                stage = Some(next);
                dependency.map_or(CpuTaskStep::Continue, CpuTaskStep::Wait)
            }
            result => {
                if let Some(worker) = worker.as_mut() {
                    worker.collect_unused();
                }
                let result = result.map(|done| match done {
                    ControlFlow::Break(resident) => Some(resident),
                    ControlFlow::Continue(_) => unreachable!("continuations return above"),
                });
                CpuTaskStep::Complete(TerrainWorkerCompletion {
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
    shared: &SharedTerrainSources,
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
    let budget = shared.read_budget();
    let next = match stage {
        TerrainStage::Map => TerrainStage::LowDetail(
            worker
                .assets
                .with_read_budget(&budget, |store| TerrainMap::load(store, definition))?,
        ),
        TerrainStage::LowDetail(terrain) => {
            if worker
                .low_detail
                .as_ref()
                .is_none_or(|(id, _)| *id != request.map_id)
            {
                worker.low_detail = Some((
                    request.map_id,
                    worker
                        .assets
                        .with_read_budget(&budget, |store| load_low_detail(store, &terrain))?,
                ));
            }
            TerrainStage::Content(terrain)
        }
        TerrainStage::Content(terrain) => {
            if let Some(placement) = terrain.global_world_model() {
                let models = WorldModelPreparation::for_global(placement);
                return Ok(ControlFlow::Continue(TerrainStage::Global(
                    terrain,
                    Box::new(GlobalPreparation {
                        models,
                        m2: ResidentM2SceneBuilder::new(),
                        dependency: None,
                    }),
                )));
            }
            if !terrain.tile(request.tile).exists() {
                return Err(RuntimeTerrainError::MissingPlayerTile {
                    map_id: request.map_id,
                    tile_x: request.tile.x(),
                    tile_y: request.tile.y(),
                });
            }
            let decoded = worker
                .assets
                .with_read_budget(&budget, |store| terrain.load_tile(store, request.tile))?;
            TerrainStage::Tile(
                terrain,
                TilePreparation::with_shared_sources(decoded, specular_textures, shared.clone()),
            )
        }
        TerrainStage::Tile(terrain, pending) => {
            match worker.assets.with_read_budget(&budget, |store| {
                pending.advance(
                    &mut worker.ground_detail_assets,
                    &mut worker.liquid_assets,
                    &mut worker.textures,
                    &mut worker.models,
                    &mut worker.world_models,
                    store,
                )
            })? {
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
        TerrainStage::Global(terrain, mut pending) => {
            if worker.assets.with_read_budget(&budget, |store| {
                pending.models.advance(
                    &mut worker.world_models,
                    &mut worker.models,
                    &mut worker.textures,
                    &mut pending.m2,
                    &mut worker.liquid_assets,
                    store,
                    Some(shared),
                    &mut pending.dependency,
                )
            })? {
                let (models, collision, liquids) = pending.models.finish();
                let global =
                    ResidentGlobalWorldModel::from_models(pending.m2, models, collision, liquids);
                return Ok(ControlFlow::Break(resident(
                    worker,
                    terrain,
                    None,
                    Some(global),
                )));
            }
            TerrainStage::Global(terrain, pending)
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
