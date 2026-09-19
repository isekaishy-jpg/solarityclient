//! Finite population asset jobs with exact object and appearance ownership.

mod admission;
mod pending;
mod publication;

use std::collections::VecDeque;

use solarity_asset::AssetPath;
use solarity_cpu::CpuExecutor;
use solarity_ecs::WorldObjectIdentity;
use solarity_rendering::{CharacterComponentTextureLevel, VulkanRenderer};

use super::{
    GlueCharacterWorkerCache, ResidentCreatureModel, ResidentPlayerModel,
    UnitPresentationGeneration,
};
use crate::application::cpu_retirement::CpuRetirementQueue;
use crate::application::terrain_frame::m2::M2GluePipelineWarmup;

/// A population owns one archive/cache worker; no per-object task backlog exists.
pub(super) struct PopulationWorker<K, T: PreparedPopulation> {
    pending: Option<PendingPopulation<K, T>>,
    cache: Option<GlueCharacterWorkerCache>,
    retired: CpuRetirementQueue<T>,
}

/// The request key remains available even when preparation returns an asset error.
struct PendingPopulation<K, T: PreparedPopulation> {
    identity: WorldObjectIdentity,
    key: K,
    level: CharacterComponentTextureLevel,
    /// Withdrawal is permanent even if the same appearance becomes current again.
    withdrawn: bool,
    stage: PopulationStage<T>,
}

/// Immutable admission identity, separate from the worker's asset providers.
pub(super) struct PopulationRequest<K> {
    pub(super) identity: WorldObjectIdentity,
    pub(super) key: K,
    pub(super) level: CharacterComponentTextureLevel,
    pub(super) model_path: AssetPath,
}

/// CPU completion precedes incremental driver admission and atomic placement publication.
enum PopulationStage<T: PreparedPopulation> {
    Preparing(pending::PendingTask<T>),
    Warming {
        resident: T,
        pipelines: VecDeque<M2GluePipelineWarmup>,
    },
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

#[cfg(test)]
#[path = "../../../../tests/application/population_worker.rs"]
mod tests;
