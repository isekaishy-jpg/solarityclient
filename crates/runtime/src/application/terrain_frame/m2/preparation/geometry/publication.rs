//! Incremental owned publication and unconditional effect-state return.

use super::super::super::{M2Frame, M2GpuPlacement, RuntimeTerrainFrameError};
use super::super::diagnostics::Work;
use super::{GeometryBatch, GeometryInput};
use crate::application::frame_pipeline::FrameWait;
use solarity_rendering::{M2BonePose, M2BonePoseOverrides, M2MaterialPose};
use solarity_rendering::{M2CameraEffectScale, VulkanError, WorldCameraFrame};

/// Ordered output prefix and capacity totals survive a coordinator yield.
#[derive(Default)]
pub(in super::super) struct GeometryPublication {
    pub(in super::super) next: usize,
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
        self.geometry_batch
            .reuse
            .index(&mut self.geometry_batch.jobs);
        self.geometry_batch.active = 0;
        self.geometry_batch.published_bones = 0;
        self.geometry_batch.handles.clear();
        self.geometry_batch.completion = None;
        self.geometry_batch.storage = Some(cpu.storage().clone());
        let maximum = self
            .frame_work
            .remaining_count()
            .checked_add(self.unit_effects.pending_placement_count())
            .ok_or(VulkanError::WorldFrameCapacity)?;
        self.geometry_batch
            .pending
            .begin(cpu, solarity_cpu::FrameBatchPlan::new(maximum, 0))?;
        self.geometry_batch.submitted = true;
        self.geometry_batch.completion = Some(self.geometry_batch.pending.completion()?);
        Ok(())
    }

    /// Exports only readiness; geometry and effect payloads retain their typed owners.
    pub(in super::super) fn geometry_completion(&self) -> Option<solarity_cpu::ReadyToken> {
        self.geometry_batch.completion.clone()
    }

    /// Flushes the final group and seals admission before independent main work so workers publish
    /// durable phase completion without waiting for main to consume a packet.
    pub(in super::super) fn close_geometry(&mut self) -> Result<(), RuntimeTerrainFrameError> {
        self.geometry_batch.flush_staged()?;
        self.geometry_batch.pending.close();
        Ok(())
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
            let result = batch.pending.reclaim(&mut batch.returned_chunks);
            batch.submitted = false;
            let used = batch.returned_chunks.len();
            for mut chunk in batch.returned_chunks.drain(..) {
                chunk.reclaim(&mut batch.jobs);
                batch.spare_chunks.push(chunk);
            }
            // An abandoned admission can own a final group never sent to workers.
            // It follows the submitted prefix and returns effects without executing.
            batch.staged.reclaim(&mut batch.jobs);
            batch.spare_chunks.truncate(used);
            for job in &mut batch.jobs {
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

    /// Waits for the exact unpublished result, or terminal phase release.
    pub(in super::super) fn wait_geometry(
        &self,
        cursor: &GeometryPublication,
        wait: &mut FrameWait<'_>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let batch = &self.geometry_batch;
        if let Some(handle) = batch.handles.get(cursor.next) {
            wait.before_result(&batch.pending, handle)?;
        } else {
            wait.before_reclaim(&batch.pending)?;
        }
        Ok(())
    }

    /// Publishes only the ready ordered prefix, retaining cursor and capacity totals.
    /// No pending result blocks main or leases a domain output across the yield.
    pub(in super::super) fn try_publish_geometry(
        &mut self,
        work: &mut Work,
        cursor: &mut GeometryPublication,
    ) -> Result<bool, RuntimeTerrainFrameError> {
        let _profile = solarity_profiling::profile!("m2.geometry_publication");
        let batch = &mut self.geometry_batch;
        let mut output = super::output::GeometryOutput {
            published_bones: &mut batch.published_bones,
            visible_draws: &mut self.visible_draws,
            shadow_draws: &mut self.shadow_draws,
            environment_shadow_draws: &mut self.environment_shadow_draws,
            particle_draws: &mut self.particle_draws,
            ribbon_draws: &mut self.ribbon_draws,
            transparent_elements: &mut self.transparent_elements,
            particle_vertices: &mut self.particle_vertices,
            particle_indices: &mut self.particle_indices,
            ribbon_vertices: &mut self.ribbon_vertices,
            recoverable_errors: &mut self.recoverable_errors,
            vertex_capacity: cursor.vertex_capacity,
            index_capacity: cursor.index_capacity,
        };
        while cursor.next < batch.handles.len() {
            let Some(result) =
                batch
                    .pending
                    .try_with_result(&batch.handles[cursor.next], |chunk| {
                        for job in chunk.jobs.writer().iter_mut() {
                            output.publish(job.job_mut(), work)?;
                        }
                        Ok::<_, RuntimeTerrainFrameError>(())
                    })?
            else {
                cursor.vertex_capacity = output.vertex_capacity;
                cursor.index_capacity = output.index_capacity;
                return Ok(false);
            };
            result?;
            cursor.next += 1;
        }
        cursor.vertex_capacity = output.vertex_capacity;
        cursor.index_capacity = output.index_capacity;
        Ok(true)
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
        material_poses: &mut Vec<Option<M2MaterialPose>>,
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
        job.reserve_outputs(
            batch
                .storage
                .as_ref()
                .unwrap_or_else(|| unreachable!("geometry admission owns a storage budget")),
            &input,
            placement,
            source,
        )?;
        job.input = Some(input);
        job.context = Some(super::GeometryContext {
            source: std::sync::Arc::clone(source),
            camera,
            effect_scale,
            twinkle: std::sync::Arc::clone(twinkle),
        });
        job.palette.prepare(
            palette,
            batch
                .storage
                .as_ref()
                .unwrap_or_else(|| unreachable!("geometry admission owns a storage budget")),
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
        job.owns_effects = input.visible.is_some();
        if job.owns_effects {
            std::mem::swap(&mut job.particles, &mut placement.particles);
            std::mem::swap(&mut job.ribbons, &mut placement.ribbons);
        }
        std::mem::swap(&mut job.pose, pose);
        std::mem::swap(&mut job.material_poses, material_poses);
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
