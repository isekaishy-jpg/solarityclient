//! Pure receiver batches share frozen inputs and own disjoint preallocated outputs.

use super::{ReceiverInputs, RuntimeTerrainFrameError, SceneLightSources};
use solarity_rendering::{M2DirectionalLight, M2SceneUniform};
use std::{ops::Range, sync::Arc};

/// A reader pin is returned on every terminal/admission path before main mutates inputs.
pub(super) struct LightingInput {
    pub sources: Arc<SceneLightSources>,
    pub receivers: Arc<ReceiverInputs>,
    pub range: Range<usize>,
    pub base: M2SceneUniform,
    pub exterior: M2DirectionalLight,
}

/// One contiguous receiver range; no receiver/placement bank is cloned for a batch.
#[derive(Default)]
pub(super) struct LightingWork {
    pub measurement: solarity_cpu::WorkMeasurement,
    pub input: Option<LightingInput>,
    pub scenes: Vec<M2SceneUniform>,
    pub result: Option<Result<(), RuntimeTerrainFrameError>>,
}

impl LightingWork {
    /// Admitted receiver ranges keep their owned output through terminal publication.
    pub(super) fn run(
        &mut self,
        context: &solarity_cpu::JobContext<'_>,
    ) -> solarity_cpu::JobOutcome {
        context.diagnostic_value(
            "m2.lighting.receivers",
            self.input.as_ref().map_or(0, |input| input.range.len()) as u64,
        );
        self.execute()
    }

    /// Output was reserved by main before dispatch; worker evaluation cannot grow it.
    fn evaluate(&mut self) -> Result<(), RuntimeTerrainFrameError> {
        let input = self
            .input
            .as_ref()
            .unwrap_or_else(|| unreachable!("receiver batch pins its published inputs"));
        input.sources.trace.link("m2.light_sources.receiver");
        solarity_profiling::profile_value!("m2.scene_lighting.batch_receivers", input.range.len());
        let _trace = solarity_profiling::TraceSpan::new(
            "m2.receiver_batch",
            input.range.start as u64,
            input.range.len() as u64,
        );
        let mut output = solarity_cpu::FixedWriter::new(&mut self.scenes);
        for index in input.range.clone() {
            output.push(input.receivers.evaluate(
                index,
                &input.sources,
                input.base,
                input.exterior,
            )?)?;
        }
        Ok(())
    }

    /// Retains domain failure with the ordered output prefix, independent of execution order.
    pub(super) fn execute(&mut self) -> solarity_cpu::JobOutcome {
        let started = self.measurement.start();
        let _profile = solarity_profiling::profile!("m2.scene_lighting.evaluate");
        self.result = Some(self.evaluate());
        if self.result.as_ref().is_some_and(Result::is_ok) {
            self.measurement.finish(started);
            solarity_cpu::JobOutcome::Succeeded
        } else {
            solarity_cpu::JobOutcome::Failed
        }
    }
}
