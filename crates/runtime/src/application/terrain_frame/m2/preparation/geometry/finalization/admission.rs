//! Final storage is admitted from completed immutable outputs before worker transfer.

use super::super::super::super::RuntimeTerrainFrameError;
use super::super::GeometryOwner;
use super::FinalStreams;
use solarity_cpu::{
    ByteReservation, CpuError, CpuScratch, CpuStorageBudget, CpuStorageClass as Class,
    CpuStorageKind as Kind, CpuStorageReservation, CpuStorageWorkingSet,
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
#[derive(Clone, Copy, Default)]
pub(super) struct OutputCounts {
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
    pub(super) fn collect(jobs: &mut [GeometryOwner]) -> Result<Self, RuntimeTerrainFrameError> {
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
    /// Proves the connected output/sorting working set before changing retained capacities.
    pub(super) fn reservation_bytes(
        &self,
        frame: &FinalStreams,
        budget: &CpuStorageBudget,
        sorting: &CpuScratch<usize>,
        counts: OutputCounts,
    ) -> Result<usize, CpuError> {
        let sorting_count = counts
            .visible_draws
            .max(counts.particle_draws)
            .max(counts.ribbon_draws);
        let mut working_set = CpuStorageWorkingSet::default();
        working_set.include(
            required_bytes(
                &frame.visible_draws,
                &self.visible_draws,
                budget,
                counts.visible_draws,
            )?,
            replacement_credit(&frame.visible_draws, counts.visible_draws)?,
        )?;
        working_set.include(
            required_bytes(
                &frame.shadow_draws,
                &self.shadow_draws,
                budget,
                counts.shadow_draws,
            )?,
            replacement_credit(&frame.shadow_draws, counts.shadow_draws)?,
        )?;
        working_set.include(
            required_bytes(
                &frame.environment_shadow_draws,
                &self.environment_shadow_draws,
                budget,
                counts.environment_shadow_draws,
            )?,
            replacement_credit(
                &frame.environment_shadow_draws,
                counts.environment_shadow_draws,
            )?,
        )?;
        working_set.include(
            required_bytes(
                &frame.particle_draws,
                &self.particle_draws,
                budget,
                counts.particle_draws,
            )?,
            replacement_credit(&frame.particle_draws, counts.particle_draws)?,
        )?;
        working_set.include(
            required_bytes(
                &frame.ribbon_draws,
                &self.ribbon_draws,
                budget,
                counts.ribbon_draws,
            )?,
            replacement_credit(&frame.ribbon_draws, counts.ribbon_draws)?,
        )?;
        working_set.include(
            required_bytes(
                &frame.transparent_elements,
                &self.transparent_elements,
                budget,
                counts.transparent_elements,
            )?,
            replacement_credit(&frame.transparent_elements, counts.transparent_elements)?,
        )?;
        working_set.include(
            required_bytes(
                &frame.particle_vertices,
                &self.particle_vertices,
                budget,
                counts.particle_vertices,
            )?,
            replacement_credit(&frame.particle_vertices, counts.particle_vertices)?,
        )?;
        working_set.include(
            required_bytes(
                &frame.particle_indices,
                &self.particle_indices,
                budget,
                counts.particle_indices,
            )?,
            replacement_credit(&frame.particle_indices, counts.particle_indices)?,
        )?;
        working_set.include(
            required_bytes(
                &frame.ribbon_vertices,
                &self.ribbon_vertices,
                budget,
                counts.ribbon_vertices,
            )?,
            replacement_credit(&frame.ribbon_vertices, counts.ribbon_vertices)?,
        )?;
        working_set.include(
            required_bytes(
                &frame.recoverable_errors,
                &self.recoverable_errors,
                budget,
                counts.recoverable_errors,
            )?,
            replacement_credit(&frame.recoverable_errors, counts.recoverable_errors)?,
        )?;
        working_set.include(
            sorting.reservation_bytes(budget, Class::Frame, sorting_count)?,
            sorting.replacement_credit(sorting_count),
        )?;
        Ok(working_set.bytes())
    }

    /// Funds renderer output streams and sort scratch from the scheduler's reservation.
    pub(super) fn reserve_reserved(
        &mut self,
        frame: &mut FinalStreams,
        reservation: &mut CpuStorageReservation,
        sorting: &mut CpuScratch<usize>,
        counts: OutputCounts,
    ) -> Result<(), CpuError> {
        let sorting_count = counts
            .visible_draws
            .max(counts.particle_draws)
            .max(counts.ribbon_draws);
        reserve(
            &mut frame.visible_draws,
            &mut self.visible_draws,
            reservation,
            counts.visible_draws,
        )?;
        reserve(
            &mut frame.shadow_draws,
            &mut self.shadow_draws,
            reservation,
            counts.shadow_draws,
        )?;
        reserve(
            &mut frame.environment_shadow_draws,
            &mut self.environment_shadow_draws,
            reservation,
            counts.environment_shadow_draws,
        )?;
        reserve(
            &mut frame.particle_draws,
            &mut self.particle_draws,
            reservation,
            counts.particle_draws,
        )?;
        reserve(
            &mut frame.ribbon_draws,
            &mut self.ribbon_draws,
            reservation,
            counts.ribbon_draws,
        )?;
        reserve(
            &mut frame.transparent_elements,
            &mut self.transparent_elements,
            reservation,
            counts.transparent_elements,
        )?;
        reserve(
            &mut frame.particle_vertices,
            &mut self.particle_vertices,
            reservation,
            counts.particle_vertices,
        )?;
        reserve(
            &mut frame.particle_indices,
            &mut self.particle_indices,
            reservation,
            counts.particle_indices,
        )?;
        reserve(
            &mut frame.ribbon_vertices,
            &mut self.ribbon_vertices,
            reservation,
            counts.ribbon_vertices,
        )?;
        reserve(
            &mut frame.recoverable_errors,
            &mut self.recoverable_errors,
            reservation,
            counts.recoverable_errors,
        )?;
        sorting.reserve_reserved(reservation, sorting_count)?;
        Ok(())
    }
}

/// Plans the actual retained allocation plus the complete replacement, preserving
/// the existing geometric growth policy and charging foreign-executor adoption.
fn required_bytes<T>(
    values: &Vec<T>,
    charge: &Option<ByteReservation>,
    budget: &CpuStorageBudget,
    additional: usize,
) -> Result<usize, CpuError> {
    let bytes = |capacity: usize| {
        capacity
            .checked_mul(size_of::<T>())
            .ok_or(CpuError::StorageSizeOverflow)
    };
    let mut total = match charge {
        Some(memory) => memory.admission_bytes(budget, Class::Frame),
        None => bytes(values.capacity())?,
    };
    let required = values
        .len()
        .checked_add(additional)
        .ok_or(CpuError::StorageSizeOverflow)?;
    if required > values.capacity() {
        add(
            &mut total,
            bytes(required.max(values.capacity().saturating_mul(2)))?,
        )?;
    }
    Ok(total)
}

/// Only growth releases the old vector; retained capacity stays charged otherwise.
fn replacement_credit<T>(values: &Vec<T>, additional: usize) -> Result<usize, CpuError> {
    let required = values
        .len()
        .checked_add(additional)
        .ok_or(CpuError::StorageSizeOverflow)?;
    if required > values.capacity() {
        values
            .capacity()
            .checked_mul(size_of::<T>())
            .ok_or(CpuError::StorageSizeOverflow)
    } else {
        Ok(0)
    }
}

/// This adapter adopts existing Vec storage, then uses the CPU buffer's
/// old-plus-new reservation rule. Kernels only receive fixed-capacity writers.
fn reserve<T>(
    values: &mut Vec<T>,
    charge: &mut Option<ByteReservation>,
    reservation: &mut CpuStorageReservation,
    additional: usize,
) -> Result<(), CpuError> {
    let bytes = |capacity: usize| {
        capacity
            .checked_mul(std::mem::size_of::<T>())
            .ok_or(CpuError::StorageSizeOverflow)
    };
    if let Some(memory) = charge {
        memory.transfer_reserved(reservation, Kind::Result)?;
    } else if values.capacity() != 0 {
        *charge = Some(reservation.reserve(Kind::Result, bytes(values.capacity())?)?);
    }
    let required = values
        .len()
        .checked_add(additional)
        .ok_or(CpuError::StorageSizeOverflow)?;
    if required <= values.capacity() {
        return Ok(());
    }
    let capacity = required.max(values.capacity().saturating_mul(2));
    let mut memory = reservation.reserve(Kind::Result, bytes(capacity)?)?;
    let mut replacement = Vec::new();
    replacement
        .try_reserve_exact(capacity)
        .map_err(|_| CpuError::StorageAllocation)?;
    memory.resize_reserved(reservation, bytes(replacement.capacity())?)?;
    replacement.append(values);
    drop(std::mem::replace(values, replacement));
    if let Some(retired) = charge.replace(memory) {
        reservation.recycle(retired)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../../../../../tests/application/m2_output_admission.rs"]
mod tests;
