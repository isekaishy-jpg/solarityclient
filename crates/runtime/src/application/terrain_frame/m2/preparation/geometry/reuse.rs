//! Prior-frame output storage follows model generations, never traversal ordinals.

use super::super::super::{M2GpuSource, M2GpuSourceData};
use super::{GeometryInput, GeometryOwner};
use solarity_asset::AssetStorageMap;
use std::sync::Weak;

/// Different visible/shadow demands must not spread particle capacities into
/// shadow-only jobs as models move across the camera's admission boundary.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub(super) struct ReuseKey {
    generation: usize,
    visible: bool,
    shadows: bool,
}

/// Weak ownership keeps the allocation address unique without pinning retired
/// models or GPU resources. The key is an identity, never a dereferenced pointer.
pub(super) struct GeometryReuseIdentity {
    key: ReuseKey,
    _generation: Weak<M2GpuSourceData>,
}

impl GeometryReuseIdentity {
    /// Identifies immutable output layout and its admitted consumer requirements.
    pub(super) fn new(source: &M2GpuSource, input: &GeometryInput) -> Self {
        Self {
            key: ReuseKey {
                generation: std::sync::Arc::as_ptr(source) as usize,
                visible: input.visible.is_some(),
                shadows: input.primary_shadow || input.environment_maps != 0,
            },
            _generation: std::sync::Arc::downgrade(source),
        }
    }
}

/// Intrusive lists reuse the existing job buffer and a warmed hash table. Only the
/// previous admitted frame is indexed; unused jobs retire at phase reclamation.
pub(super) struct GeometryReuse {
    pub(super) heads: AssetStorageMap<ReuseKey, usize>,
}
impl Default for GeometryReuse {
    fn default() -> Self {
        Self {
            heads: AssetStorageMap::metadata(),
        }
    }
}

impl GeometryReuse {
    /// Effect state has already returned to placements. Index only completed
    /// jobs, without moving their output allocations or retaining source payloads.
    pub(super) fn index(&mut self, jobs: &mut [GeometryOwner]) {
        self.heads.clear_retaining_capacity();
        for (index, owner) in jobs.iter_mut().enumerate() {
            let job = owner.job_mut();
            debug_assert!(!job.owns_effects);
            job.next_reuse = job
                .reuse_identity
                .as_ref()
                .and_then(|identity| self.heads.insert_reserved(identity.key, index));
        }
    }

    /// Takes one matching prior allocation in constant expected time. New demand
    /// starts empty, rather than inheriting a different model's historical maxima.
    pub(super) fn take(
        &mut self,
        identity: &GeometryReuseIdentity,
        jobs: &mut solarity_cpu::CpuBuffer<GeometryOwner>,
        budget: &solarity_cpu::CpuStorageBudget,
    ) -> Result<usize, solarity_cpu::CpuError> {
        if let Some(head) = self.heads.get_mut(&identity.key) {
            let slot = *head;
            // Refusal must leave the reusable list and its original owner intact.
            jobs[slot].admit(budget)?;
            if let Some(next) = jobs[slot].job_mut().next_reuse.take() {
                *head = next;
            } else {
                self.heads.remove(&identity.key);
            }
            return Ok(slot);
        }
        let slot = jobs.len();
        if slot == jobs.capacity() {
            jobs.reserve(
                budget,
                solarity_cpu::CpuStorageClass::Frame,
                solarity_cpu::CpuStorageKind::Metadata,
                slot.checked_add(1)
                    .and_then(usize::checked_next_power_of_two)
                    .ok_or(solarity_cpu::CpuError::StorageSizeOverflow)?,
            )?;
        }
        jobs.push(GeometryOwner::new(budget)?)?;
        Ok(slot)
    }

    /// No index may survive replacement of the reclaimed job list.
    pub(super) fn clear(&mut self) {
        self.heads.clear_retaining_capacity();
    }
}

#[cfg(test)]
#[path = "../../../../../../tests/application/m2_geometry_reuse.rs"]
mod tests;
