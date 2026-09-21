//! Archive opening and each effect source yield without retaining a waiting worker.

use super::preparation::Preparation;
use crate::application::terrain_coordinator::SharedTerrainSources;
use crate::application::terrain_frame::m2::unit_effects::PreparedUnitEffects;
use crate::application::{RuntimeTerrainError, archive_job::prepare_archive_resumable};
use solarity_asset::{ArchiveCatalog, EnvironmentalDamageCatalog};
use solarity_cpu::{CpuError, CpuTaskStep, JobContext};
use std::{ops::ControlFlow, sync::Arc};

/// Domain order and error policy survive suspension; cancellation discards only this consumer.
pub(super) fn source_steps(
    catalog: ArchiveCatalog,
    environmental: Arc<EnvironmentalDamageCatalog>,
    shared: SharedTerrainSources,
) -> impl FnMut(&JobContext<'_>) -> CpuTaskStep<Result<PreparedUnitEffects, RuntimeTerrainError>> + Send
{
    let mut preparation = None;
    let mut operation = prepare_archive_resumable(catalog, move |store| {
        let progress = store.with_read_budget(&shared.read_budget(), |store| {
            if preparation.is_none() {
                preparation = Some(Preparation::new(store, &environmental, &shared.budget)?);
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
            return CpuTaskStep::Complete(Err(CpuError::JobCancelled.into()));
        }
        operation()
    }
}

#[cfg(test)]
#[path = "../../../../tests/application/unit_effect_sources.rs"]
mod tests;

#[cfg(test)]
pub(in crate::application) use tests::prepare_sources_for_test;
