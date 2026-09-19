//! Population completion restores its exclusive cache before publication.

use super::super::{AppearanceWorkerCache, RuntimePlayerError};
use super::{PopulationStage, PreparedPopulation};
use solarity_cpu::CpuError;

impl<T: PreparedPopulation> PopulationStage<T> {
    /// Only unfinished CPU work needs withdrawal; warming output is already owned.
    pub(super) fn retire(&mut self) {
        match self {
            Self::Preparing(task) => task.retire(),
            Self::Warming { .. } => {}
        }
    }

    pub(super) fn is_finished(&self) -> bool {
        match self {
            Self::Preparing(task) => task.is_finished(),
            Self::Warming { .. } => true,
        }
    }

    /// Used only after completion, including when an obsolete owner must retire.
    pub(super) fn into_result(
        self,
        cache: &mut Option<AppearanceWorkerCache>,
    ) -> Result<Result<T, RuntimePlayerError>, CpuError> {
        match self {
            Self::Preparing(task) => {
                let completion = task.join()?;
                *cache = Some(completion.cache);
                Ok(completion.result)
            }
            Self::Warming { resident, .. } => Ok(Ok(resident)),
        }
    }
}
