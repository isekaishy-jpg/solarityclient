//! A GameObject publishes its root and default-set doodads only after all source edges complete.

use super::super::world_model::{GameObjectWorldModelDoodad, GameObjectWorldModelSource};
use super::super::{GameObjectResource, ResourceRequest, RuntimeGameObjectError};
use super::state::{GameObjectWorkerCompletion, GameObjectWorkerSource, GameObjectWorkerState};
use crate::application::terrain_coordinator::m2_residency::{
    ResidentM2Source, world_model_doodad_transform,
};
use crate::application::terrain_coordinator::world_model_residency::ResidentWorldModelSource;
use crate::application::terrain_coordinator::{RuntimeTerrainError, SharedTerrainSources};
use solarity_asset::{AssetPath, M2LoadDependency, M2LoadError};
use solarity_cpu::{CpuError, CpuTaskDependency, CpuTaskStep, JobContext};
use std::{collections::HashMap, ops::ControlFlow, sync::Arc};

/// Only this admitted operation mutates its bank; suspended edges retain the same ownership.
#[derive(Default)]
struct Preparation {
    root: Option<ResidentWorldModelSource>,
    pending_root: Option<crate::application::terrain_coordinator::PendingWorldModel>,
    pending_doodad: Option<M2LoadDependency>,
    pending_source: Option<
        crate::application::terrain_coordinator::world_model_residency::WorldModelSourcePreparation,
    >,
    model: Option<solarity_asset::ResourceLease<solarity_asset::DecodedM2Model>>,
    materials: solarity_asset::BlpTexturePreparation,
    indices: Vec<usize>,
    sources: HashMap<AssetPath, Arc<ResidentM2Source>>,
    doodads: Vec<GameObjectWorldModelDoodad>,
}

impl Preparation {
    /// Decode one root or selected MODD source per turn, preserving authored reference order.
    fn step(
        &mut self,
        worker: &mut GameObjectWorkerState,
        request: &ResourceRequest,
        shared: &SharedTerrainSources,
    ) -> Result<
        ControlFlow<GameObjectWorldModelSource, Option<CpuTaskDependency>>,
        RuntimeTerrainError,
    > {
        if self.root.is_none() {
            if self.pending_source.is_none() {
                let model = match shared.world_model(
                    &request.path,
                    &mut self.pending_root,
                    &mut worker.assets,
                )? {
                    ControlFlow::Break(model) => model,
                    ControlFlow::Continue(edge) => return Ok(ControlFlow::Continue(edge)),
                };
                self.pending_source = Some(crate::application::terrain_coordinator::world_model_residency::WorldModelSourcePreparation::new(model, &mut worker.world_models)?);
            }
            let root = match self
                .pending_source
                .as_mut()
                .unwrap_or_else(|| unreachable!("WMO material inputs remain owned"))
                .step(
                    shared,
                    &mut worker.textures,
                    &mut worker.liquid_assets,
                    &mut worker.assets,
                )? {
                ControlFlow::Break(source) => source,
                ControlFlow::Continue(edge) => return Ok(ControlFlow::Continue(Some(edge))),
            };
            self.pending_source = None;
            self.indices = root.model().referenced_active_doodad_indices(0)?;
            self.root = Some(root);
            return Ok(ControlFlow::Continue(None));
        }
        let Some(&index) = self.indices.get(self.doodads.len()) else {
            return Ok(ControlFlow::Break(
                GameObjectWorldModelSource::from_prepared(
                    self.root
                        .take()
                        .unwrap_or_else(|| unreachable!("root precedes default-set completion")),
                    std::mem::take(&mut self.doodads),
                ),
            ));
        };
        let root = self
            .root
            .as_ref()
            .unwrap_or_else(|| unreachable!("doodads follow root publication"));
        let doodad = &root.model().doodads()[index];
        let source = if let Some(source) = self.sources.get(doodad.path()) {
            Arc::clone(source)
        } else {
            if self.model.is_none() {
                match shared.model(doodad.path(), &mut self.pending_doodad, &mut worker.assets)? {
                    ControlFlow::Break(model) => self.model = Some(model),
                    ControlFlow::Continue(edge) => return Ok(ControlFlow::Continue(Some(edge))),
                }
            }
            let model = self
                .model
                .as_ref()
                .unwrap_or_else(|| unreachable!("MODD material inputs remain owned"));
            let source = match shared.materials(
                &mut self.materials,
                &mut worker.textures,
                &mut worker.assets,
                |textures, store| {
                    ResidentM2Source::from_model_with_lights(
                        model.clone(),
                        textures,
                        store,
                        solarity_rendering::M2LocalLightCount::Four,
                    )
                },
            )? {
                ControlFlow::Break(source) => Arc::new(source),
                ControlFlow::Continue(edge) => return Ok(ControlFlow::Continue(Some(edge))),
            };
            self.model = None;
            self.sources
                .insert(doodad.path().clone(), Arc::clone(&source));
            source
        };
        self.doodads.push(GameObjectWorldModelDoodad {
            index,
            source,
            local_transform: world_model_doodad_transform(doodad)?,
        });
        Ok(ControlFlow::Continue(None))
    }
}

