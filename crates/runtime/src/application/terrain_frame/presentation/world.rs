//! Ordered world-frame driver with resumable model admission between main steps.

use super::super::{RuntimeTerrainFrameError, TerrainFrame, m2, shadow, world_model};
use crate::application::environment_coordinator::RuntimeWorldEnvironmentFrame;
use crate::application::game_object_coordinator::GameObjectFrameInput;
use crate::application::liquid::{liquid_depth_images, liquid_environment};
use crate::application::player_coordinator::{
    ResidentCreatureFrameInput, ResidentPlayerFrameInput,
};
use crate::random::CrtRand;
use glam::Vec4;
use solarity_asset::TerrainTileIndex;
use solarity_rendering::{
    M2LocalLightState, M2SceneUniform, TerrainSceneUniform, UiPreparedDraw, VulkanRenderer,
    WorldCameraFrame, WorldFrameReport, WorldFrameScene, WorldFrustum, WorldModelSceneUniform,
    WorldScreenWindow,
};
use std::sync::Arc;

impl TerrainFrame {
    /// Culls, lights, records, and presents one resident terrain frame.
    ///
    /// The visible packet buffer is retained across frames. An empty result is
    /// still presented as a cleared world attachment; looking away from the
    /// resident ADT is valid camera state, not a rendering failure.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::application) fn present(
        &mut self,
        renderer: &mut VulkanRenderer,
        cpu: &solarity_cpu::CpuExecutor,
        wait: &mut crate::application::frame_pipeline::FrameWait<'_>,
        plan: Option<TerrainTileIndex>,
        environment: RuntimeWorldEnvironmentFrame,
        terrain: &mut crate::application::terrain_coordinator::RuntimeTerrainCoordinator,
        liquid_types: &solarity_asset::LiquidTypeCatalog,
        camera: WorldCameraFrame,
        liquid_time_ms: u32,
        texture_animation: &solarity_rendering::TerrainTextureAnimationState,
        screen_effect: Option<solarity_rendering::WorldFrameScreenEffect>,
        camera_submerged: bool,
        specular_enabled: bool,
        ripples: Option<solarity_rendering::WaterRippleFrame<'_>>,
        underwater_particles: Option<solarity_rendering::UnderwaterParticleFrame<'_>>,
        celestial_resources: crate::application::sky_resources::RuntimeCelestialResources,
        sky_resources: &mut crate::application::sky_resources::RuntimeSkyResources,
        unit_effect_sources: Option<Arc<m2::unit_effects::M2UnitEffectSources>>,
        unit_effect_callback: Option<&mut m2::unit_effects::UnitEffectEventCallback<'_>>,
        random: &mut CrtRand,
        player: ResidentPlayerFrameInput<'_>,
        creatures: &[ResidentCreatureFrameInput<'_>],
        remote_players: &[ResidentPlayerFrameInput<'_>],
        game_objects: GameObjectFrameInput<'_>,
        ui_extent: [f32; 2],
        ui_draws: &[UiPreparedDraw],
        ui_overlay_draws: &[UiPreparedDraw],
    ) -> Result<WorldFrameReport, RuntimeTerrainFrameError> {
        let mut profile = solarity_profiling::profile!("World scene preparation");
        match (self.tile, plan) {
            (Some(frame), Some(plan)) if frame != plan => {
                return Err(RuntimeTerrainFrameError::TileMismatch {
                    frame_x: frame.x(),
                    frame_y: frame.y(),
                    plan_x: plan.x(),
                    plan_y: plan.y(),
                });
            }
            (Some(_), None) | (None, Some(_)) => {
                return Err(RuntimeTerrainFrameError::SceneKindMismatch);
            }
            (Some(_), Some(_)) | (None, None) => {}
        }
        let local_animation_time_ms = self.m2.animation_time_ms();
        self.m2
            .update_game_object_states(game_objects, local_animation_time_ms, random)?;
        self.world_models.update_game_object_states(game_objects)?;
        profile.mark("object states");
        let frustum = WorldFrustum::new(camera, WorldScreenWindow::FULL)?;
        let light = environment.light();
        // 7816F0 uses the preceding post-world glare update for these two
        // exterior channels; cloud/sky, fog, and specular colors are separate.
        let glare_lighting = renderer.world_glare_lighting();
        let ambient = glare_lighting.apply(light.ambient_color());
        let diffuse = glare_lighting.apply(light.diffuse_color());
        self.sky.update(
            environment,
            camera,
            liquid_time_ms,
            celestial_resources.colors,
        );
        profile.mark("sky update");
        let fog = environment.fog();
        let (fog_start, fog_end) = fog.range();
        let fog_parameters = Vec4::new(fog_start, fog_end, 0.0, fog.exponent());
        let terrain_scene = TerrainSceneUniform::new(
            camera.projection(),
            camera.view(),
            ambient,
            diffuse,
            environment.light_direction(),
        )
        .with_fog(camera.view(), fog_parameters, fog.color())
        .with_texture_animation(texture_animation)
        .with_specular(light.specular_color(), specular_enabled);
        let world_model_scene = WorldModelSceneUniform::new(
            camera.projection(),
            camera.view(),
            camera.camera().position(),
            ambient,
            diffuse,
            environment.light_direction(),
            fog_parameters,
        );
        let m2_scene = M2SceneUniform::new(
            camera.projection(),
            camera.view(),
            camera.camera().position(),
            glam::Vec3::ZERO,
            glam::Vec3::ZERO,
            environment.light_direction(),
            fog_parameters,
            fog.color(),
            [M2LocalLightState::disabled(); 4],
        )
        .with_specular_enabled(specular_enabled);
        profile.mark("scene uniforms");
        self.m2
            .update_player_state(player, local_animation_time_ms, random)?;
        self.m2
            .update_creature_states(creatures, local_animation_time_ms, random)?;
        self.m2
            .update_remote_player_states(remote_players, local_animation_time_ms, random)?;
        profile.mark("unit states");
        if let Some(sources) = unit_effect_sources {
            self.m2.set_unit_effect_sources(sources);
        }
        // 874210 registers extShadowQuality=2. 7BB3E0 centers its primary map
        // on the controlled unit; 7BB570 consumes the raw day/night ray.
        let shadow_projection = self
            .shadow_quality
            .texture_size()
            .map(|_| {
                solarity_rendering::WorldShadowProjection::primary(
                    self.shadow_quality,
                    environment.position(),
                    camera.camera().position(),
                    solarity_asset::exterior_light_ray_at(environment.day_fraction()),
                )
                .map(|projection| projection.with_camera_culling(camera))
            })
            .transpose()?;
        // Retain the last submitted cache until this frame succeeds. The small
        // CPU copy keeps skipped or failed submissions from publishing regions.
        let mut pending_environment = if self.shadow_quality.shader_mode() >= 2 {
            Some(self.environment_shadows.clone().unwrap_or_else(|| {
                solarity_rendering::WorldEnvironmentShadowState::new(self.shadow_quality)
            }))
        } else {
            None
        };
        let ray = solarity_asset::exterior_light_ray_at(environment.day_fraction());
        let environment_frame = if let Some(state) = &mut pending_environment {
            let updates = state.advance(environment.position())?;
            Some(solarity_rendering::WorldEnvironmentShadowFrame::new(
                state,
                updates,
                camera.camera().position(),
                ray,
            )?)
        } else {
            None
        };
        let shadow_admission = shadow_projection
            .zip(environment_frame)
            .map(|(primary, frame)| shadow::WorldShadowAdmission::new(primary, frame, camera, ray))
            .transpose()?;
        self.world_models
            .prepare_shadow_draws(renderer, shadow_admission.as_ref())?;
        let entity_light_environment = solarity_systems::WorldEntityLightEnvironment::new(
            ambient,
            diffuse,
            -environment.light_direction(),
            solarity_asset::exterior_light_ray_at(environment.day_fraction()),
        );
        if self.main_preparation.is_none() {
            self.main_preparation = Some(super::WorldMainPreparation::new(cpu)?);
        }
        let mut main = self
            .main_preparation
            .as_mut()
            .unwrap_or_else(|| unreachable!("world main preparation was initialized"))
            .begin(cpu)?;
        let mut pending_m2 = self.m2.begin_visible_draws_with_unit_effects(
            renderer,
            cpu,
            frustum,
            camera,
            if camera_submerged {
                solarity_rendering::M2TransparentPass::One
            } else {
                solarity_rendering::M2TransparentPass::Two
            },
            fog.color(),
            local_animation_time_ms,
            solarity_rendering::M2CameraEffectScale::EXTERNAL_CAMERA,
            random,
            Some(game_objects),
            unit_effect_callback,
            Some((
                m2_scene,
                solarity_rendering::M2DirectionalLight::new(
                    -environment.light_direction(),
                    ambient,
                    diffuse,
                ),
            )),
            Some((
                terrain,
                entity_light_environment,
                environment.ordinary_model_fog().color(),
                liquid_types,
            )),
            shadow_projection,
            shadow_admission
                .as_ref()
                .map(|admission| shadow::SceneryShadowQueries {
                    admission,
                    doodads: self.world_models.shadow_doodads(),
                }),
        )?;
        profile.mark("M2 admission");
        // Scene visibility and model clocks are fixed. Ordered M2 traversal may
        // be paused before an unfinished root pose, with earlier geometry running.
        // These disjoint owners need neither emitted M2 packets
        // nor receiver lights. Keep WMO fog-bank publication after the receiver
        // callbacks below; preparing its immutable packets does not publish it.
        let mut independent = terrain
            .world_terrain_frustum(camera)
            .map_err(RuntimeTerrainFrameError::from)
            .map(|frustum| (frustum, Ok(())));
        while !main.initial_done() {
            let (step, outcome) = main
                .take_ready()
                .ok_or(solarity_cpu::CpuError::CompletionLost)?;
            // Revisit ready root palettes between main-only operations. This can
            // release geometry while the following WMO operation is still pending.
            pending_m2.try_admit(
                cpu,
                random,
                Some(game_objects),
                Some((
                    terrain,
                    entity_light_environment,
                    environment.ordinary_model_fog().color(),
                    liquid_types,
                )),
                shadow_admission
                    .as_ref()
                    .map(|admission| shadow::SceneryShadowQueries {
                        admission,
                        doodads: self.world_models.shadow_doodads(),
                    }),
            )?;
            let _profile = solarity_profiling::profile!("world.independent_preparation");
            let _trace =
                solarity_profiling::TraceSpan::new("world.main_continuation", step as u64, 0);
            if let Ok((exterior_frustum, wmo)) = &mut independent {
                debug_assert_eq!(outcome, solarity_cpu::JobOutcome::Succeeded);
                match step {
                    super::MainPreparationStep::GroundDetail => {
                        if let Err(error) = self.ground_detail.prepare(
                            renderer,
                            terrain.resident_tiles(),
                            camera,
                            *exterior_frustum,
                        ) {
                            independent = Err(error);
                        }
                    }
                    super::MainPreparationStep::WorldModels => {
                        *wmo = self
                            .world_models
                            .prepare_visible_draws(
                                renderer,
                                terrain.world_model_scene_groups(),
                                environment.world_model_emissive(),
                                environment.ordinary_model_fog().color(),
                                fog.color(),
                            )
                            .map(|_| ());
                    }
                    super::MainPreparationStep::Surfaces | super::MainPreparationStep::Uniforms => {
                        unreachable!("light continuations follow initial world preparation")
                    }
                }
            }
            if step == super::MainPreparationStep::GroundDetail {
                main.ground_finished(independent.is_ok())?;
            }
        }
        profile.mark("independent ground detail and WMO packets");
        // Receiver callbacks stay after both independent main operations. Once
        // sources are published, terrain/liquid consumers need not await M2
        // receiver uniforms. Keep their failure pending until M2 completes.
        let mut surface_result = None;
        loop {
            let ready = pending_m2.try_advance(
                cpu,
                random,
                Some(game_objects),
                Some((
                    terrain,
                    entity_light_environment,
                    environment.ordinary_model_fog().color(),
                    liquid_types,
                )),
                shadow_admission
                    .as_ref()
                    .map(|admission| shadow::SceneryShadowQueries {
                        admission,
                        doodads: self.world_models.shadow_doodads(),
                    }),
            )?;
            if !main.sources_published() && pending_m2.scene_lights().is_some() {
                main.publish_sources(pending_m2.receiver_completion()?)?;
            }
            while let Some((step, outcome)) = main.take_ready() {
                let _trace =
                    solarity_profiling::TraceSpan::new("world.main_continuation", step as u64, 0);
                match step {
                    super::MainPreparationStep::Surfaces => {
                        if outcome != solarity_cpu::JobOutcome::Succeeded {
                            continue;
                        }
                        let (exterior_frustum, _) = independent.as_ref().unwrap_or_else(|_| {
                            unreachable!("surface readiness requires successful ground preparation")
                        });
                        let lights = pending_m2.scene_lights().unwrap_or_else(|| {
                            unreachable!("surface readiness follows light publication")
                        });
                        let (lighting, fog) =
                            liquid_environment(environment, camera, glare_lighting);
                        surface_result = Some(
                            super::surfaces::SurfacePreparation {
                                tiles: &self.tiles,
                                terrain_draws: &mut self.visible_draws,
                                liquid_draws: &mut self.liquid_draws,
                                world_models: &self.world_models,
                            }
                            .prepare(
                                renderer,
                                terrain,
                                super::surfaces::SurfaceInputs {
                                    exterior_frustum: *exterior_frustum,
                                    camera,
                                    lighting,
                                    fog,
                                    ordinary_model_fog: environment.ordinary_model_fog().color(),
                                    liquid_time_ms,
                                    specular_enabled,
                                },
                                lights,
                            ),
                        );
                    }
                    super::MainPreparationStep::Uniforms => {}
                    super::MainPreparationStep::GroundDetail
                    | super::MainPreparationStep::WorldModels => {
                        unreachable!("initial world continuations were consumed before receivers")
                    }
                }
            }
            if ready {
                break;
            }
            if main.needs_uniform_notice() {
                main.wait(wait)?;
            } else {
                pending_m2.wait(wait)?;
            }
        }
        let m2 = pending_m2.into_visible_frame()?;
        let (_, wmo_result) = independent?;
        surface_result
            .unwrap_or_else(|| unreachable!("ready M2 published surface light inputs"))?;
        profile.mark("M2 and lit surface publication");
        // Preserve the original failure order even though WMO work executed earlier.
        wmo_result?;
        let world_model::WorldModelVisibleFrame {
            draws: world_model_draws,
            last_group: last_world_model_group,
            shadow_draws: world_model_shadow_draws,
        } = self.world_models.visible_frame();
        terrain.complete_world_model_scene(last_world_model_group);
        profile.mark("WMO publication");

        let sky_window = terrain.world_model_sky_window()?;
        let has_sky_window = terrain.has_world_model_sky_window();
        let world_model_skybox = has_sky_window
            .then(|| terrain.world_model_skybox())
            .flatten();
        let (default_sky, sky_models) = sky_resources.prepare_models(
            cpu,
            renderer,
            camera,
            liquid_time_ms,
            environment,
            sky_window,
            world_model_skybox,
            m2.bone_transforms.len(),
            random,
        )?;
        let depths = (!self.liquid_draws.is_empty()).then(|| liquid_depth_images(light));
        let mut scene = WorldFrameScene::new(terrain_scene, world_model_scene, m2_scene)
            .with_ground_detail(self.ground_detail.frame(camera.camera().position())?)
            .with_world_depth_range()
            .with_m2_instance_scenes(m2.instance_scenes)
            .with_sky_models(sky_models)
            .with_sky_window(
                sky_window
                    .filter(|_| !environment.has_camera_liquid() && environment.sky_enabled()),
            )
            .with_background_color(if !has_sky_window {
                environment.fog().color().extend(1.)
            } else if environment.has_camera_liquid() || !environment.sky_enabled() {
                environment.ordinary_model_fog().color().extend(1.)
            } else {
                Vec4::ZERO
            })
            .with_particle_capacity(m2.particle_vertex_capacity, m2.particle_index_capacity);
        if let Some(projection) = shadow_projection {
            scene = scene.with_primary_shadows(solarity_rendering::WorldPrimaryShadowFrame::new(
                projection,
                m2.shadow_draws,
            ));
        }
        if let Some(environment) = environment_frame {
            scene = scene.with_environment_shadows(
                environment.with_casters(m2.environment_shadow_draws, world_model_shadow_draws),
            );
        }
        // 7D5E70 consumes DayNight+8C; camera-interior color remains at +A0.
        if let Some(horizon) = terrain.world_low_detail_frame(
            camera,
            environment.horizon_fog_color(),
            self.horizon_scale,
        )? {
            scene = scene.with_low_detail(horizon);
        }
        if environment.sky_enabled() {
            scene = scene.with_glare(self.sky.glare_frame(
                camera,
                environment,
                &celestial_resources,
                sky_models.glare_suppression(),
            ));
        }
        if default_sky {
            scene = scene
                .with_celestials(
                    self.sky
                        .celestial_frame(camera, celestial_resources.textures),
                )
                .with_sky(self.sky.gradient_frame(camera))
                .with_clouds(self.sky.cloud_frame(camera));
        }
        if let Some([river, ocean, world_model]) = &depths {
            scene = scene.with_liquids(
                solarity_rendering::LiquidFrame::new(
                    &self.liquid_draws,
                    river,
                    ocean,
                    world_model,
                    m2.water_scene_order,
                )
                .with_texture_filtering(self.liquid_filtering),
            );
        }
        if let Some(ripples) = ripples {
            scene = scene.with_ripples(ripples.with_water_scene_order(m2.water_scene_order));
        }
        if let Some(underwater_particles) = underwater_particles {
            scene = scene.with_underwater_particles(underwater_particles);
        }
        scene = scene.with_screen_effect(screen_effect);
        let _submission_trace = solarity_profiling::TraceSpan::new("world.render.consume", 0, 0);
        m2.trace.link("m2.frame.consume");
        wait.before_gpu_frame(renderer, solarity_rendering::GpuFrameKind::World)?;
        let report = renderer.present_world_frame_with_ui_layers(
            &mut wait.recording(cpu),
            scene,
            m2.bone_transforms,
            &self.visible_draws,
            world_model_draws,
            m2.draws,
            m2.particle_vertices,
            m2.particle_indices,
            m2.particle_draws,
            m2.ribbon_vertices,
            m2.ribbon_draws,
            WorldScreenWindow::FULL,
            ui_extent,
            ui_draws,
            ui_overlay_draws,
        )?;
        self.environment_shadows = pending_environment;
        profile.mark("Vulkan presentation");
        Ok(report)
    }
}
