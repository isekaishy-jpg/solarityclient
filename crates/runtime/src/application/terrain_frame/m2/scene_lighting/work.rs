//! Owned pure receiver evaluation; retained Rc light ownership stays on main.

use super::{RuntimeTerrainFrameError, SceneLightSources, SceneLighting};
use glam::Vec3;
use solarity_rendering::{
    M2DirectionalLight, M2LocalLightState, M2SceneUniform, merge_wotlk_directional_lights,
};
use std::sync::Arc;

/// Frozen receiver inputs and reusable output storage owned by one phase.
#[derive(Default)]
pub(super) struct LightingWork {
    sources: Option<Arc<SceneLightSources>>,
    scenes: Vec<M2SceneUniform>,
    centers: Vec<Vec3>,
    placement_centers: Vec<Option<Vec3>>,
    receiver_lights: Vec<Option<M2DirectionalLight>>,
    receiver_fog: Vec<Option<Vec3>>,
    placement_lights: Vec<Option<M2DirectionalLight>>,
    placement_parents: Vec<Option<usize>>,
    receiver_placements: Vec<usize>,
    placement_end: usize,
    base: Option<M2SceneUniform>,
    exterior: Option<M2DirectionalLight>,
    result: Option<Result<(), RuntimeTerrainFrameError>>,
}

impl LightingWork {
    /// Transfers vector ownership without cloning the spatial bank or receiver lists.
    pub(super) fn swap(&mut self, scene: &mut SceneLighting) {
        std::mem::swap(&mut self.scenes, &mut scene.scenes);
        std::mem::swap(&mut self.centers, &mut scene.centers);
        std::mem::swap(&mut self.placement_centers, &mut scene.placement_centers);
        std::mem::swap(&mut self.receiver_lights, &mut scene.receiver_lights);
        std::mem::swap(&mut self.receiver_fog, &mut scene.receiver_fog);
        std::mem::swap(&mut self.placement_lights, &mut scene.placement_lights);
        std::mem::swap(&mut self.placement_parents, &mut scene.placement_parents);
        std::mem::swap(
            &mut self.receiver_placements,
            &mut scene.receiver_placements,
        );
        std::mem::swap(&mut self.placement_end, &mut scene.placement_end);
    }

    /// Samples the frozen bank after every ordered receiver callback has completed.
    fn evaluate(
        &mut self,
        base: M2SceneUniform,
        exterior: M2DirectionalLight,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let sources = self
            .sources
            .as_ref()
            .unwrap_or_else(|| unreachable!("receiver work pins its published light sources"));
        sources.trace.link("m2.light_sources.receiver");
        for (((center, light), fog), placement) in self
            .centers
            .iter()
            .zip(&self.receiver_lights)
            .zip(&self.receiver_fog)
            .zip(&self.receiver_placements)
        {
            // A vehicle may be published after its passenger. Resolve only
            // after every source and entity callback has joined this frame.
            let (mut center, mut light) = (*center, *light);
            let mut current = *placement;
            for _ in 0..self.placement_end {
                let Some(parent) = self.placement_parents.get(current).copied().flatten() else {
                    break;
                };
                if parent == *placement {
                    break;
                }
                if let Some(value) = self.placement_centers.get(parent).copied().flatten() {
                    center = value;
                }
                light = self
                    .placement_lights
                    .get(parent)
                    .copied()
                    .flatten()
                    .or(light);
                current = parent;
            }
            let exterior = light.unwrap_or(exterior);
            // Preserve stock's linked-list sources followed by this receiver's
            // exterior contribution, without modifying the shared source bank.
            let sunlight = merge_wotlk_directional_lights(
                sources
                    .directionals
                    .iter()
                    .chain(std::iter::once(&exterior)),
            );
            let mut lights = [M2LocalLightState::disabled(); 4];
            if let Some(sunlight) = sunlight {
                lights[0] = sunlight.local_light_state();
            }
            for (slot, index) in sources
                .points
                .query(center, 0.0)?
                .indices()
                .into_iter()
                .flatten()
                .take(3)
                .enumerate()
            {
                lights[slot + 1] = sources.points.points()[index].local_light_state();
            }
            let scene = base.with_local_lights(lights);
            self.scenes
                .push(fog.map_or(scene, |color| scene.with_fog_color(color)));
        }
        Ok(())
    }

