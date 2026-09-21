//! Retained authored M2 resource and ordinary animation for one sky scene model.

use super::*;
use glam::{Vec3, Vec4};

#[cfg(test)]
#[path = "../../../../tests/application/sky_models.rs"]
pub(super) mod tests;

pub(in crate::application) struct SkyM2Model {
    resident: ResidentM2Source,
    gpu: Option<M2GpuSource>,
    playback: Option<M2Playback>,
    #[cfg(test)]
    prepared: SkyPrepared,
    created_tick: u32,
    clock: Option<solarity_rendering::M2AnimationClock>,
    local_light_count: M2LocalLightCount,
}

impl SkyM2Model {
    pub(in crate::application) fn new(resident: ResidentM2Source, created_tick: u32) -> Self {
        Self::with_lights(resident, created_tick, M2LocalLightCount::Zero)
    }

    pub(in crate::application) fn with_lights(
        resident: ResidentM2Source,
        created_tick: u32,
        local_light_count: M2LocalLightCount,
    ) -> Self {
        Self {
            resident,
            created_tick,
            gpu: None,
            playback: None,
            #[cfg(test)]
            prepared: SkyPrepared::default(),
            clock: None,
            local_light_count,
        }
    }

    /// Every resident model belongs to the retained skybox scene, including
    /// models absent from the current LightSkybox slots.
    pub(in crate::application) fn advance(
        &mut self,
        time_ms: u32,
        animations: &AnimationDataCatalog,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        if self.playback.is_none() {
            self.playback = Some(M2Playback::default_sequence(
                self.resident.model(),
                animations,
                0,
                random,
            )?);
        }
        let playback = self
            .playback
            .as_mut()
            .ok_or(solarity_rendering::VulkanError::WorldFrameCapacity)?;
        self.clock = Some(
            playback
                .clock(
                    self.resident.model(),
                    time_ms.wrapping_sub(self.created_tick) as f32,
                    random,
                )?
                .clock,
        );
        Ok(())
    }

    pub(in crate::application) fn primary_span(&self) -> u32 {
        self.playback
            .as_ref()
            .and_then(|playback| playback.script_timer)
            .map_or(0, |timer| {
                timer.end_time_ms().wrapping_sub(timer.start_time_ms())
            })
    }

    /// Native 7ECF20 issues this explicit Stand request after scene advancement.
    pub(in crate::application) fn synchronize_phase(
        &mut self,
        offset: i32,
        speed: f32,
        time_ms: u32,
        animations: &AnimationDataCatalog,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let model = self.resident.model();
        let Some(resolved) = model.animations().resolve_model_animation(animations, 0) else {
            return Ok(());
        };
        let playback = self
            .playback
            .as_mut()
            .ok_or(solarity_rendering::VulkanError::WorldFrameCapacity)?;
        let scene_time = time_ms.wrapping_sub(self.created_tick);
        playback.apply_resolved_model_sequence_variation(
            model,
            resolved.animation_id(),
            None,
            resolved.mode(),
            speed,
            offset,
            scene_time,
            solarity_rendering::M2SequenceStartPhase::BeforeSceneUpdate,
            true,
            random,
        )?;
        self.clock = Some(playback.clock(model, scene_time as f32, random)?.clock);
        Ok(())
    }

