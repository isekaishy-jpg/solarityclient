//! Frozen WMO traversal on shared workers, with ordered publication and pin reclamation.
use super::super::shadow::WorldShadowAdmission;
use super::*;
use crate::application::frame_pipeline::FrameWait;
use glam::Mat4;
use solarity_cpu::{
    CpuBuffer, CpuError, CpuExecutor, CpuStorageClass as Class, CpuStorageKind as Kind, FrameBatch,
    JobOutcome,
};
use solarity_systems::WorldSceneFrustum;

struct VisibleInput {
    source: Arc<WorldModelGpuSource>,
    transform: Mat4,
    group: usize,
    scene_index: usize,
    frusta: std::ops::Range<usize>,
    fog: Vec3,
}
#[derive(Default)]
struct VisibleJob {
    inputs: CpuBuffer<VisibleInput>,
    frusta: CpuBuffer<WorldSceneFrustum>,
    selected: CpuBuffer<usize>,
    visible: CpuBuffer<bool>,
    draws: CpuBuffer<WorldModelPreparedDraw>,
    emissive: f32,
    outdoor_fog: Vec3,
    result: Option<Result<Option<usize>, RuntimeTerrainFrameError>>,
}
impl VisibleJob {
    fn run(&mut self) -> Result<Option<usize>, RuntimeTerrainFrameError> {
        let mut last_group = None;
        self.draws.clear();
        for input in self.inputs.iter() {
            let source = &input.source;
            let group = source.plan.groups().get(input.group).ok_or(
                RuntimeTerrainFrameError::WorldModelGroupIndex {
                    group_index: input.group,
                    group_count: source.plan.groups().len(),
                },
            )?;
            let range = group.draw_range();
            last_group = Some(input.scene_index);
            let selected = WorldModelBatchVisibilityQuery::query_admitted(
                &source.draw_bounds[range.clone()],
                &self.frusta[input.frusta.clone()],
                &mut self.visible.writer()[..range.len()],
                &mut self.selected,
            )?;
            for &batch in selected {
                let resources = &source.draws[range.start + batch];
                for template in resources.passes[..resources.pass_count].iter() {
                    let template = template
                        .as_ref()
                        .ok_or(VulkanError::WorldModelDrawMaterial)?;
                    self.draws.push(
                        template
                            .instantiate(input.transform, self.emissive, input.fog)
                            .with_outdoor_fog_color(self.outdoor_fog),
                    )?;
                }
            }
        }
        Ok(last_group)
    }
}

