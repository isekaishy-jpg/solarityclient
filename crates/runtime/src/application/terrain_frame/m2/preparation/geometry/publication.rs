//! Complete required geometry publication and unconditional effect-state return.

use super::super::super::{M2Frame, M2GpuPlacement, RuntimeTerrainFrameError};
use super::{GeometryBatch, GeometryInput};
use crate::application::frame_pipeline::FrameWait;
use solarity_rendering::{M2BonePose, M2BonePoseOverrides};
use solarity_rendering::{M2CameraEffectScale, VulkanError, WorldCameraFrame};

/// Ordered output prefix and capacity totals survive a coordinator yield.
#[derive(Default)]
pub(in super::super) struct GeometryPublication {
    pub(in super::super) vertex_capacity: usize,
    pub(in super::super) index_capacity: usize,
}

impl M2Frame {
    /// Starts the visible/shadow work list after prior-frame state has returned.
    pub(in super::super) fn begin_geometry(
        &mut self,
        cpu: &solarity_cpu::CpuExecutor,
    ) -> Result<(), RuntimeTerrainFrameError> {
        debug_assert!(
            self.geometry_batch
                .jobs
                .iter()
                .all(|owner| !owner.job().owns_effects)
        );
        let maximum = self
            .frame_work
            .remaining_count()
            .checked_add(self.unit_effects.pending_placement_count())
            .ok_or(VulkanError::WorldFrameCapacity)?;
        self.geometry_batch.prepare_scratch(cpu)?;
        self.geometry_batch.begin_metadata(cpu, maximum)?;
        self.geometry_batch
            .reuse
            .index(&mut self.geometry_batch.jobs);
        debug_assert!(self.geometry_batch.owned_chunks.is_empty());
        self.geometry_batch.chunk_costs.clear();
        self.geometry_batch.active = 0;
        self.geometry_batch.published_bones = 0;
        self.geometry_batch.completion = None;
        self.geometry_batch.storage = Some(cpu.storage().clone());
        self.geometry_batch.submitted = true;
        self.geometry_batch.completion = Some(self.geometry_batch.pending.completion()?);
        Ok(())
    }

    /// Exports only readiness; geometry and effect payloads retain their typed owners.
    pub(in super::super) fn geometry_completion(&self) -> Option<solarity_cpu::ReadyToken> {
        self.geometry_batch.completion.clone()
    }

    /// Ordered admission has discovered the full effect tail. Every group's outputs
    /// and live simulation capacity are funded before the first geometry dispatch.
    pub(in super::super) fn close_geometry(&mut self) -> Result<(), RuntimeTerrainFrameError> {
        self.geometry_batch.publish_groups()
    }

    /// Restores every model on success, validation errors and joined-worker panics.
    pub(in super::super) fn restore_geometry_states(&mut self) {
        for owner in &mut self.geometry_batch.jobs[..self.geometry_batch.active] {
            let job = owner.job_mut();
            if !job.owns_effects {
                continue;
            }
            let Some(input) = job.input else {
                unreachable!("owned effects have an admitted input");
            };
            let placement = &mut self.placements[input.placement_index];
            std::mem::swap(&mut job.particles, &mut placement.particles);
            std::mem::swap(&mut job.ribbons, &mut placement.ribbons);
            job.owns_effects = false;
        }
    }

    /// Closes the producer and restores owned jobs before effect-state return.
    pub(in super::super) fn finish_geometry(
        &mut self,
        wait: &mut FrameWait<'_>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let batch = &mut self.geometry_batch;
        if batch.submitted {
            // Used slots are placeholders. Unmatched prior-frame jobs release
            // their output capacities here instead of following new draw ordinals.
            batch.jobs.clear();
            batch.reuse.clear();
            batch.pending.close();
            let readiness = wait.before_reclaim(&batch.pending);
            let result = batch.pending.reclaim_into(&mut batch.owned_chunks.writer());
            batch.submitted = false;
            batch.chunk_costs.clear();
            // On failed admission, the same bank still owns undispatched groups.
            // Atomic publication means these never mix with a submitted prefix.
            let used = batch.owned_chunks.len();
            for mut chunk in batch.owned_chunks.drain() {
                chunk
                    .reclaim(&mut batch.jobs.writer())
                    .unwrap_or_else(|_| unreachable!("admitted model return capacity"));
                batch
                    .spare_chunks
                    .push(chunk)
                    .unwrap_or_else(|_| unreachable!("admitted chunk return capacity"));
            }
            // An abandoned admission can own a final group never sent to workers.
            // It follows the submitted prefix and returns effects without executing.
            batch
                .staged
                .reclaim(&mut batch.jobs.writer())
                .unwrap_or_else(|_| unreachable!("admitted staged model capacity"));
            batch.spare_chunks.truncate(used);
            for job in batch.jobs.iter_mut() {
                batch.calibration.record(job.job_mut());
            }
            readiness?;
            result?;
            if solarity_profiling::detail_enabled()
                && let Some(storage) = &batch.storage
            {
                let storage = storage.snapshot();
                solarity_profiling::profile_value!(
                    "m2.geometry.retained_frame_bytes",
                    storage.used(solarity_cpu::CpuStorageClass::Frame)
                );
                solarity_profiling::profile_value!(
                    "m2.geometry.retained_result_bytes",
                    storage.bytes(
                        solarity_cpu::CpuStorageClass::Frame,
                        solarity_cpu::CpuStorageKind::Result,
                    )
                );
            }
        }
        Ok(())
    }