    /// Keeps the domain error in owned state and suppresses dependent phases on failure.
    fn execute(&mut self) -> solarity_cpu::JobOutcome {
        let _profile = solarity_profiling::profile!("m2.scene_lighting.evaluate");
        let base = self
            .base
            .take()
            .unwrap_or_else(|| unreachable!("lighting phase owns base scene"));
        let exterior = self
            .exterior
            .take()
            .unwrap_or_else(|| unreachable!("lighting phase owns exterior light"));
        self.result = Some(self.evaluate(base, exterior));
        if self.result.as_ref().is_some_and(Result::is_ok) {
            solarity_cpu::JobOutcome::Succeeded
        } else {
            solarity_cpu::JobOutcome::Failed
        }
    }
}

/// One retained typed phase; the vectors, not the Rc ownership graph, cross workers.
pub(super) struct LightingBatch {
    pending: solarity_cpu::FrameBatch<LightingWork>,
    template: solarity_cpu::FrameGraphTemplate,
    jobs: Vec<LightingWork>,
    submitted: bool,
}
impl Default for LightingBatch {
    fn default() -> Self {
        Self {
            pending: solarity_cpu::FrameBatch::with_outcome(LightingWork::execute),
            template: solarity_cpu::FrameGraphTemplate::independent(1)
                .with_priority(solarity_cpu::FramePriority::Prerequisite),
            jobs: Vec::new(),
            submitted: false,
        }
    }
}

impl SceneLighting {
    pub(in crate::application::terrain_frame::m2) fn has_pending(&self) -> bool {
        self.batch.submitted
    }

    pub(in crate::application::terrain_frame::m2) fn is_ready(&self) -> bool {
        !self.batch.submitted || self.batch.pending.is_finished()
    }

    /// The main driver parks only after running its other permitted continuations.
    pub(in crate::application::terrain_frame::m2) fn wait_pending(
        &self,
        wait: &mut crate::application::frame_pipeline::FrameWait<'_>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        wait.before_reclaim(&self.batch.pending)?;
        Ok(())
    }

    /// Dispatches only after packet-dependent receiver selection. The geometry
    /// completion token establishes the cross-batch dependency explicitly.
    pub(in crate::application::terrain_frame::m2) fn begin_finish(
        &mut self,
        cpu: Option<&solarity_cpu::CpuExecutor>,
        dependency: Option<&solarity_cpu::ReadyToken>,
        base: M2SceneUniform,
        exterior: M2DirectionalLight,
    ) -> Result<(), RuntimeTerrainFrameError> {
        // Validate the external identity before changing the retained light bank.
        let dependencies = match cpu {
            Some(_) => {
                std::slice::from_ref(dependency.ok_or(solarity_cpu::CpuError::BatchInactive)?)
            }
            None => &[],
        };
        self.prepare_directionals();
        let mut job = self.batch.jobs.pop().unwrap_or_default();
        job.swap(self);
        job.sources = Some(Arc::clone(&self.sources));
        job.base = Some(base);
        job.exterior = Some(exterior);
        job.result = None;
        self.batch.jobs.push(job);
        if let Some(cpu) = cpu {
            if let Err(error) = self.batch.pending.start_graph(
                cpu,
                &self.batch.template,
                &mut self.batch.jobs,
                dependencies,
            ) {
                let mut job = self
                    .batch
                    .jobs
                    .pop()
                    .unwrap_or_else(|| unreachable!("rejected lighting retains state"));
                job.swap(self);
                job.sources = None;
                self.batch.jobs.push(job);
                return Err(error.into());
            }
            self.batch.submitted = true;
        } else {
            self.batch
                .jobs
                .last_mut()
                .unwrap_or_else(|| unreachable!("inline lighting owns input"))
                .execute();
        }
        Ok(())
    }

    /// Restores all scene storage before surfacing domain or worker failures.
    pub(in crate::application::terrain_frame::m2) fn finish_pending(
        &mut self,
        wait: &mut crate::application::frame_pipeline::FrameWait<'_>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let readiness = wait.before_reclaim(&self.batch.pending);
        let worker = if self.batch.submitted {
            self.batch.submitted = false;
            self.batch.pending.reclaim(&mut self.batch.jobs)
        } else {
            Ok(())
        };
        let mut job = self
            .batch
            .jobs
            .pop()
            .unwrap_or_else(|| unreachable!("lighting phase retains its job"));
        job.swap(self);
        // Release the worker pin before next-frame mutation, including failure.
        job.sources = None;
        let result = job.result.take();
        self.batch.jobs.push(job);
        if let Some(result) = result {
            result?;
        }
        readiness?;
        worker?;
        Ok(())
    }
}
