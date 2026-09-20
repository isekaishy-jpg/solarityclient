//! Final storage is admitted from completed immutable outputs before worker transfer.

use super::super::super::super::{M2Frame, RuntimeTerrainFrameError};
use super::super::GeometryOwner;
use solarity_cpu::{
    ByteReservation, CpuError, CpuStorageBudget, CpuStorageClass as Class, CpuStorageKind as Kind,
};

/// Charges follow the retained renderer vectors even while main owns the values.
#[derive(Default)]
pub(super) struct OutputMemory {
    visible_draws: Option<ByteReservation>,
    shadow_draws: Option<ByteReservation>,
    environment_shadow_draws: Option<ByteReservation>,
    particle_draws: Option<ByteReservation>,
    ribbon_draws: Option<ByteReservation>,
    transparent_elements: Option<ByteReservation>,
    particle_vertices: Option<ByteReservation>,
    particle_indices: Option<ByteReservation>,
    ribbon_vertices: Option<ByteReservation>,
    recoverable_errors: Option<ByteReservation>,
}

/// Exact live output counts are immutable after the geometry phase is terminal.
#[derive(Default)]
struct OutputCounts {
    visible_draws: usize,
    shadow_draws: usize,
    environment_shadow_draws: usize,
    particle_draws: usize,
    ribbon_draws: usize,
    transparent_elements: usize,
    particle_vertices: usize,
    particle_indices: usize,
    ribbon_vertices: usize,
    recoverable_errors: usize,
}

impl OutputCounts {
    /// Error precedence follows model order before optional capacity admission.
    fn collect(jobs: &mut [GeometryOwner]) -> Result<Self, RuntimeTerrainFrameError> {
        let mut counts = Self::default();
        for owner in jobs {
            let job = owner.job_mut();
            if job.result.as_ref().is_some_and(Result::is_err) {
                return job
                    .result
                    .take()
                    .unwrap_or_else(|| unreachable!("checked geometry result exists"))
                    .map(|()| counts);
            }
            let input = job
                .input
                .unwrap_or_else(|| unreachable!("completed geometry retains its input"));
            if input.primary_shadow {
                add(&mut counts.shadow_draws, job.shadow_draws.len())?;
            }
            if input.environment_maps != 0 {
                add(&mut counts.environment_shadow_draws, job.shadow_draws.len())?;
            }
            if input.visible.is_none() {
                continue;
            }
            add(&mut counts.visible_draws, job.visible_draws.len())?;
            add(&mut counts.particle_draws, job.particle_draws.len())?;
            add(&mut counts.ribbon_draws, job.ribbon_draws.len())?;
            add(
                &mut counts.transparent_elements,
                job.transparent_elements.len(),
            )?;
            add(&mut counts.particle_vertices, job.particle_vertices.len())?;
            add(&mut counts.particle_indices, job.particle_indices.len())?;
            add(&mut counts.ribbon_vertices, job.ribbon_vertices.len())?;
            add(&mut counts.recoverable_errors, job.recoverable_errors.len())?;
        }
        Ok(counts)
    }
}

/// Checked arithmetic fails before any frame-owned input is relinquished.
fn add(total: &mut usize, count: usize) -> Result<(), CpuError> {
    *total = total
        .checked_add(count)
        .ok_or(CpuError::StorageSizeOverflow)?;
    Ok(())
}

impl OutputMemory {
    /// Rebinding/growth keeps every previous buffer valid if a later admission fails.
    pub(super) fn prepare(
        &mut self,
        frame: &mut M2Frame,
        budget: &CpuStorageBudget,
    ) -> Result<usize, RuntimeTerrainFrameError> {
        let counts = OutputCounts::collect(&mut frame.geometry_batch.jobs)?;
        reserve(
            &mut frame.visible_draws,
            &mut self.visible_draws,
            budget,
            counts.visible_draws,
        )?;
        reserve(
            &mut frame.shadow_draws,
            &mut self.shadow_draws,
            budget,
            counts.shadow_draws,
        )?;
        reserve(
            &mut frame.environment_shadow_draws,
            &mut self.environment_shadow_draws,
            budget,
            counts.environment_shadow_draws,
        )?;
        reserve(
            &mut frame.particle_draws,
            &mut self.particle_draws,
            budget,
            counts.particle_draws,
        )?;
        reserve(
            &mut frame.ribbon_draws,
            &mut self.ribbon_draws,
            budget,
            counts.ribbon_draws,
        )?;
        reserve(
            &mut frame.transparent_elements,
            &mut self.transparent_elements,
            budget,
            counts.transparent_elements,
        )?;
        reserve(
            &mut frame.particle_vertices,
            &mut self.particle_vertices,
            budget,
            counts.particle_vertices,
        )?;
        reserve(
            &mut frame.particle_indices,
            &mut self.particle_indices,
            budget,
            counts.particle_indices,
        )?;
        reserve(
            &mut frame.ribbon_vertices,
            &mut self.ribbon_vertices,
            budget,
            counts.ribbon_vertices,
        )?;
        reserve(
            &mut frame.recoverable_errors,
            &mut self.recoverable_errors,
            budget,
            counts.recoverable_errors,
        )?;
        Ok(counts
            .visible_draws
            .max(counts.particle_draws)
            .max(counts.ribbon_draws))
    }
}

/// This adapter adopts existing Vec storage, then uses the CPU buffer's
/// old-plus-new reservation rule. Kernels only receive fixed-capacity writers.
fn reserve<T>(
    values: &mut Vec<T>,
    charge: &mut Option<ByteReservation>,
    budget: &CpuStorageBudget,
    additional: usize,
) -> Result<(), CpuError> {
    let bytes = |capacity: usize| {
        capacity
            .checked_mul(std::mem::size_of::<T>())
            .ok_or(CpuError::StorageSizeOverflow)
    };
    if let Some(memory) = charge {
        memory.transfer(budget, Class::Frame, Kind::Result)?;
    } else if values.capacity() != 0 {
        *charge = Some(budget.reserve(Class::Frame, Kind::Result, bytes(values.capacity())?)?);
    }
    let required = values
        .len()
        .checked_add(additional)
        .ok_or(CpuError::StorageSizeOverflow)?;
    if required <= values.capacity() {
        return Ok(());
    }
    let capacity = required.max(values.capacity().saturating_mul(2));
    let mut memory = budget.reserve(Class::Frame, Kind::Result, bytes(capacity)?)?;
    let mut replacement = Vec::new();
    replacement
        .try_reserve_exact(capacity)
        .map_err(|_| CpuError::StorageAllocation)?;
    memory.resize(bytes(replacement.capacity())?)?;
    replacement.append(values);
    drop(std::mem::replace(values, replacement));
    *charge = Some(memory);
    Ok(())
}

#[cfg(test)]
#[path = "../../../../../../../tests/application/m2_output_admission.rs"]
mod tests;