/// The same task returns its bank on completion, source failure, or cancellation.
pub(in super::super) fn world_model_steps(
    source: GameObjectWorkerSource,
    request: ResourceRequest,
    shared: SharedTerrainSources,
) -> impl FnMut(&JobContext<'_>) -> CpuTaskStep<GameObjectWorkerCompletion> + Send {
    let mut source = Some(source);
    let mut worker = None;
    let mut preparation = Preparation::default();
    move |context| {
        context.diagnostic_value("game_object.prepare.world_model_step", 1);
        if context.is_cancelled() && worker.is_none() {
            let returned = match source.take() {
                Some(GameObjectWorkerSource::Ready(bank)) => Some(bank),
                _ => None,
            };
            return CpuTaskStep::Complete(GameObjectWorkerCompletion {
                worker: returned,
                result: Err(RuntimeGameObjectError::Cpu(CpuError::JobCancelled)),
            });
        }
        if worker.is_none() {
            worker = match source
                .take()
                .unwrap_or_else(|| unreachable!("one source mounts each owned bank"))
            {
                GameObjectWorkerSource::Ready(worker) => Some(worker),
                GameObjectWorkerSource::Catalog(catalog) => {
                    match GameObjectWorkerState::mount(catalog) {
                        Ok(worker) => Some(Box::new(worker)),
                        Err(error) => {
                            return CpuTaskStep::Complete(GameObjectWorkerCompletion {
                                worker: None,
                                result: Err(M2LoadError::Asset(error).into()),
                            });
                        }
                    }
                }
            };
            return CpuTaskStep::Continue;
        }
        let bank = worker
            .as_mut()
            .unwrap_or_else(|| unreachable!("mounted bank remains owned across suspension"));
        let result = if context.is_cancelled() {
            // Source authority survives withdrawal of its initiating scene.
            // Publish only that root, then return this bank with cancellation.
            if !crate::application::terrain_coordinator::PendingWorldModel::retire_step(
                &mut preparation.pending_root,
                &mut bank.assets,
            ) {
                return CpuTaskStep::Continue;
            }
            Err(RuntimeGameObjectError::Cpu(CpuError::JobCancelled))
        } else {
            match preparation.step(bank, &request, &shared) {
                Ok(ControlFlow::Continue(Some(edge))) => return CpuTaskStep::Wait(edge),
                Ok(ControlFlow::Continue(None)) => return CpuTaskStep::Continue,
                Ok(ControlFlow::Break(source)) => Ok(GameObjectResource::WorldModel(source)),
                Err(error) => Err(error.into()),
            }
        };
        bank.collect_unused();
        CpuTaskStep::Complete(GameObjectWorkerCompletion {
            worker: worker.take(),
            result,
        })
    }
}
