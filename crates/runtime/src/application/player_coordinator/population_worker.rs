//! Finite population asset jobs with exact object and appearance ownership.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use solarity_asset::ArchiveCatalog;
use solarity_cpu::{CpuError, CpuExecutor, CpuTask};
use solarity_ecs::{ActiveWorld, WorldObjectIdentity};
use solarity_rendering::{CharacterComponentTextureLevel, M2ModelOrientation, VulkanRenderer};

use super::{
    GlueCharacterWorkerCache, ResidentCreatureModel, ResidentPlayerModel, RuntimePlayerError,
    RuntimePlayerPresentation, RuntimePlayerSharedCatalogs, UnitPresentationGeneration,
    with_worker_presentation,
};
use crate::application::cpu_retirement::CpuRetirementQueue;
use crate::application::terrain_frame::m2::M2GluePipelineWarmup;

/// A population owns one archive/cache worker; no per-object task backlog exists.
pub(super) struct PopulationWorker<K, T> {
    pending: Option<PendingPopulation<K, T>>,
    cache: Arc<Mutex<GlueCharacterWorkerCache>>,
    retired: CpuRetirementQueue<T>,
}

/// The request key remains available even when preparation returns an asset error.
struct PendingPopulation<K, T> {
    identity: WorldObjectIdentity,
    key: K,
    level: CharacterComponentTextureLevel,
    stage: PopulationStage<T>,
}

/// Immutable admission identity, separate from the worker's asset providers.
pub(super) struct PopulationRequest<K> {
    pub(super) identity: WorldObjectIdentity,
    pub(super) key: K,
    pub(super) level: CharacterComponentTextureLevel,
}

/// CPU completion precedes incremental driver admission and atomic placement publication.
enum PopulationStage<T> {
    Preparing(CpuTask<Result<T, RuntimePlayerError>>),
    Warming {
        resident: T,
        pipelines: VecDeque<M2GluePipelineWarmup>,
    },
}

impl<T> PopulationStage<T> {
    fn is_finished(&self) -> bool {
        match self {
            Self::Preparing(task) => task.is_finished(),
            Self::Warming { .. } => true,
        }
    }

    /// Used only after completion, including when an obsolete owner must retire.
    fn into_result(self) -> Result<Result<T, RuntimePlayerError>, CpuError> {
        match self {
            Self::Preparing(task) => task.join(),
            Self::Warming { resident, .. } => Ok(Ok(resident)),
        }
    }
}

/// Each prepared unit owns the exact CPU sources its GPU generation will consume.
pub(super) trait PreparedPopulation: Send + 'static {
    fn generation(&self) -> &UnitPresentationGeneration;
}

impl PreparedPopulation for ResidentCreatureModel {
    fn generation(&self) -> &UnitPresentationGeneration {
        &self.generation
    }
}

impl PreparedPopulation for ResidentPlayerModel {
    fn generation(&self) -> &UnitPresentationGeneration {
        &self.generation
    }
}

/// Loading ownership is explicit at synchronous fixture and live renderer call sites.
pub(super) enum PopulationLoading<'a> {
    Synchronous,
    Asynchronous {
        cpu: &'a CpuExecutor,
        renderer: &'a mut VulkanRenderer,
    },
}

impl PopulationLoading<'_> {
    pub(super) fn is_asynchronous(&self) -> bool {
        matches!(self, Self::Asynchronous { .. })
    }
}

impl<K: PartialEq, T: PreparedPopulation> PopulationWorker<K, T> {
    pub(super) fn new() -> Self {
        Self {
            pending: None,
            cache: Arc::new(Mutex::new(GlueCharacterWorkerCache::default())),
            retired: CpuRetirementQueue::new(),
        }
    }

    /// A running request may be inspected, but cannot admit another asset job.
    pub(super) fn accepts(&self, identity: WorldObjectIdentity) -> bool {
        self.pending
            .as_ref()
            .is_none_or(|pending| pending.identity == identity)
    }