    /// A closed batch can be reclaimed only after its final publication releases admission.
    pub(in super::super) fn geometry_is_finished(&self) -> bool {
        self.geometry_batch.pending.is_finished()
    }

    /// A complete geometry phase returns model ownership before finalization.
    pub(in super::super) fn wait_geometry(
        &self,
        wait: &mut FrameWait<'_>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        wait.before_reclaim(&self.geometry_batch.pending)?;
        Ok(())
    }
}

impl GeometryBatch {
    /// Transfers palettes and simulation storage without cloning live particle state.
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn queue(
        &mut self,
        input: GeometryInput,
        placement: &mut M2GpuPlacement,
        pose: &mut M2BonePose,
        palette: Option<M2BonePoseOverrides<'_>>,
        source: &super::super::super::M2GpuSource,
        camera: WorldCameraFrame,
        effect_scale: M2CameraEffectScale,
        twinkle: &std::sync::Arc<solarity_rendering::M2ParticleTwinkleTable>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let batch = self;
        if !batch.submitted {
            return Err(solarity_cpu::CpuError::BatchInactive.into());
        }
        let identity = super::reuse::GeometryReuseIdentity::new(source, &input);
        let slot = batch.reuse.take(
            &identity,
            &mut batch.jobs,
            batch
                .storage
                .as_ref()
                .unwrap_or_else(|| unreachable!("geometry admission owns a storage budget")),
        )?;
        let job = batch.jobs[slot].job_mut();
        job.reuse_identity = Some(identity);
        job.reset();
        // Reserve the shared worker lane before capturing any live simulation.
        let sorting = if input.visible.is_some() {
            placement
                .particles
                .iter()
                .zip(&source.particles)
                .filter(|(particle, _)| particle.unsupported.is_none())
                .map(|(particle, resource)| {
                    particle
                        .simulation
                        .capacity()
                        .max(resource.maximum_particles)
                })
                .max()
                .unwrap_or(0)
        } else {
            0
        };
        batch
            .particle_scratch
            .as_mut()
            .unwrap_or_else(|| unreachable!("geometry admission owns worker scratch"))
            .reserve(sorting)?;
        batch.particle_scratch_peak = batch.particle_scratch_peak.max(sorting);
        // Output headroom and copied overrides are one admission transaction.
        // Numeric work and the output allocations themselves remain on workers.
        job.admit_capture(
            batch
                .storage
                .as_ref()
                .unwrap_or_else(|| unreachable!("geometry admission owns budget")),
            &input,
            source,
            pose,
            palette,
            input
                .visible
                .is_some()
                .then_some((&mut placement.particles, &mut placement.ribbons)),
        )?;
        batch.calibration.prepare(job, source, placement);
        let cost = job.measurement.cost();
        if batch.staged.precedes(cost) {
            batch.flush_staged()?;
        }
        batch.staged.reserve(
            batch
                .storage
                .as_ref()
                .unwrap_or_else(|| unreachable!("admitted draw phase owns budget")),
        )?;
        let job = batch.jobs[slot].job_mut();
        job.input = Some(input);
        job.context = Some(super::GeometryContext {
            source: std::sync::Arc::clone(source),
            camera,
            effect_scale,
            twinkle: std::sync::Arc::clone(twinkle),
        });
        job.owns_effects = input.visible.is_some();
        if job.owns_effects {
            std::mem::swap(&mut job.particles, &mut placement.particles);
            std::mem::swap(&mut job.ribbons, &mut placement.ribbons);
        }
        // Deferred palettes are produced into this model's retained storage.
        // Only an already sampled root transfers a palette from ordered admission.
        if !job.palette.pending {
            std::mem::swap(&mut job.pose, pose);
        }
        batch
            .staged
            .push(std::mem::take(&mut batch.jobs[slot]), cost);
        batch.active += 1;
        if batch.staged.full() {
            batch.flush_staged()?;
        }
        Ok(())
    }
}