    /// Captures only immutable numerical inputs after ordered scene advancement.
    pub(in crate::application) fn seed(
        &mut self,
        renderer: &mut solarity_rendering::GpuPreparation<'_>,
        camera: WorldCameraFrame,
        opacity: f32,
        job: &mut SkyJob,
    ) -> Result<(), RuntimeTerrainFrameError> {
        job.reset();
        if opacity <= 0.0 {
            return Ok(());
        }
        if self.gpu.is_none() {
            self.gpu =
                prepare_source_with_lights(renderer, &self.resident, self.local_light_count)?;
        }
        if let Some(source) = &self.gpu {
            let clock = self
                .clock
                .ok_or(solarity_rendering::VulkanError::WorldFrameCapacity)?;
            job.input = Some(SkyInput {
                source: Arc::clone(source),
                camera,
                clock,
                opacity,
            });
        }
        Ok(())
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(in crate::application) fn prepare(
        &mut self,
        renderer: &mut solarity_rendering::GpuPreparation<'_>,
        camera: WorldCameraFrame,
        time_ms: u32,
        opacity: f32,
        bone_offset: u32,
        animations: &AnimationDataCatalog,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        if opacity > 0. {
            self.advance(time_ms, animations, random)?;
        }
        let mut job = SkyJob {
            prepared: std::mem::take(&mut self.prepared),
            ..Default::default()
        };
        self.seed(renderer, camera, opacity, &mut job)?;
        job.execute();
        self.prepared = job.prepared;
        job.result.take().unwrap_or(Ok(()))?;
        for draw in &mut self.prepared.draws {
            *draw = draw.relocate_bones(bone_offset)?;
        }
        Ok(())
    }

    #[cfg(test)]
    pub(in crate::application) fn bones(&self) -> &[Mat4] {
        self.prepared.bones()
    }
    #[cfg(test)]
    pub(in crate::application) fn draws(&self) -> &[M2PreparedDraw] {
        &self.prepared.draws
    }
}

/// Native sky models use the untranslated scene view and the reserved far range.
pub(in crate::application) fn sky_scene(
    camera: WorldCameraFrame,
) -> solarity_rendering::M2SceneUniform {
    let mut view = camera.view();
    view.w_axis = Vec4::W;
    let depth = Mat4::from_cols(
        Vec4::X,
        Vec4::Y,
        Vec4::new(0., 0., 0.000_976_562_5, 0.),
        Vec4::new(0., 0., 0.999_023_44, 1.),
    );
    solarity_rendering::M2SceneUniform::new(
        depth * camera.projection(),
        view,
        Vec3::ZERO,
        Vec3::ZERO,
        Vec3::ZERO,
        Vec3::Z,
        Vec4::ZERO,
        Vec3::ZERO,
        [solarity_rendering::M2LocalLightState::disabled(); 4],
    )
}

/// Slot-owned output: aliasing skybox slots have independent opacity and offsets.
pub(in crate::application) struct SkyPrepared {
    pose: M2BonePose,
    pub(in crate::application) draws: Vec<M2PreparedDraw>,
    transparent: Vec<(M2TransparentSortKey, M2PreparedDraw)>,
    local_lights: [solarity_rendering::M2LocalLightState; 4],
    directional: Vec<solarity_rendering::M2DirectionalLight>,
    points: Vec<solarity_rendering::M2PointLight>,
}
impl Default for SkyPrepared {
    fn default() -> Self {
        Self {
            pose: M2BonePose::default(),
            draws: Vec::new(),
            transparent: Vec::new(),
            local_lights: [solarity_rendering::M2LocalLightState::disabled(); 4],
            directional: Vec::new(),
            points: Vec::new(),
        }
    }
}
impl SkyPrepared {
    pub(in crate::application) fn bones(&self) -> &[Mat4] {
        if self.draws.is_empty() {
            &[]
        } else {
            self.pose.transforms()
        }
    }
    pub(in crate::application) fn scene(
        &self,
        camera: WorldCameraFrame,
    ) -> solarity_rendering::M2SceneUniform {
        sky_scene(camera).with_local_lights(self.local_lights)
    }
    fn prepare(&mut self, input: &SkyInput) -> Result<(), RuntimeTerrainFrameError> {
        let SkyInput {
            source,
            camera,
            clock,
            opacity,
        } = input;
        let (clock, opacity) = (*clock, *opacity);
        let mut model_view = camera.view();
        model_view.w_axis = Vec4::W;
        self.pose.recompose_with_overrides(
            source.model.animations(),
            clock,
            model_view,
            Default::default(),
        )?;
        sample_m2_lights_into(
            source.model.animations(),
            &self.pose,
            clock,
            Mat4::IDENTITY,
            &mut self.directional,
            &mut self.points,
        )?;
        self.local_lights = [solarity_rendering::M2LocalLightState::disabled(); 4];
        let mut next_light = 0;
        if let Some(sunlight) =
            solarity_rendering::merge_wotlk_directional_lights(&self.directional)
        {
            self.local_lights[0] = sunlight.local_light_state();
            next_light = 1;
        }
        for point in self.points.iter().take(4 - next_light) {
            self.local_lights[next_light] = point.local_light_state();
            next_light += 1;
        }
        if source.mesh.is_none() {
            return Ok(());
        }
        for (draw_index, resources) in source.draws.iter().enumerate() {
            let Some(resources) = resources else {
                continue;
            };
            let draw = &source.plan.draws()[draw_index];
            let pose = M2MaterialPose::sample(&source.model, &source.plan, draw_index, clock)?;
            let color = pose.mesh_color() * Vec4::new(1., 1., 1., opacity);
            let alpha = M2ElementAlphaState::classify(color.w);
            if alpha == M2ElementAlphaState::Hidden {
                continue;
            }
            let state = M2MaterialState::from_material(draw.material());
            let runtime_fade = alpha == M2ElementAlphaState::Translucent && !state.blend_enabled();
            let template = if runtime_fade {
                resources.fade_template.ok_or_else(|| {
                    RuntimeTerrainFrameError::M2RuntimeFadePipeline {
                        model: source.model.path().clone(),
                        draw_index,
                    }
                })?
            } else {
                resources.template
            };
            let material = M2MaterialUniform::new(
                Mat4::IDENTITY,
                pose.texture_transforms(),
                model_view,
                color,
                Vec4::ZERO,
                Vec4::new(state.alpha_reference(opacity), 0., 0., 0.),
            );
            let prepared = template.instantiate(material, 0, 0)?;
            if draw.transparent_sort_unit() || alpha == M2ElementAlphaState::Translucent {
                let distance = section_distance_key(draw, &self.pose, model_view)?;
                let primary = if source.model.skin_profile_count() >= 2 {
                    m2_model_distance_key(model_view)
                } else {
                    distance
                };
                self.transparent.push((
                    M2TransparentSortKey::new(
                        primary,
                        false,
                        i16::from(draw.batch().priority_plane),
                        distance,
                        0,
                        draw.batch().material_layer,
                    )
                    .with_scene_element(0, self.transparent.len() as u32),
                    prepared,
                ));
            } else {
                self.draws.push(prepared);
            }
        }
        self.transparent
            .sort_unstable_by(|(a, _), (b, _)| compare_m2_transparent(a, b));
        self.draws
            .extend(self.transparent.iter().map(|(_, draw)| *draw));
        Ok(())
    }
}
struct SkyInput {
    source: M2GpuSource,
    camera: WorldCameraFrame,
    clock: solarity_rendering::M2AnimationClock,
    opacity: f32,
}
#[derive(Default)]
pub(in crate::application) struct SkyJob {
    input: Option<SkyInput>,
    pub(in crate::application) prepared: SkyPrepared,
    result: Option<Result<(), RuntimeTerrainFrameError>>,
}
impl SkyJob {
    pub(in crate::application) fn reset(&mut self) {
        self.input = None;
        self.result = None;
        self.prepared.draws.clear();
        self.prepared.transparent.clear();
        self.prepared.local_lights = [solarity_rendering::M2LocalLightState::disabled(); 4];
    }
    fn execute(&mut self) {
        self.result = Some(match self.input.take() {
            Some(input) => self.prepared.prepare(&input),
            None => Ok(()),
        });
    }
}
/// Five fixed ordered consumers: stars, three ordinary slots, global skybox.
pub(in crate::application) struct SkyBatch {
    pub(in crate::application) jobs: Vec<SkyJob>,
    batch: solarity_cpu::FrameBatch<SkyJob>,
}
impl Default for SkyBatch {
    fn default() -> Self {
        Self {
            jobs: (0..5).map(|_| SkyJob::default()).collect(),
            batch: solarity_cpu::FrameBatch::new(SkyJob::execute),
        }
    }
}
impl SkyBatch {
    pub(in crate::application) fn run(
        &mut self,
        cpu: &solarity_cpu::CpuExecutor,
        wait: &mut crate::application::frame_pipeline::FrameWait<'_>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        if self.jobs.iter().all(|job| job.input.is_none()) {
            return Ok(());
        }
        let started = self.batch.start(cpu, &mut self.jobs);
        if let Err(error) = started {
            for job in &mut self.jobs {
                job.input = None;
            }
            return Err(error.into());
        }
        self.finish(wait)
    }
    fn finish(
        &mut self,
        wait: &mut crate::application::frame_pipeline::FrameWait<'_>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let ready = wait.before_reclaim(&self.batch);
        let reclaimed = self.batch.reclaim(&mut self.jobs);
        // Even a panic or native failure releases every retained source generation.
        for job in &mut self.jobs {
            job.input = None;
        }
        ready?;
        reclaimed?;
        for job in &mut self.jobs {
            job.result.take().unwrap_or(Ok(()))?;
        }
        Ok(())
    }
}
