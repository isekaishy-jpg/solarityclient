//! Stable geometry records keep admission and reclamation from moving full job state.

use super::GeometryJob;
use solarity_cpu::{CpuError, CpuOwnedCell, CpuStorageBudget, CpuStorageClass, CpuStorageKind};

/// Empty slots exist only where an admitted owner moved into a worker chunk.
/// Completed jobs retain their own charge while awaiting next-frame identity reuse.
#[derive(Default)]
pub(super) struct GeometryOwner {
    cell: Option<CpuOwnedCell<GeometryJob>>,
}

impl GeometryOwner {
    /// Admits a new generation's record before taking any live placement state.
    pub(super) fn new(budget: &CpuStorageBudget) -> Result<Self, CpuError> {
        Ok(Self {
            cell: Some(CpuOwnedCell::new_with(
                budget,
                CpuStorageClass::Frame,
                CpuStorageKind::Scratch,
                GeometryJob::default,
            )?),
        })
    }

    pub(super) fn job(&self) -> &GeometryJob {
        self.cell
            .as_ref()
            .unwrap_or_else(|| unreachable!("geometry owner is admitted before use"))
            .value()
    }

    /// A retained frame may be rebound to another executor; its record follows
    /// the same transactional budget transfer as its nested output buffers.
    pub(super) fn admit(&mut self, budget: &CpuStorageBudget) -> Result<(), CpuError> {
        self.cell
            .as_mut()
            .unwrap_or_else(|| unreachable!("reused geometry owner retains its cell"))
            .transfer(budget, CpuStorageClass::Frame, CpuStorageKind::Scratch)
    }

    pub(super) fn job_mut(&mut self) -> &mut GeometryJob {
        self.cell
            .as_mut()
            .unwrap_or_else(|| unreachable!("geometry owner is admitted before use"))
            .value_mut()
    }
}
