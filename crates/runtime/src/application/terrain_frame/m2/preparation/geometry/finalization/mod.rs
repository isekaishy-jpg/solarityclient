//! Owned final geometry assembly and ordering on the shared CPU executor.

mod admission;
mod batch;
mod ordering;
mod streams;

use super::super::super::RuntimeTerrainFrameError;
use super::super::diagnostics::Work;
use super::GeometryOwner;
use solarity_cpu::{CpuOwnedCell, FrameBatch};
use streams::FinalStreams;

/// One retained operation owns outputs only between capture and explicit return.
pub(super) struct Finalization {
    pending: FrameBatch<CpuOwnedCell<FinalizationJob>>,
    retained: Vec<CpuOwnedCell<FinalizationJob>>,
    submitted: bool,
    owns_inputs: bool,
}

impl Default for Finalization {
    fn default() -> Self {
        Self {
            pending: FrameBatch::with_context(FinalizationJob::execute),
            retained: Vec::new(),
            submitted: false,
            owns_inputs: false,
        }
    }
}

/// Jobs and contiguous renderer streams move together, without borrowed packet views.
#[derive(Default)]
struct FinalizationJob {
    jobs: Vec<GeometryOwner>,
    streams: FinalStreams,
    work: Work,
    sorting: solarity_cpu::CpuScratch<usize>,
    first_pass: Option<solarity_rendering::M2TransparentPass>,
    result: Option<Result<OrderedOutput, RuntimeTerrainFrameError>>,
    // Drops after streams; in-flight task ownership pins every capacity charge.
    memory: admission::OutputMemory,
}

/// Returned capacities accompany an ordering result deferred to its native boundary.
pub(in crate::application::terrain_frame::m2::preparation) struct FinalizedGeometry {
    pub(in crate::application::terrain_frame::m2::preparation) water:
        Result<u32, RuntimeTerrainFrameError>,
    pub(in crate::application::terrain_frame::m2::preparation) capacities:
        super::publication::GeometryPublication,
}

/// Ordering failure is reported after receiver callbacks, at its original boundary.
struct OrderedOutput {
    water: Result<u32, RuntimeTerrainFrameError>,
    vertex_capacity: usize,
    index_capacity: usize,
}

impl FinalizationJob {
    /// A terminal kernel always retains its inputs, including on domain failure.
    fn execute(
        cell: &mut CpuOwnedCell<Self>,
        context: &solarity_cpu::JobContext<'_>,
    ) -> solarity_cpu::JobOutcome {
        let job = cell.value_mut();
        let _profile = solarity_profiling::profile!("m2.finalization.worker");
        job.result = Some(job.assemble(context));
        if job.result.as_ref().is_some_and(Result::is_ok) {
            solarity_cpu::JobOutcome::Succeeded
        } else {
            solarity_cpu::JobOutcome::Failed
        }
    }

    /// Relocation uses original model order; only pure output work runs here.
    fn assemble(
        &mut self,
        context: &solarity_cpu::JobContext<'_>,
    ) -> Result<OrderedOutput, RuntimeTerrainFrameError> {
        let mut output = self.streams.output();
        {
            let _profile = solarity_profiling::profile!("m2.finalization.assembly");
            for owner in &mut self.jobs {
                output.publish(owner.job_mut(), &mut self.work)?;
            }
        }
        let vertex_capacity = output.vertex_capacity;
        let index_capacity = output.index_capacity;
        let first_pass = self
            .first_pass
            .unwrap_or_else(|| unreachable!("captured finalization has a transparent pass"));
        let water = {
            let _profile = solarity_profiling::profile!("m2.finalization.ordering");
            self.streams
                .order(first_pass, context.scratch(&mut self.sorting).writer())
        };
        Ok(OrderedOutput {
            water,
            vertex_capacity,
            index_capacity,
        })
    }
}
