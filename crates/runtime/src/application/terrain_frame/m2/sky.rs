//! Retained authored M2 resource and ordinary animation for one sky scene model.

use super::*;
use glam::{Vec3, Vec4};

#[cfg(test)]
#[path = "../../../../tests/application/sky_models.rs"]
mod tests;

pub(in crate::application) struct SkyM2Model {
    resident: ResidentM2Source,
    gpu: Option<M2GpuSource>,
    playback: Option<M2Playback>,
    pose: M2BonePose,
    draws: Vec<M2PreparedDraw>,
    transparent: Vec<(M2TransparentSortKey, M2PreparedDraw)>,
    created_tick: u32,
    clock: Option<solarity_rendering::M2AnimationClock>,
    local_light_count: M2LocalLightCount,
    local_lights: [solarity_rendering::M2LocalLightState; 4],
    directional: Vec<solarity_rendering::M2DirectionalLight>,
    points: Vec<solarity_rendering::M2PointLight>,
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
            pose: M2BonePose::default(),
            draws: Vec::new(),
            transparent: Vec::new(),
            clock: None,
            local_light_count,
            local_lights: [solarity_rendering::M2LocalLightState::disabled(); 4],
            directional: Vec::new(),
            points: Vec::new(),
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

    #[allow(clippy::too_many_arguments)]
    pub(in crate::application) fn prepare(
        &mut self,
        renderer: &mut VulkanRenderer,
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
        self.prepare_current(renderer, camera, opacity, bone_offset)
    }

    pub(in crate::application) fn prepare_current(
        &mut self,
        renderer: &mut VulkanRenderer,
        camera: WorldCameraFrame,
        opacity: f32,
        bone_offset: u32,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.draws.clear();
        self.transparent.clear();
        if opacity <= 0.0 {
            return Ok(());
        }
        if self.gpu.is_none() {
            self.gpu =
                prepare_source_with_lights(renderer, &self.resident, self.local_light_count)?;
        }
        let Some(source) = &self.gpu else {
            return Ok(());
        };
        let clock = self
            .clock
            .ok_or(solarity_rendering::VulkanError::WorldFrameCapacity)?;
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
        let Some(mesh) = source.mesh else {
            return Ok(());
        };
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
            let pipeline = if runtime_fade {
                resources.runtime_fade_pipeline.ok_or_else(|| {
                    RuntimeTerrainFrameError::M2RuntimeFadePipeline {
                        model: source.model.path().clone(),
                        draw_index,
                    }
                })?
            } else {
                resources.pipeline
            };
            let material = M2MaterialUniform::new(
                Mat4::IDENTITY,
                pose.texture_transforms(),
                model_view,
                color,
                Vec4::ZERO,
                Vec4::new(state.alpha_reference(opacity), 0., 0., 0.),
            );
            let prepared = renderer.prepare_m2_draw(
                mesh,
                pipeline,
                resources.texture_set,
                &source.plan,
                draw_index,
                runtime_fade,
                material,
                bone_offset,
                0,
            )?;
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

    pub(in crate::application) fn bones(&self) -> &[Mat4] {
        if self.draws.is_empty() {
            &[]
        } else {
            self.pose.transforms()
        }
    }
    pub(in crate::application) fn draws(&self) -> &[M2PreparedDraw] {
        &self.draws
    }

    pub(in crate::application) fn scene(
        &self,
        camera: WorldCameraFrame,
    ) -> solarity_rendering::M2SceneUniform {
        sky_scene(camera).with_local_lights(self.local_lights)
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
        depth * camera.projection() * view,
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
