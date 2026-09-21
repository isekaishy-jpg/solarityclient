//! Whole effect banks publish after their shared sources and GPU warmup complete.

use super::RuntimeUnitEffects;
use crate::application::terrain_coordinator::SharedTerrainSources;
use crate::application::terrain_frame::m2::unit_effects::{
    M2UnitEffectSources, M2UnitEffectWarmup, PreparedUnitEffects, UnitEffectPreparation,
};
use crate::application::{ApplicationError, RuntimeTerrainError};
use solarity_asset::{ArchiveCatalog, EnvironmentalDamageCatalog};
use solarity_cpu::{CpuExecutor, CpuTask, CpuTaskStep, JobContext};
use std::{ops::ControlFlow, sync::Arc};

/// Partial readers and models belong to the running task until complete publication.
pub(super) enum Sources {
    Deferred(ArchiveCatalog),
    Running(CpuTask<Result<PreparedUnitEffects, RuntimeTerrainError>>),
    Warming(Box<M2UnitEffectWarmup>),
    Ready(Arc<M2UnitEffectSources>),
}

impl RuntimeUnitEffects {
    /// Worker decoding and one driver pipeline per service tick precede callbacks.
    pub(in crate::application) fn service_sources(
        &mut self,
        cpu: &CpuExecutor,
        renderer: &mut solarity_rendering::GpuPreparation<'_>,
    ) -> Result<(), ApplicationError> {
        self.sources =
            Some(
                match self
                    .sources
                    .take()
                    .ok_or(ApplicationError::UnitEffectPreparationFailed)?
                {
                    Sources::Deferred(catalog) if cpu.can_admit_speculative()? => {
                        match cpu.try_reserve() {
                            Ok(permit) => {
                                let shared =
                                    crate::application::terrain_coordinator::SharedTerrainSources {
                                        budget: cpu.storage().clone(),
                                        service: permit.service_control(),
                                    };
                                Sources::Running(permit.submit_resumable_with_context(
                                    source_steps(catalog, Arc::clone(&self.environmental), shared),
                                ))
                            }
                            Err(solarity_cpu::CpuError::AtCapacity { .. }) => {
                                Sources::Deferred(catalog)
                            }
                            Err(error) => return Err(error.into()),
                        }
                    }
                    Sources::Running(task) if task.is_finished() => {
                        Sources::Warming(Box::new(M2UnitEffectWarmup::new(task.join()??)))
                    }
                    Sources::Warming(mut warmup) => {
                        if warmup.service_one(renderer)? {
                            Sources::Ready(Arc::new(warmup.into_sources()))
                        } else {
                            Sources::Warming(warmup)
                        }
                    }
                    state => state,
                },
            );
        Ok(())
    }

    pub(in crate::application) fn sources(&self) -> Option<Arc<M2UnitEffectSources>> {
        match &self.sources {
            Some(Sources::Ready(sources)) => Some(Arc::clone(sources)),
            _ => None,
        }
    }
}

/// Each resume uses the same archive bank and ordered definition cursor. A
/// withdrawn consumer drops its readiness interest on the worker; another
/// consumer's shared producer continues independently.
fn source_steps(
    catalog: ArchiveCatalog,
    environmental: Arc<EnvironmentalDamageCatalog>,
    shared: SharedTerrainSources,
) -> impl FnMut(&JobContext<'_>) -> CpuTaskStep<Result<PreparedUnitEffects, RuntimeTerrainError>> + Send
{
    let mut preparation = None;
    let mut operation =
        crate::application::archive_job::prepare_archive_resumable(catalog, move |store| {
            let progress = store.with_read_budget(&shared.read_budget(), |store| {
                if preparation.is_none() {
                    preparation = Some(UnitEffectPreparation::begin(
                        store,
                        &environmental,
                        &shared.budget,
                    )?);
                    return Ok(ControlFlow::Continue(None));
                }
                preparation
                    .as_mut()
                    .unwrap_or_else(|| unreachable!("effect preparation initialized"))
                    .step(store, &shared)
            });
            match progress {
                Ok(ControlFlow::Continue(edge)) => {
                    edge.map_or(CpuTaskStep::Continue, CpuTaskStep::Wait)
                }
                Ok(ControlFlow::Break(effects)) => CpuTaskStep::Complete(Ok(effects)),
                Err(error) => CpuTaskStep::Complete(Err(error)),
            }
        });
    let mut step = 0_u64;
    move |context| {
        context.diagnostic_value("unit_effects.source_step", step);
        step = step.saturating_add(1);
        if context.is_cancelled() {
            return CpuTaskStep::Complete(Err(solarity_cpu::CpuError::JobCancelled.into()));
        }
        operation()
    }
}

#[cfg(test)]
#[path = "../../../tests/application/unit_effect_source_task.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/application/unit_effect_sources.rs"]
mod accounting_tests;

#[cfg(test)]
pub(in crate::application) use accounting_tests::prepare_sources_for_test;