    /// An already-current resident supersedes any outstanding replacement.
    pub(super) fn discard_ready(
        &mut self,
        identity: WorldObjectIdentity,
    ) -> Result<(), RuntimePlayerError> {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.identity == identity && pending.stage.is_finished())
            && let Some(pending) = self.pending.take()
            && let Ok(resident) = pending.stage.into_result()?
        {
            self.retired.extend([resident]);
        }
        Ok(())
    }

    /// Detaches large replaced atlases/plans without running their destructors here.
    pub(super) fn retire(&mut self, residents: impl IntoIterator<Item = T>) {
        self.retired.extend(residents);
    }

    /// Retires completed work whose exact ECS lifetime no longer exists.
    pub(super) fn service(
        &mut self,
        world: Option<&ActiveWorld>,
        cpu: &CpuExecutor,
    ) -> Result<(), RuntimePlayerError> {
        if self.pending.as_ref().is_some_and(|pending| {
            pending.stage.is_finished()
                && world.is_none_or(|world| {
                    world.object_identity(pending.identity.guid()) != Some(pending.identity)
                })
        }) && let Some(pending) = self.pending.take()
            && let Ok(resident) = pending.stage.into_result()?
        {
            self.retired.extend([resident]);
        }
        self.retired.service(cpu)?;
        Ok(())
    }

    /// Warms one driver program per service before publishing the exact current appearance.
    pub(super) fn take_ready(
        &mut self,
        key: &K,
        level: CharacterComponentTextureLevel,
        renderer: &mut VulkanRenderer,
    ) -> Result<Option<T>, RuntimePlayerError> {
        if !self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.stage.is_finished())
        {
            return Ok(None);
        }
        let Some(mut pending) = self.pending.take() else {
            return Ok(None);
        };
        if pending.key != *key || pending.level != level {
            if let Ok(resident) = pending.stage.into_result()? {
                self.retired.extend([resident]);
            }
            return Ok(None);
        }
        let (resident, mut pipelines) = match pending.stage {
            PopulationStage::Preparing(task) => {
                let resident = task.join()??;
                let pipelines = resident
                    .generation()
                    .0
                    .iter()
                    .map(|source| M2GluePipelineWarmup::new(source, M2ModelOrientation::Authored))
                    .collect();
                (resident, pipelines)
            }
            PopulationStage::Warming {
                resident,
                pipelines,
            } => (resident, pipelines),
        };
        while let Some(front) = pipelines.front_mut() {
            if front.is_resident(renderer) {
                pipelines.pop_front();
                continue;
            }
            if front
                .service_one(renderer)
                .map_err(|error| RuntimePlayerError::RenderPreparation(Box::new(error)))?
            {
                pipelines.pop_front();
            }
            break;
        }
        if pipelines.is_empty() {
            return Ok(Some(resident));
        }
        pending.stage = PopulationStage::Warming {
            resident,
            pipelines,
        };
        self.pending = Some(pending);
        Ok(None)
    }

    /// Reserves capacity before transferring work to a private archive owner.
    pub(super) fn submit(
        &mut self,
        cpu: &CpuExecutor,
        request: PopulationRequest<K>,
        catalog: ArchiveCatalog,
        catalogs: RuntimePlayerSharedCatalogs,
        prepare: impl FnOnce(&mut RuntimePlayerPresentation) -> Result<T, RuntimePlayerError>
        + Send
        + 'static,
    ) -> Result<(), RuntimePlayerError> {
        if self.pending.is_some() {
            return Ok(());
        }
        let permit = match cpu.try_reserve() {
            Ok(permit) => permit,
            Err(CpuError::AtCapacity { .. }) => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        let cache = Arc::clone(&self.cache);
        let PopulationRequest {
            identity,
            key,
            level,
        } = request;
        self.pending = Some(PendingPopulation {
            identity,
            key,
            level,
            stage: PopulationStage::Preparing(permit.submit(move || {
                with_worker_presentation(catalog, catalogs, level, &cache, prepare)
            })),
        });
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../../tests/application/population_worker.rs"]
mod tests;
