//! Owned visible and shadow draw work follows ordered animation and CPU callbacks.

mod chunk;
mod cost;
mod finalization;
mod input;
mod job;
mod meshes;
mod metadata;
mod output;
mod owner;
mod palette;
mod palettes;
mod particles;
mod prepare;
mod publication;
mod reuse;
mod ribbon_draws;
mod ribbons;
mod scratch;
mod shadows;
mod storage;

pub(super) use publication::GeometryPublication;
pub(in super::super) use ribbons::advance_ribbons;

use job::{GeometryContext, GeometryJob};
use owner::GeometryOwner;

pub(in super::super) use input::{GeometryInput, VisibleGeometryInput};

/// Retains only the current admitted job count, never historical model generations.
pub(in super::super::super) struct GeometryBatch {
    jobs: solarity_cpu::CpuBuffer<GeometryOwner>,
    finalization: finalization::Finalization,
    reuse: reuse::GeometryReuse,
    active: usize,
    published_bones: usize,
    staged: chunk::GeometryChunk,
    spare_chunks: solarity_cpu::CpuBuffer<chunk::GeometryChunk>,
    /// Funded groups before publication; returned groups after terminal reclaim.
    owned_chunks: solarity_cpu::CpuBuffer<chunk::GeometryChunk>,
    chunk_costs: solarity_cpu::CpuBuffer<solarity_cpu::JobCost>,
    pending: solarity_cpu::FrameBatch<chunk::GeometryChunk>,
    submitted: bool,
    completion: Option<solarity_cpu::ReadyToken>,
    storage: Option<solarity_cpu::CpuStorageBudget>,
    particle_scratch: Option<solarity_cpu::CpuWorkerScratch<usize>>,
    particle_scratch_peak: usize,
    calibration: cost::GeometryCalibration,
}

impl Default for GeometryBatch {
    fn default() -> Self {
        Self {
            jobs: solarity_cpu::CpuBuffer::default(),
            finalization: finalization::Finalization::default(),
            reuse: reuse::GeometryReuse::default(),
            active: 0,
            published_bones: 0,
            staged: chunk::GeometryChunk::default(),
            spare_chunks: solarity_cpu::CpuBuffer::default(),
            owned_chunks: solarity_cpu::CpuBuffer::default(),
            chunk_costs: solarity_cpu::CpuBuffer::default(),
            pending: solarity_cpu::FrameBatch::with_context(chunk::GeometryChunk::execute),
            submitted: false,
            completion: None,
            storage: None,
            particle_scratch: None,
            particle_scratch_peak: 0,
            calibration: cost::GeometryCalibration::default(),
        }
    }
}

#[cfg(test)]
#[path = "../../../../../../tests/application/m2_geometry.rs"]
mod tests;
