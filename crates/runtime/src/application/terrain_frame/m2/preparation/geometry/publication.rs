//! Incremental owned publication and unconditional effect-state return.

use super::super::super::{M2Frame, M2GpuPlacement, RuntimeTerrainFrameError};
use super::super::diagnostics::Work;
use super::{GeometryBatch, GeometryInput, GeometryJob};
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
    /// Starts the visible work list; prior state was returned before its frame ended.
    pub(in super::super) fn begin_geometry(
        &mut self,
        cpu: &solarity_cpu::CpuExecutor,
    ) -> Result<(), RuntimeTerrainFrameError> {
        debug_assert!(self.geometry_batch.jobs.iter().all(|job| !job.owns_effects));
        self.geometry_batch.active = 0;
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

    /// Seals admission before independent main work so workers can publish the
    /// durable phase completion without waiting for main to consume a packet.
    pub(in super::super) fn close_geometry(&mut self) {
        self.geometry_batch.pending.close();
    }

    /// Restores every model on success, validation errors and joined-worker panics.
    pub(in super::super) fn restore_geometry_states(&mut self) {
        for job in &mut self.geometry_batch.jobs[..self.geometry_batch.active] {
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
            // Placeholders have no live state. Reclaim retains the Vec allocation.
            batch.jobs.clear();
            batch.pending.close();
            let readiness = wait.before_reclaim(&batch.pending);
            let result = batch.pending.reclaim(&mut batch.jobs);
            batch.submitted = false;
            readiness?;
            result?;
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
        let mut output = super::output::GeometryOutput {
            bone_transforms: &mut self.bone_transforms,
            visible_draws: &mut self.visible_draws,
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
        let batch = &mut self.geometry_batch;
        while cursor.next < batch.active {
            let Some(result) = batch
                .pending
                .try_with_result(&batch.handles[cursor.next], |job| output.publish(job, work))?
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
        if batch.active == batch.jobs.len() {
            batch.jobs.push(GeometryJob::default());
        }
        let job = &mut batch.jobs[batch.active];
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
        job.owns_effects = true;
        std::mem::swap(&mut job.particles, &mut placement.particles);
        std::mem::swap(&mut job.ribbons, &mut placement.ribbons);
        std::mem::swap(&mut job.pose, pose);
        std::mem::swap(&mut job.material_poses, material_poses);
        batch.active += 1;
        let mut owned = Some(std::mem::take(job));
        match batch.pending.push(&mut owned) {
            Ok(handle) => batch.handles.push(handle),
            Err(error) => {
                *job = owned.unwrap_or_else(|| unreachable!("rejected job retains its state"));
                std::mem::swap(&mut job.particles, &mut placement.particles);
                std::mem::swap(&mut job.ribbons, &mut placement.ribbons);
                std::mem::swap(&mut job.pose, pose);
                std::mem::swap(&mut job.material_poses, material_poses);
                job.owns_effects = false;
                batch.active -= 1;
                return Err(error.into());
            }
        }
        Ok(())
    }
}