pub(super) struct ShadowInput {
    pub(super) source: Arc<WorldModelGpuSource>,
    pub(super) transform: Mat4,
    pub(super) owner: WorldModelGpuPlacementOwner,
}
#[derive(Default)]
pub(super) struct ShadowJob {
    pub(super) inputs: CpuBuffer<ShadowInput>,
    pub(super) admission: Option<WorldShadowAdmission>,
    pub(super) draws: CpuBuffer<solarity_rendering::WorldEnvironmentWmoCaster>,
    pub(super) doodads: CpuBuffer<((RuntimeWorldModelMovementOwner, usize), u8)>,
    result: Option<Result<(), RuntimeTerrainFrameError>>,
}
pub(super) struct WorldModelPreparation {
    visible: Vec<VisibleJob>,
    visible_batch: FrameBatch<VisibleJob>,
    shadow: Vec<ShadowJob>,
    shadow_batch: FrameBatch<ShadowJob>,
    visible_started: bool,
    shadow_started: bool,
}
impl Default for WorldModelPreparation {
    fn default() -> Self {
        Self {
            visible: Vec::new(),
            visible_batch: FrameBatch::with_context(|job, _context| {
                job.result = Some(job.run());
                job.inputs.clear();
                job.frusta.clear();
                JobOutcome::Succeeded
            }),
            shadow: vec![ShadowJob::default()],
            shadow_batch: FrameBatch::with_context(|job, _context| {
                job.result = Some(job.run());
                job.inputs.clear();
                job.admission = None;
                JobOutcome::Succeeded
            }),
            visible_started: false,
            shadow_started: false,
        }
    }
}
impl WorldModelFrame {
    /// Captures WMO source generations while their renderer resources stay owned on main.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(in crate::application::terrain_frame) fn begin_preparation(
        &mut self,
        cpu: &CpuExecutor,
        groups: &[WorldModelSceneGroup],
        admission: Option<&WorldShadowAdmission>,
        emissive: f32,
        outdoor_fog: Vec3,
        indoor_fog: Vec3,
    ) -> Result<PendingWorldModels<'_>, RuntimeTerrainFrameError> {
        let mut pending = PendingWorldModels { owner: self };
        pending.start_visible(cpu, groups, emissive, outdoor_fog, indoor_fog)?;
        pending.start_shadow(cpu, admission)?;
        Ok(pending)
    }

    /// Shadow inputs are independent of the portal scene rebuilt during M2 entry.
    pub(in crate::application::terrain_frame) fn begin_shadow_preparation(
        &mut self,
        cpu: &CpuExecutor,
        admission: Option<&WorldShadowAdmission>,
    ) -> Result<PendingWorldModels<'_>, RuntimeTerrainFrameError> {
        let mut pending = PendingWorldModels { owner: self };
        pending.start_shadow(cpu, admission)?;
        Ok(pending)
    }

    #[cfg(test)]
    pub(super) fn prepare_visible_draws(
        &mut self,
        _renderer: &mut VulkanRenderer,
        groups: &[WorldModelSceneGroup],
        emissive: f32,
        outdoor_fog: Vec3,
        indoor_fog: Vec3,
    ) -> Result<WorldModelVisibleFrame<'_>, RuntimeTerrainFrameError> {
        let cpu = crate::frame_cpu_support::executor()?;
        let mut pending =
            self.begin_preparation(&cpu, groups, None, emissive, outdoor_fog, indoor_fog)?;
        pending.finish_visible(&mut FrameWait::Offline)?;
        drop(pending);
        Ok(self.visible_frame())
    }
}
/// Exceptional exits still restore reusable buffers and release every frozen source pin.
pub(in crate::application::terrain_frame) struct PendingWorldModels<'a> {
    owner: &'a mut WorldModelFrame,
}
impl PendingWorldModels<'_> {
    pub(in crate::application::terrain_frame) fn frame(&self) -> &WorldModelFrame {
        self.owner
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::application::terrain_frame) fn start_visible(
        &mut self,
        cpu: &CpuExecutor,
        groups: &[WorldModelSceneGroup],
        emissive: f32,
        outdoor_fog: Vec3,
        indoor_fog: Vec3,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let frame = &mut self.owner;
        let prep = &mut frame.preparation;
        let count = groups
            .len()
            .div_ceil(8)
            .min(cpu.worker_count().saturating_mul(4));
        let width = groups.len().div_ceil(count.max(1));
        prep.visible
            .resize_with(groups.len().div_ceil(width.max(1)), VisibleJob::default);
        let mut total_draws = 0;
        for (job, (offset, groups)) in prep
            .visible
            .iter_mut()
            .zip(groups.chunks(width.max(1)).enumerate())
        {
            job.inputs.clear();
            job.frusta.clear();
            job.draws.clear();
            job.result = None;
            job.emissive = emissive;
            job.outdoor_fog = outdoor_fog;
            job.inputs
                .reserve(cpu.storage(), Class::Frame, Kind::Metadata, groups.len())?;
            job.frusta.reserve(
                cpu.storage(),
                Class::Frame,
                Kind::Metadata,
                groups.iter().map(|g| g.frusta.len()).sum(),
            )?;
            let mut capacity = 0;
            let mut maximum = 0;
            for (index, group) in groups.iter().enumerate() {
                let Some(&placement_index) = frame.placement_indices.get(&group.owner) else {
                    continue;
                };
                let placement = &frame.placements[placement_index];
                if !placement.placement_valid {
                    continue;
                }
                let source = frame.sources[placement.source_index].as_ref().ok_or(
                    RuntimeTerrainFrameError::WorldModelSourceIndex {
                        source_index: placement.source_index,
                        source_count: frame.sources.len(),
                    },
                )?;
                if let Some(plan) = source.plan.groups().get(group.group) {
                    let range = plan.draw_range();
                    maximum = maximum.max(range.len());
                    capacity += source.draws[range]
                        .iter()
                        .map(|draw| draw.pass_count)
                        .sum::<usize>();
                }
                let start = job.frusta.len();
                job.frusta.extend_from_slice(&group.frusta)?;
                job.inputs.push(VisibleInput {
                    source: Arc::clone(source),
                    transform: placement.plan.transform(),
                    group: group.group,
                    scene_index: offset * width + index,
                    frusta: start..job.frusta.len(),
                    fog: if group.indoor_fog {
                        indoor_fog
                    } else {
                        outdoor_fog
                    },
                })?;
            }
            job.selected
                .reserve(cpu.storage(), Class::Frame, Kind::Scratch, maximum)?;
            job.visible
                .reserve(cpu.storage(), Class::Frame, Kind::Scratch, maximum)?;
            job.visible.clear();
            for _ in 0..maximum {
                job.visible.push(false)?;
            }
            job.draws
                .reserve(cpu.storage(), Class::Frame, Kind::Result, capacity)?;
            total_draws += capacity;
        }
        frame.prepared_draws.clear();
        frame
            .prepared_draws
            .reserve(cpu.storage(), Class::Frame, Kind::Result, total_draws)?;
        frame.prepared_last_group = None;
        if !prep.visible.is_empty() {
            prep.visible_batch.start(cpu, &mut prep.visible)?;
            prep.visible_started = true;
        }
        Ok(())
    }
    fn start_shadow(
        &mut self,
        cpu: &CpuExecutor,
        admission: Option<&WorldShadowAdmission>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let frame = &mut self.owner;
        let prep = &mut frame.preparation;
        let shadow = &mut prep.shadow[0];
        shadow.inputs.clear();
        shadow.draws.clear();
        shadow.doodads.clear();
        shadow.result = None;
        shadow.admission = admission.copied();
        if admission.is_some() {
            shadow.inputs.reserve(
                cpu.storage(),
                Class::Frame,
                Kind::Metadata,
                frame.placements.len(),
            )?;
            let mut draws = 0;
            let mut doodads = 0;
            for placement in &frame.placements {
                if !placement.placement_valid {
                    continue;
                }
                let source = frame.sources[placement.source_index].as_ref().ok_or(
                    RuntimeTerrainFrameError::WorldModelSourceIndex {
                        source_index: placement.source_index,
                        source_count: frame.sources.len(),
                    },
                )?;
                draws += source.shadow_templates.len();
                doodads += source
                    .model
                    .groups()
                    .iter()
                    .map(|group| group.doodad_references().len())
                    .sum::<usize>();
                shadow.inputs.push(ShadowInput {
                    source: Arc::clone(source),
                    transform: placement.plan.transform(),
                    owner: placement.owner,
                })?;
            }
            shadow
                .draws
                .reserve(cpu.storage(), Class::Frame, Kind::Result, draws)?;
            shadow
                .doodads
                .reserve(cpu.storage(), Class::Frame, Kind::Result, doodads)?;
            prep.shadow_batch.start(cpu, &mut prep.shadow)?;
            prep.shadow_started = true;
        }
        Ok(())
    }
    pub(in crate::application::terrain_frame) fn finish_shadow(
        &mut self,
        wait: &mut FrameWait<'_>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let frame = &mut self.owner;
        let prep = &mut frame.preparation;
        if prep.shadow_started {
            let ready = wait.before_reclaim(&prep.shadow_batch);
            let reclaimed = prep.shadow_batch.reclaim(&mut prep.shadow);
            prep.shadow_started = false;
            ready?;
            reclaimed?;
            prep.shadow[0]
                .result
                .take()
                .ok_or(CpuError::CompletionLost)??;
        }
        let job = &mut prep.shadow[0];
        std::mem::swap(&mut frame.shadow_draws, &mut job.draws);
        frame.shadow_doodads.clear();
        for (key, maps) in job.doodads.drain() {
            *frame.shadow_doodads.entry(key).or_default() |= maps;
        }
        Ok(())
    }
    pub(in crate::application::terrain_frame) fn finish_visible(
        &mut self,
        wait: &mut FrameWait<'_>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let frame = &mut self.owner;
        let prep = &mut frame.preparation;
        if prep.visible_started {
            let ready = wait.before_reclaim(&prep.visible_batch);
            let reclaimed = prep.visible_batch.reclaim(&mut prep.visible);
            prep.visible_started = false;
            ready?;
            reclaimed?;
        }
        for job in &mut prep.visible {
            if let Some(last) = job.result.take().ok_or(CpuError::CompletionLost)?? {
                frame.prepared_last_group = Some(last);
            }
            frame.prepared_draws.extend_from_slice(&job.draws)?;
        }
        Ok(())
    }
}
impl Drop for PendingWorldModels<'_> {
    fn drop(&mut self) {
        let prep = &mut self.owner.preparation;
        if prep.visible_started {
            let _ = prep.visible_batch.reclaim(&mut prep.visible);
            prep.visible_started = false;
        }
        if prep.shadow_started {
            let _ = prep.shadow_batch.reclaim(&mut prep.shadow);
            prep.shadow_started = false;
        }
        for job in &mut prep.visible {
            job.inputs.clear();
            job.frusta.clear();
            job.result = None;
        }
        for job in &mut prep.shadow {
            job.inputs.clear();
            job.admission = None;
            job.result = None;
        }
    }
}
