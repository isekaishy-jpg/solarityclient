//! Owned visible and shadow draw work follows ordered animation and CPU callbacks.

mod chunk;
mod cost;
mod input;
mod job;
mod meshes;
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
mod shadows;
mod storage;

pub(super) use publication::GeometryPublication;
pub(in super::super) use ribbons::advance_ribbons;

use job::{GeometryContext, GeometryJob};
use owner::GeometryOwner;

pub(in super::super) use input::{GeometryInput, VisibleGeometryInput};

/// Retains only the current admitted job count, never historical model generations.
pub(in super::super::super) struct GeometryBatch {
    jobs: Vec<GeometryOwner>,
    reuse: reuse::GeometryReuse,
    active: usize,
    published_bones: usize,
    staged: chunk::GeometryChunk,
    spare_chunks: Vec<chunk::GeometryChunk>,
    returned_chunks: Vec<chunk::GeometryChunk>,
    pending: solarity_cpu::FrameBatch<chunk::GeometryChunk>,
    handles: Vec<solarity_cpu::FrameJob<chunk::GeometryChunk>>,
    submitted: bool,
    completion: Option<solarity_cpu::ReadyToken>,
    storage: Option<solarity_cpu::CpuStorageBudget>,
    calibration: cost::GeometryCalibration,
}

impl Default for GeometryBatch {
    fn default() -> Self {
        Self {
            jobs: Vec::new(),
            reuse: reuse::GeometryReuse::default(),
            active: 0,
            published_bones: 0,
            staged: chunk::GeometryChunk::default(),
            spare_chunks: Vec::new(),
            returned_chunks: Vec::new(),
            pending: solarity_cpu::FrameBatch::with_context(chunk::GeometryChunk::execute),
            handles: Vec::new(),
            submitted: false,
            completion: None,
            storage: None,
            calibration: cost::GeometryCalibration::default(),
        }
    }
}

#[cfg(test)]
#[path = "../../../../../../tests/application/m2_geometry.rs"]
mod tests;
