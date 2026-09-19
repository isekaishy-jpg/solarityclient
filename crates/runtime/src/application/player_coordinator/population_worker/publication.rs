//! Exact-lifetime publication and incremental GPU admission retain their original order.

use super::super::{AppearanceWorkerCache, RuntimePlayerError};
use super::{PendingPopulation, PopulationStage, PopulationWorker, PreparedPopulation};
use crate::application::cpu_retirement::CpuRetirementQueue;
use crate::application::terrain_frame::m2::M2GluePipelineWarmup;
use solarity_cpu::CpuExecutor;
use solarity_ecs::{ActiveWorld, WorldObjectIdentity};
use solarity_rendering::{CharacterComponentTextureLevel, M2ModelOrientation, VulkanRenderer};

impl<K, T: PreparedPopulation> PendingPopulation<K, T> {
    /// A cancelled attempt cannot become publishable again when inputs return.
    pub(super) fn withdraw(&mut self) {
        self.withdrawn = true;
        self.stage.retire();
    }
}

impl<K: PartialEq, T: PreparedPopulation> PopulationWorker<K, T> {
    /// Owner/configuration withdrawal remains permanent even if the same key returns.
    pub(in crate::application::player_coordinator) fn withdraw(&mut self) {
        if let Some(pending) = &mut self.pending {
            pending.withdraw();
        }
    }

    /// A stable pending request skips reconstruction of its immutable appearance plan.
    pub(in crate::application::player_coordinator) fn matches_request(
        &self,
        identity: WorldObjectIdentity,
        key: &K,
        level: CharacterComponentTextureLevel,
    ) -> bool {
        self.pending.as_ref().is_some_and(|pending| {
            !pending.withdrawn
                && pending.identity == identity
                && pending.key == *key
                && pending.level == level
        })
    }

    /// Keeps one exclusive cache bank and no initial source or GPU work.
    pub(in crate::application::player_coordinator) fn new() -> Self {
        Self {
            pending: None,
            cache: Some(AppearanceWorkerCache::default()),
            retired: CpuRetirementQueue::new(),
        }
    }

    /// A running request may be inspected, but cannot admit another asset job.
    pub(in crate::application::player_coordinator) fn accepts(
        &self,
        identity: WorldObjectIdentity,
    ) -> bool {
        self.pending
            .as_ref()
            .is_none_or(|pending| pending.identity == identity)
    }

    /// An already-current resident supersedes any outstanding replacement.
    pub(in crate::application::player_coordinator) fn discard_ready(
        &mut self,
        identity: WorldObjectIdentity,
    ) -> Result<(), RuntimePlayerError> {
        if let Some(pending) = &mut self.pending
            && pending.identity == identity
        {
            pending.withdraw();
        }
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.identity == identity && pending.stage.is_finished())
            && let Some(pending) = self.pending.take()
            && let Ok(resident) = pending.stage.into_result(&mut self.cache)?
        {
            self.retired.extend([resident]);
        }
        Ok(())
    }

    /// Detaches large replaced atlases/plans without running their destructors here.
    pub(in crate::application::player_coordinator) fn retire(
        &mut self,
        residents: impl IntoIterator<Item = T>,
    ) {
        self.retired.extend(residents);
    }

    /// Reclaims withdrawn work before a returning appearance can consume its result.
    pub(in crate::application::player_coordinator) fn service(
        &mut self,
        world: Option<&ActiveWorld>,
        cpu: &CpuExecutor,
    ) -> Result<(), RuntimePlayerError> {
        if let Some(pending) = &mut self.pending
            && world.is_none_or(|world| {
                world.object_identity(pending.identity.guid()) != Some(pending.identity)
            })
        {
            pending.withdraw();
        }
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.stage.is_finished() && pending.withdrawn)
            && let Some(pending) = self.pending.take()
            && let Ok(resident) = pending.stage.into_result(&mut self.cache)?
        {
            self.retired.extend([resident]);
        }
        self.retired.service(cpu)?;
        Ok(())
    }

    /// Warms one driver program per service before publishing the exact current appearance.
    pub(in crate::application::player_coordinator) fn take_ready(
        &mut self,
        key: &K,
        level: CharacterComponentTextureLevel,
        renderer: &mut VulkanRenderer,
    ) -> Result<Option<T>, RuntimePlayerError> {
        if let Some(pending) = &mut self.pending
            && (pending.key != *key || pending.level != level)
        {
            pending.withdraw();
        }
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
        if pending.withdrawn {
            if let Ok(resident) = pending.stage.into_result(&mut self.cache)? {
                self.retired.extend([resident]);
            }
            return Ok(None);
        }
        let (resident, mut pipelines) = match pending.stage {
            PopulationStage::Preparing(task) => {
                let completion = task.join()?;
                self.cache = Some(completion.cache);
                let resident = completion.result?;
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
}
