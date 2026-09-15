//! Owned pure receiver evaluation; retained Rc light ownership stays on main.

use super::{RuntimeTerrainFrameError, SceneLighting};
use glam::Vec3;
use solarity_rendering::{
    M2DirectionalLight, M2LocalLightState, M2SceneUniform, ScenePointLights,
    merge_wotlk_directional_lights,
};

/// Frozen receiver inputs and reusable output storage owned by one phase.
#[derive(Default)]
pub(super) struct LightingWork {
    points: ScenePointLights,
    scenes: Vec<M2SceneUniform>,
    directional: Vec<M2DirectionalLight>,
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
        std::mem::swap(&mut self.points, &mut scene.points);
        std::mem::swap(&mut self.scenes, &mut scene.scenes);
        std::mem::swap(&mut self.directional, &mut scene.directional);
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
            if let Some(last) = self.directional.last_mut() {
                *last = light.unwrap_or(exterior);
            }
            let sunlight = merge_wotlk_directional_lights(&self.directional);
            let mut lights = [M2LocalLightState::disabled(); 4];
            if let Some(sunlight) = sunlight {
                lights[0] = sunlight.local_light_state();
            }
            for (slot, index) in self
                .points
                .query(center, 0.0)?
                .indices()
                .into_iter()
                .flatten()
                .take(3)
                .enumerate()
            {
                lights[slot + 1] = self.points.points()[index].local_light_state();
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
    jobs: Vec<LightingWork>,
    submitted: bool,
}
impl Default for LightingBatch {
    fn default() -> Self {
        Self {
            pending: solarity_cpu::FrameBatch::with_outcome(LightingWork::execute),
            jobs: Vec::new(),
            submitted: false,
        }
    }
}

impl SceneLighting {
    /// Dispatches only after packet-dependent receiver selection. The geometry
    /// completion token establishes the cross-batch dependency explicitly.
    pub(in crate::application::terrain_frame::m2) fn begin_finish(
        &mut self,
        cpu: Option<&solarity_cpu::CpuExecutor>,
        dependency: Option<&solarity_cpu::ReadyToken>,
        base: M2SceneUniform,
        exterior: M2DirectionalLight,
    ) -> Result<(), RuntimeTerrainFrameError> {
        if let Some(cpu) = cpu {
            self.batch.pending.begin_when(
                cpu,
                solarity_cpu::FrameBatchPlan::new(1, 0),
                dependency.ok_or(solarity_cpu::CpuError::BatchInactive)?,
            )?;
            self.batch.submitted = true;
        }
        self.prepare_directionals(exterior);
        let mut job = self.batch.jobs.pop().unwrap_or_default();
        job.swap(self);
        job.base = Some(base);
        job.exterior = Some(exterior);
        job.result = None;
        if self.batch.submitted {
            let mut owned = Some(job);
            if let Err(error) = self.batch.pending.push(&mut owned) {
                let mut job =
                    owned.unwrap_or_else(|| unreachable!("rejected lighting retains state"));
                job.swap(self);
                self.batch.pending.reclaim(&mut self.batch.jobs)?;
                self.batch.submitted = false;
                self.batch.jobs.push(job);
                return Err(error.into());
            }
            self.batch.pending.close();
        } else {
            job.execute();
            self.batch.jobs.push(job);
        }
        Ok(())
    }

    /// Restores all scene storage before surfacing domain or worker failures.
    pub(in crate::application::terrain_frame::m2) fn finish_pending(
        &mut self,
    ) -> Result<(), RuntimeTerrainFrameError> {
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
        let result = job.result.take();
        self.batch.jobs.push(job);
        if let Some(result) = result {
            result?;
        }
        worker?;
        Ok(())
    }
}
