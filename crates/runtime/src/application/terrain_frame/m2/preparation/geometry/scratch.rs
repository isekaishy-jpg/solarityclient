//! Phase admission maintains bounded worker scratch independently of model count.

use super::GeometryBatch;
use solarity_cpu::{CpuError, CpuExecutor, CpuStorageClass, CpuWorkerScratch};

impl GeometryBatch {
    /// Runs after prior-state reclamation. A changed executor gets a fresh binding.
    /// Shrink at this explicit maintenance boundary only when warm capacity exceeds
    /// twice the preceding phase's bound. This avoids retaining a rare large scene
    /// forever while leaving hysteresis for ordinary visibility changes.
    pub(super) fn prepare_scratch(&mut self, cpu: &CpuExecutor) -> Result<(), CpuError> {
        match &mut self.particle_scratch {
            Some(scratch) if scratch.belongs_to(cpu) => {
                if scratch.capacity() > self.particle_scratch_peak.saturating_mul(2) {
                    scratch.trim(self.particle_scratch_peak)?;
                }
            }
            slot => *slot = Some(CpuWorkerScratch::new(cpu, CpuStorageClass::Frame)?),
        }
        self.particle_scratch_peak = 0;
        Ok(())
    }
}
