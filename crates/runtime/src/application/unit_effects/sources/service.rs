//! CPU source completion precedes incremental GPU warmup and publication.

use crate::application::terrain_coordinator::SharedTerrainSources;
use crate::application::terrain_frame::m2::unit_effects::{
    M2UnitEffectSources, M2UnitEffectWarmup, PreparedUnitEffects,
};
use crate::application::{ApplicationError, RuntimeTerrainError};
use solarity_asset::{ArchiveCatalog, EnvironmentalDamageCatalog};
use solarity_cpu::{CpuExecutor, CpuTask};
use solarity_rendering::VulkanRenderer;
use std::sync::Arc;

/// Only complete source banks enter the existing GPU preparation sequence.
pub(in crate::application::unit_effects) enum Sources {
    Deferred(ArchiveCatalog),
    Running(CpuTask<Result<PreparedUnitEffects, RuntimeTerrainError>>),
    Warming(Box<M2UnitEffectWarmup>),
    Ready(Arc<M2UnitEffectSources>),
}

impl Sources {
    /// Admission refusal preserves the selected catalog; errors retain existing terminal policy.
    pub(in crate::application::unit_effects) fn service(
        self,
        cpu: &CpuExecutor,
        renderer: &mut VulkanRenderer,
        environmental: &Arc<EnvironmentalDamageCatalog>,
    ) -> Result<Self, ApplicationError> {
        Ok(match self {
            Self::Deferred(catalog) if cpu.can_admit_speculative()? => {
                match cpu.try_reserve() {
                    Ok(permit) => {
                        let shared = SharedTerrainSources {
                            budget: cpu.storage().clone(),
                            service: permit.service_control(),
                        };
                        Self::Running(permit.submit_resumable_with_context(
                            super::steps::source_steps(catalog, Arc::clone(environmental), shared),
                        ))
                    }
                    Err(solarity_cpu::CpuError::AtCapacity { .. }) => Self::Deferred(catalog),
                    Err(error) => return Err(error.into()),
                }
            }
            Self::Running(task) if task.is_finished() => {
                Self::Warming(Box::new(M2UnitEffectWarmup::new(task.join()??)))
            }
            Self::Warming(mut warmup) => {
                if warmup.service_one(renderer)? {
                    Self::Ready(Arc::new(warmup.into_sources()))
                } else {
                    Self::Warming(warmup)
                }
            }
            state => state,
        })
    }
}
