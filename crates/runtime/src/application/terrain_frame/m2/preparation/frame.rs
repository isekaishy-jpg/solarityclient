//! Selected-placement preparation; source admission and topology publication live separately.

use super::super::{
    CrtRand, GameObjectFrameInput, M2AnimationClock, M2BonePoseOverrides, M2CameraEffectScale,
    M2EffectOrder, M2ElementAlphaState, M2Frame, M2GpuPlacementOwner, M2MaterialPose,
    M2MaterialState, M2MaterialUniform, M2ParticleMeshPlan, M2ParticleMeshPlanError,
    M2ParticlePose, M2PlaybackStorage, M2RibbonMeshPlan, M2RibbonPose, M2TransparentDrawIndex,
    M2TransparentElement, M2TransparentPass, M2TransparentSortKey, M2VisibleFrame, Mat4, Rc,
    RuntimeFrameProfile, RuntimeTerrainFrameError, VulkanRenderer, WorldCameraFrame, WorldFrustum,
    advance_ribbons, append_triggered_events, compare_m2_transparent, held_item_finger_pose,
    m2_model_distance_key, particle_emission_density, particle_lod_origin,
    placement_bounding_sphere, placement_color, placement_light_bank, placement_mesh_color,
    sample_m2_lights_into, sample_mount_camera, scene_element_count, section_distance_key, shadow,
    unit_effects,
};

impl M2Frame {
    /// Unit callbacks construct CEffect models before this frame's effect pass.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::application::terrain_frame) fn prepare_visible_draws_with_unit_effects(
        &mut self,
        renderer: &VulkanRenderer,
        cpu: Option<&solarity_cpu::CpuExecutor>,
        frustum: WorldFrustum,
        camera: WorldCameraFrame,
        first_transparent_pass: M2TransparentPass,
        fog_color: glam::Vec3,
        animation_time_ms: f32,
        effect_scale: M2CameraEffectScale,
        random: &mut CrtRand,
        game_objects: Option<GameObjectFrameInput<'_>>,
        unit_effect_callback: Option<&mut unit_effects::UnitEffectEventCallback<'_>>,
        world_lighting: Option<(
            solarity_rendering::M2SceneUniform,
            solarity_rendering::M2DirectionalLight,
        )>,
        mut spatial_lighting: Option<(
            &mut crate::application::terrain_coordinator::RuntimeTerrainCoordinator,
            solarity_systems::WorldEntityLightEnvironment,
            glam::Vec3,
            &solarity_asset::LiquidTypeCatalog,
        )>,
        shadow_projection: Option<solarity_rendering::WorldShadowProjection>,
        scenery_shadows: Option<
            crate::application::terrain_frame::shadow::SceneryShadowQueries<'_>,
        >,
    ) -> Result<M2VisibleFrame<'_>, RuntimeTerrainFrameError> {
        let mut frame_profile = RuntimeFrameProfile::new("M2 frame preparation");
        let frame_seconds = ((animation_time_ms - self.unit_scene_time_ms) * 0.001).max(0.0);
        self.unit_scene_time_ms = animation_time_ms;
        if let Some((terrain, ..)) = spatial_lighting.as_mut() {
            terrain.prepare_world_scene(camera)?;
        }
        if let Some(game_objects) = game_objects {
            game_objects.advance_scene(animation_time_ms, random)?;
        }
        self.advance_retired_models(animation_time_ms as u32, game_objects);
        self.bone_transforms.clear();
        self.visible_draws.clear();
        self.shadow_draws.clear();
        self.shadow_admission.clear();
        self.environment_shadow_draws.clear();
        self.environment_shadow_admission.clear();
        self.transparent_elements.clear();
        self.particle_vertices.clear();
        self.particle_indices.clear();
        self.particle_draws.clear();
        self.ribbon_vertices.clear();
        self.ribbon_draws.clear();
        self.triggered_events.clear();
        self.mount_camera_sample = None;
        self.glue_directional_lights.clear();
        self.glue_point_lights.clear();
        self.scene_lighting.clear();
        let mut particle_vertex_capacity = 0_usize;
        let mut particle_index_capacity = 0_usize;
        frame_profile.mark("scene setup");
        self.unit_effects
            .publish_loaded(&self.animations, animation_time_ms, random)?;
        self.unit_effects.begin_frame();
        // Current topology excludes every ordinary model before the first
        // effect. Publication can invalidate that boundary before this pass.
        let first_effect = if self.placement_topology_dirty {
            0
        } else {
            self.placement_visibility.effect_start()
        };
        if self
            .unit_effects
            .retire_drained(&mut self.placements, first_effect)
        {
            if !self.placement_topology_dirty {
                self.placement_visibility
                    .replace_effect_tail(&self.placements, &self.sources);
            }
            self.compact_sources();
        }
        self.publish_placement_topology();
        frame_profile.mark("residency and topology");
        self.vehicle_passengers.prepare_timing(
            &mut self.placements,
            &self.sources,
            &self.placement_visibility,
            &self.requested_items,
            camera.view(),
            animation_time_ms,
            random,
        )?;
        // Prepare unit selection and yaw before placement. Authored events and
        // completion follow below in scene traversal order, before camera culling.
        for &index in self.placement_visibility.dynamic_indices() {
            let placement = &mut self.placements[index];
            if let Some(animation) = &placement.unit_animation {
                if let Some(registration) = placement.scene_registration
                    && let Some((terrain, ..)) = spatial_lighting.as_mut()
                    && terrain.unit_scene_admits(registration.position, registration.bounds)?
                {
                    animation.admit_scene_collision();
                }
                animation.prepare_scene(animation_time_ms, random)?;
                placement.transform =
                    placement.local_transform * animation.body_pose().placement_rotation;
            }
        }
        // Ground placement follows unit animation/yaw for every model, including
        // mounts inserted before their riders. It does not advance a second
        // unit callback or substitute the rider's model/timer for the mount.
        for &index in self.placement_visibility.dynamic_indices() {
            let placement = &mut self.placements[index];
            let Some(ground) = &placement.ground_placement else {
                continue;
            };
            if ground.owner.passenger_input().is_some() {
                continue;
            }
            if !ground.owner.uses_ground_placement() {
                continue;
            }
            if placement.unit_animation.is_some() {
                placement.transform = ground.owner.ground_transform(
                    ground.position,
                    ground.scale,
                    animation_time_ms,
                    frame_seconds,
                )?;
            } else if let Some(source) = &self.sources[placement.source_index]
                && let Some(playback) = &placement.playback
            {
                placement.transform = ground.owner.ground_model_transform(
                    ground.position,
                    ground.scale,
                    animation_time_ms,
                    frame_seconds,
                    &source.model,
                    &playback.borrow(),
                )?;
            }
        }
        self.vehicle_passengers.prepare(
            &mut self.placements,
            &self.sources,
            &self.placement_visibility,
            &self.requested_items,
            camera.view(),
            animation_time_ms,
            random,
        )?;
        self.placement_visibility
            .set_vehicle_parents(self.vehicle_passengers.parents());
        self.advance_unit_callbacks(camera, animation_time_ms, random, unit_effect_callback)?;
        self.rider_transforms.clear();
        self.rider_transforms.reserve(
            self.mounted_guids
                .len()
                .saturating_sub(self.rider_transforms.capacity()),
        );
        self.item_transforms.clear();
        self.item_transforms.reserve(
            self.requested_items
                .len()
                .saturating_sub(self.item_transforms.capacity()),
        );
        self.visual_transforms.clear();
        self.visual_transforms.reserve(
            self.requested_visuals
                .len()
                .saturating_sub(self.visual_transforms.capacity()),
        );
        self.glue_attachment_transforms.clear();
        self.glue_attachment_transforms.reserve(
            self.glue_attachment_ids
                .len()
                .saturating_sub(self.glue_attachment_transforms.capacity()),
        );
        frame_profile.mark("dynamic models");
        self.prepare_unit_poses(cpu, camera.view(), animation_time_ms as u32)?;
        frame_profile.mark("unit pose batch");
        if let Some((terrain, ..)) = spatial_lighting.as_ref() {
            self.doodad_scene.prepare(
                terrain,
                &self.placement_visibility,
                &mut self.placements,
                &self.sources,
                camera.camera().position(),
                self.environment_detail,
            )?;
        }
        frame_profile.mark("WMO doodad admission");
        self.placement_visibility.select_frame_work(
            camera.camera().position(),
            self.environment_detail,
            scenery_shadows.is_some(),
            &mut self.frame_work,
        );
        self.shadow_admission.resize(self.placements.len(), false);
        self.environment_shadow_admission
            .resize(self.placements.len(), 0);
        frame_profile.mark("frame work selection");
        let effect_start = self.placement_visibility.effect_start();
        let mut effects_published = false;
        loop {
            if !effects_published
                && self
                    .frame_work
                    .next_index()
                    .is_none_or(|index| index >= effect_start)
            {
                effects_published = true;
                if self
                    .unit_effects
                    .publish(&mut self.placements, &mut self.sources, effect_start)
                {
                    self.placement_visibility
                        .replace_effect_tail(&self.placements, &self.sources);
                    self.placement_visibility
                        .set_vehicle_parents(self.vehicle_passengers.parents());
                    self.shadow_admission.resize(self.placements.len(), false);
                    self.environment_shadow_admission
                        .resize(self.placements.len(), 0);
                }
                self.frame_work
                    .publish_effect_tail(effect_start, self.placements.len());
            }
            let Some(placement_index) = self.frame_work.next() else {
                break;
            };
            // Moving-parent transforms have already been resolved. Forward
            // attachments query that same root; earlier roots reuse admission.
            let environment_maps = if let Some(queries) = scenery_shadows {
                let root = self
                    .placement_visibility
                    .light_root(placement_index)
                    .unwrap_or(placement_index);
                if root < placement_index {
                    self.environment_shadow_admission[root]
                } else if self
                    .placement_visibility
                    .scenery(root)
                    .is_some_and(|scenery| {
                        !scenery.admits_shadow(camera.camera().position(), self.environment_detail)
                    })
                {
                    // Immutable scenery uses this same cutoff in environment_maps.
                    // Reject it from compact metadata before loading the large
                    // placement and source records for distant resident tiles.
                    0
                } else if !self.vehicle_passengers.hidden(root)
                    && let Some(source) = &self.sources[self.placements[root].source_index]
                {
                    shadow::environment_maps(
                        queries,
                        source,
                        &self.placements[root],
                        camera.camera().position(),
                        self.environment_detail,
                    )?
                } else {
                    0
                }
            } else {
                0
            };
            let publishes_lights =
                world_lighting.is_some() && self.placement_visibility.has_lights(placement_index);
            let bounds = self.placement_visibility.bounds()[placement_index];
            let doodad_scene_active = spatial_lighting.is_some()
                && self
                    .placement_visibility
                    .is_world_model_doodad(placement_index);
            let doodad_fog = self.doodad_scene.fog_bank(placement_index);
            let doodad_visible = !doodad_scene_active || doodad_fog.is_some();
            if !doodad_visible && !publishes_lights && environment_maps == 0 {
                continue;
            }
            let mut placement_fog_color = if doodad_scene_active && doodad_fog == Some(false) {
                spatial_lighting
                    .as_ref()
                    .map_or(fog_color, |(_, _, ordinary, _)| *ordinary)
            } else {
                fog_color
            };
            let scenery_opacity = if doodad_scene_active {
                self.doodad_scene.opacity(placement_index)
            } else {
                self.placement_visibility.opacity(
                    placement_index,
                    camera.camera().position(),
                    self.environment_detail,
                )
            };
            if scenery_opacity == 0.0 && !publishes_lights && environment_maps == 0 {
                continue;
            }
            if let Some((center, radius)) = bounds
                && !publishes_lights
                && !doodad_scene_active
                && environment_maps == 0
                && !frustum.contains_sphere(center, radius)?
            {
                continue;
            }
            // Forward vehicle parents already have their final model matrices
            // from the ancestry pass. Admission needs no second animation tick.
            let forward_shadow = if let Some(projection) = shadow_projection
                && self
                    .placement_visibility
                    .light_parent(placement_index)
                    .is_some_and(|parent| parent >= placement_index)
            {
                if let Some(root) = self.placement_visibility.light_root(placement_index) {
                    let placement = &self.placements[root];
                    if root < placement_index {
                        self.shadow_admission[root]
                    } else if placement.placement_valid
                        && !placement
                            .entity_opacity
                            .as_ref()
                            .is_some_and(|owner| owner.hidden())
                        && let Some(source) = &self.sources[placement.source_index]
                    {
                        shadow::admits_root(
                            projection,
                            source,
                            placement,
                            scenery_shadows.map(|queries| queries.admission),
                        )?
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            };
            // 82F0F0 retains the containing model's +88 distance for ordinary
            // attachments; mesh, particle and ribbon queues consume that lane.
            let inherited_model_distance = self
                .placement_visibility
                .light_parent(placement_index)
                .and_then(|_| self.placement_visibility.light_root(placement_index))
                .map(|root| m2_model_distance_key(camera.view() * self.placements[root].transform));
            let (root_liquid, owner_fog) =
                if let Some((terrain, _, _, liquid_types)) = spatial_lighting.as_mut() {
                    let root = self
                        .placement_visibility
                        .light_root(placement_index)
                        .unwrap_or(placement_index);
                    let root = &mut self.placements[root];
                    if root.placement_valid
                        && !root
                            .entity_opacity
                            .as_ref()
                            .is_some_and(|owner| owner.hidden())
                        && let Some(source) = &self.sources[root.source_index]
                    {
                        root.entity_lighting.scene_state(
                            root.retirement
                                .as_ref()
                                .map_or(root.owner, |retired| retired.original_owner),
                            &source.model,
                            root.local_transform,
                            root.scene_registration,
                            terrain,
                            liquid_types,
                            camera.view(),
                        )?
                    } else {
                        (solarity_rendering::M2LiquidState::Above, None)
                    }
                } else {
                    (solarity_rendering::M2LiquidState::Above, None)
                };
            if owner_fog == Some(false) {
                placement_fog_color = spatial_lighting
                    .as_ref()
                    .map_or(fog_color, |(_, _, ordinary, _)| *ordinary);
            }
            let placement = &mut self.placements[placement_index];
            if self.vehicle_passengers.hidden(placement_index) {
                if let M2GpuPlacementOwner::PlayerMount { guid }
                | M2GpuPlacementOwner::RemotePlayerMount { guid }
                | M2GpuPlacementOwner::CreatureMount { guid }
                | M2GpuPlacementOwner::PlayerBody { guid }
                | M2GpuPlacementOwner::RemotePlayerBody { guid }
                | M2GpuPlacementOwner::CreatureBody { guid } = placement.owner
                {
                    self.rider_transforms.push((guid, None));
                }
                continue;
            }
            let shadow_opacity = placement.opacity
                * placement
                    .retirement
                    .as_ref()
                    .map_or(1.0, |owner| owner.opacity())
                * placement
                    .entity_opacity
                    .as_ref()
                    .map_or(1.0, |owner| owner.opacity());
            let placement_opacity = shadow_opacity * scenery_opacity;
            self.unit_effects.prepare_attachment(placement);
            if !self.retirement.prepare_attachment(placement) {
                continue;
            }
            if let Some(effect) = &mut placement.unit_effect
                && let Some(event) = effect.take_ready_sound(placement.transform.w_axis.truncate())
            {
                self.triggered_events.push(event);
            }
            if !placement.placement_valid {
                continue;
            }
            if let Some(attachment_id) = placement.glue_parent_attachment {
                let parent = self
                    .glue_attachment_transforms
                    .iter()
                    .find_map(|(id, transform)| (*id == attachment_id).then_some(*transform))
                    .ok_or(RuntimeTerrainFrameError::MissingGlueM2AttachmentPose {
                        attachment_id,
                    })?;
                let Some(parent) = parent else {
                    continue;
                };
                placement.transform = parent * placement.local_transform;
            }
            if let M2GpuPlacementOwner::PlayerBody { guid }
            | M2GpuPlacementOwner::RemotePlayerBody { guid }
            | M2GpuPlacementOwner::CreatureBody { guid } = placement.owner
                && self.mounted_guids.contains(&guid)
            {
                let transform = self
                    .rider_transforms
                    .iter()
                    .find_map(|(owner_guid, transform)| (*owner_guid == guid).then_some(*transform))
                    .ok_or(RuntimeTerrainFrameError::MissingMountM2AttachmentPose {
                        guid,
                        attachment_id: 0,
                    })?;
                let Some(transform) = transform else {
                    continue;
                };
                placement.transform =
                    transform * Mat4::from_scale(glam::Vec3::splat(placement.rider_scale));
            }
            if let M2GpuPlacementOwner::UnitItem { guid, point } = placement.owner {
                if self
                    .rider_transforms
                    .iter()
                    .any(|(owner_guid, transform)| *owner_guid == guid && transform.is_none())
                {
                    continue;
                }
                let transform = self
                    .item_transforms
                    .iter()
                    .find_map(|(owner_guid, owner_point, transform)| {
                        (*owner_guid == guid && *owner_point == point).then_some(*transform)
                    })
                    .ok_or(RuntimeTerrainFrameError::MissingPlayerM2AttachmentPose {
                        guid,
                        attachment_id: point.id(),
                    })?;
                let Some(transform) = transform else {
                    continue;
                };
                placement.transform = transform * placement.orientation.local_transform();
            }
            if let M2GpuPlacementOwner::UnitItemVisual {
                guid,
                item_point,
                effect_point,
            } = placement.owner
            {
                if self
                    .rider_transforms
                    .iter()
                    .any(|(owner_guid, transform)| *owner_guid == guid && transform.is_none())
                {
                    continue;
                }
                let transform = self
                    .visual_transforms
                    .iter()
                    .find_map(
                        |(owner_guid, owner_item_point, owner_effect_point, transform)| {
                            (*owner_guid == guid
                                && *owner_item_point == item_point
                                && *owner_effect_point == effect_point)
                                .then_some(*transform)
                        },
                    )
                    .ok_or(RuntimeTerrainFrameError::MissingPlayerM2AttachmentPose {
                        guid,
                        attachment_id: effect_point,
                    })?;
                let Some(transform) = transform else {
                    continue;
                };
                placement.transform = transform;
            }
            let Some(source) = self.sources[placement.source_index].as_ref() else {
                continue;
            };
            let owner = placement.owner;
            // ADT/WMO placements were culled from compact immutable bounds
            // before touching instance state. Replicated WMO doodads can move
            // with their parent and require their current transform here.
            // CEffects can leave live particles outside their authored model
            // bounds. Keep their update and particle packets in the scene;
            // 821BEE advances registered roots and 828A00 their children.
            let static_visibility_resolved = matches!(
                owner,
                M2GpuPlacementOwner::UnitEffect { .. }
                    | M2GpuPlacementOwner::Static(_)
                    | M2GpuPlacementOwner::GameObjectWorldModelDoodad { .. }
            );
            if matches!(
                owner,
                M2GpuPlacementOwner::GameObjectWorldModelDoodad { .. }
            ) && !publishes_lights
                && !doodad_scene_active
                && environment_maps == 0
            {
                let (center, radius) =
                    placement_bounding_sphere(&source.model, placement.transform);
                if !frustum.contains_sphere(center, radius)? {
                    continue;
                }
            }
            let Some(mut playback) = placement
                .playback
                .as_mut()
                .map(M2PlaybackStorage::borrow_mut)
            else {
                continue;
            };
            let scene_sample = if let M2GpuPlacementOwner::GameObject { identity, .. } = owner
                && let Some(game_objects) = game_objects
                && let Some(instance) = game_objects.get(identity)
            {
                instance
                    .behavior()
                    .and_then(|behavior| behavior.take_scene_sample())
                    .or_else(|| {
                        instance
                            .transport_model()
                            .and_then(|model| model.take_scene_sample())
                    })
                    .map(|sample| (sample.advance, sample.event_window))
            } else {
                placement
                    .unit_animation
                    .as_ref()
                    .and_then(|animation| animation.take_scene_sample())
                    .map(|sample| (sample.advance, sample.event_window))
            };
            let (advance, prepared_event_window) = if let Some((advance, event_window)) =
                scene_sample
            {
                (advance, Some(event_window))
            } else {
                let advance = if let Some(advance) = placement.passenger_playback_advance.take() {
                    advance
                } else if let Some(effect) = &mut placement.unit_effect {
                    effect.advance(&mut playback, &source.model, animation_time_ms, random)?
                } else if matches!(owner, M2GpuPlacementOwner::GlueModel { .. }) {
                    self.pending_glue_playback_advance.take().map_or_else(
                        || playback.clock(&source.model, animation_time_ms, random),
                        Ok,
                    )?
                } else {
                    playback.clock(&source.model, animation_time_ms, random)?
                };
                (advance, None)
            };
            let body_pose = placement
                .unit_animation
                .as_ref()
                .map(|animation| animation.body_pose())
                .or_else(|| {
                    placement
                        .retirement
                        .as_ref()?
                        .unit_pose
                        .map(|pose| pose.body)
                });
            let bone_transforms = body_pose
                .as_ref()
                .map_or(&[][..], |pose| pose.bone_transforms());
            for expired in advance.expired_variations {
                self.bone_pose_scratch.recompose_with_overrides(
                    source.model.animations(),
                    expired.clock,
                    camera.view() * placement.transform,
                    M2BonePoseOverrides {
                        model_oriented_billboard_bones: &source.model_oriented_billboard_bones,
                        bone_transforms,
                        bone_sequences: &expired.bone_sequences,
                        ..Default::default()
                    },
                )?;
                let first_event = self.triggered_events.len();
                append_triggered_events(
                    &mut self.triggered_events,
                    &source.model,
                    placement.owner,
                    placement.transform,
                    &placement.sound_lifetime,
                    &self.bone_pose_scratch,
                    expired.event_window,
                )?;
                if let Some(effect) = &placement.unit_effect {
                    effect.bind_sound_events(&mut self.triggered_events[first_event..]);
                }
            }
            let clock = advance.clock;
            let bone_sequences =
                playback.bone_sequence_clocks(&source.model, clock, animation_time_ms as u32);
            let finger_pose_hands = placement
                .retirement
                .as_ref()
                .and_then(|retired| retired.finger_hands)
                .or_else(|| held_item_finger_pose(&self.requested_items, owner));
            let finger_pose = finger_pose_hands.and_then(|hands| {
                source
                    .model
                    .animations()
                    .sequence_for_variation(15, 0)
                    .map(|sequence| {
                        (
                            M2AnimationClock::new_with_global_tick(
                                sequence,
                                0.0,
                                playback.global_tick(animation_time_ms as u32),
                            ),
                            hands,
                        )
                    })
            });
            let event_window =
                prepared_event_window.unwrap_or_else(|| playback.event_window(animation_time_ms));
            drop(playback);
            let model_view = camera.view() * placement.transform;
            let model_bounds = source.model.bounds();
            let particle_liquid = root_liquid.classify_model(
                (model_bounds.minimum() + model_bounds.maximum()) * 0.5,
                model_bounds.sphere_radius(),
                model_view,
                true,
                false,
            );
            let model_liquid = particle_liquid.with_clipping_support(
                renderer.m2_liquid_clipping_enabled(),
                first_transparent_pass == M2TransparentPass::One,
            );
            let instance_identity = std::ptr::from_ref(&*placement).addr();
            let instance_distance =
                inherited_model_distance.unwrap_or_else(|| m2_model_distance_key(model_view));
            let overrides = M2BonePoseOverrides {
                model_oriented_billboard_bones: &source.model_oriented_billboard_bones,
                finger_pose,
                bone_transforms,
                bone_sequences: &bone_sequences,
            };
            if !self.pose_batch.take(
                placement_index,
                &source.model,
                clock,
                model_view,
                overrides,
                &mut self.bone_pose_scratch,
            )? {
                self.bone_pose_scratch.recompose_with_overrides(
                    source.model.animations(),
                    clock,
                    model_view,
                    overrides,
                )?;
            }
            let bone_pose = &self.bone_pose_scratch;
            self.retirement
                .publish_attachments(placement, &source.model, bone_pose, clock)?;
            let first_event = self.triggered_events.len();
            append_triggered_events(
                &mut self.triggered_events,
                &source.model,
                placement.owner,
                placement.transform,
                &placement.sound_lifetime,
                bone_pose,
                event_window,
            )?;
            if let Some(effect) = &placement.unit_effect {
                effect.bind_sound_events(&mut self.triggered_events[first_event..]);
            }
            if let Some(animation) = &placement.unit_animation {
                self.unit_effects.update_anchor(
                    animation,
                    &source.model,
                    bone_pose,
                    placement.transform,
                )?;
            }
            if publishes_lights {
                solarity_rendering::sample_m2_scene_lights_into(
                    source.model.animations(),
                    bone_pose,
                    clock,
                    placement.transform,
                    &mut self.scene_lighting.sample_directional,
                    &mut self.scene_lighting.sample_points,
                )?;
                self.scene_lighting.publish(
                    placement.light_lifetime.get_or_init(|| Rc::new(())),
                    placement_mesh_color(placement.owner, placement.color).w * placement_opacity,
                )?;
            }
            if matches!(placement.owner, M2GpuPlacementOwner::GlueModel { .. }) {
                sample_m2_lights_into(
                    source.model.animations(),
                    bone_pose,
                    clock,
                    placement.transform,
                    &mut self.glue_directional_lights,
                    &mut self.glue_point_lights,
                )?;
                for attachment_id in &self.glue_attachment_ids {
                    let attachment = source.model.attachment(*attachment_id).ok_or_else(|| {
                        RuntimeTerrainFrameError::MissingGlueM2Attachment {
                            model: source.model.path().clone(),
                            attachment_id: *attachment_id,
                        }
                    })?;
                    let transform = bone_pose.attachment_transform(
                        source.model.animations(),
                        attachment,
                        clock,
                        placement.transform,
                    )?;
                    self.glue_attachment_transforms
                        .push((*attachment_id, transform));
                }
            }
            if let M2GpuPlacementOwner::PlayerMount { guid }
            | M2GpuPlacementOwner::RemotePlayerMount { guid }
            | M2GpuPlacementOwner::CreatureMount { guid } = placement.owner
            {
                if matches!(placement.owner, M2GpuPlacementOwner::PlayerMount { .. }) {
                    self.mount_camera_sample = Some(sample_mount_camera(
                        &source.model,
                        placement.transform,
                        bone_pose,
                        animation_time_ms,
                    )?);
                }
                let attachment = source.model.attachment(0).ok_or_else(|| {
                    RuntimeTerrainFrameError::MissingMountM2Attachment {
                        model: source.model.path().clone(),
                        attachment_id: 0,
                    }
                })?;
                let transform = bone_pose.attachment_transform(
                    source.model.animations(),
                    attachment,
                    clock,
                    placement.transform,
                )?;
                self.rider_transforms.push((guid, transform));
            }
            if let M2GpuPlacementOwner::PlayerBody { guid }
            | M2GpuPlacementOwner::RemotePlayerBody { guid }
            | M2GpuPlacementOwner::CreatureBody { guid } = placement.owner
            {
                for (_owner_guid, point) in self
                    .requested_items
                    .iter()
                    .filter(|(owner_guid, _point)| *owner_guid == guid)
                {
                    let attachment = source.model.attachment(point.id()).ok_or_else(|| {
                        RuntimeTerrainFrameError::MissingPlayerM2Attachment {
                            model: source.model.path().clone(),
                            attachment_id: point.id(),
                        }
                    })?;
                    let transform = bone_pose.attachment_transform(
                        source.model.animations(),
                        attachment,
                        clock,
                        placement.transform,
                    )?;
                    self.item_transforms.push((guid, *point, transform));
                }
            }
            if let M2GpuPlacementOwner::UnitItem { guid, point } = placement.owner {
                for (_owner_guid, _owner_item_point, effect_point) in self
                    .requested_visuals
                    .iter()
                    .filter(|(owner_guid, item_point, _)| {
                        *owner_guid == guid && *item_point == point
                    })
                {
                    let attachment = source.model.attachment(*effect_point).ok_or_else(|| {
                        RuntimeTerrainFrameError::MissingPlayerM2Attachment {
                            model: source.model.path().clone(),
                            attachment_id: *effect_point,
                        }
                    })?;
                    let transform = bone_pose.attachment_transform(
                        source.model.animations(),
                        attachment,
                        clock,
                        placement.transform,
                    )?;
                    self.visual_transforms
                        .push((guid, point, *effect_point, transform));
                }
            }
            // 4F8D10 updates unit state/placement before clearing model activity.
            // Preserve attachment samples, but hidden player hierarchies publish
            // no model lights, shadow packets, visible effects or mesh packets.
            if placement
                .entity_opacity
                .as_ref()
                .is_some_and(|owner| owner.hidden())
            {
                continue;
            }
            let scene_index = if world_lighting.is_some() {
                // 831AF0 queries matrix F4's translation at +124 with radius
                // zero. Authored mesh bounds do not move the lighting center.
                let center = placement.transform.w_axis.truncate();
                let parent = self.placement_visibility.light_parent(placement_index);
                let callback = if parent.is_none()
                    && let Some((terrain, environment, _, _)) = spatial_lighting.as_mut()
                {
                    placement.entity_lighting.sample(
                        placement
                            .retirement
                            .as_ref()
                            .map_or(placement.owner, |retired| retired.original_owner),
                        &source.model,
                        placement.transform,
                        placement.color,
                        animation_time_ms,
                        terrain,
                        *environment,
                    )?
                } else {
                    None
                };
                Some(self.scene_lighting.receiver_with_light(
                    placement_index,
                    parent,
                    center,
                    callback,
                    (doodad_scene_active || owner_fog.is_some()).then_some(placement_fog_color),
                )?)
            } else {
                None
            };
            // Native shadow traversal uses the light volume independently of
            // camera visibility, and attached models inherit root admission.
            let shadow_admitted = if let Some(projection) = shadow_projection {
                if let Some(parent) = self.placement_visibility.light_parent(placement_index) {
                    if parent < placement_index {
                        self.shadow_admission[parent]
                    } else {
                        forward_shadow
                    }
                } else {
                    shadow::admits_root(
                        projection,
                        source,
                        placement,
                        scenery_shadows.map(|queries| queries.admission),
                    )?
                }
            } else {
                false
            };
            self.shadow_admission[placement_index] = shadow_admitted;
            self.environment_shadow_admission[placement_index] = environment_maps;
            let shadow_bone_offset = u32::try_from(self.bone_transforms.len())
                .map_err(|_source| solarity_rendering::VulkanError::M2BoneTransformRange)?;
            let first_shadow_draw = self.shadow_draws.len();
            let first_environment_draw = self.environment_shadow_draws.len();
            // A new instance/clock must never observe the previous model's samples.
            self.material_pose_scratch.clear();
            if shadow_admitted || environment_maps != 0 {
                shadow::append_packets(
                    source,
                    placement,
                    clock,
                    model_view,
                    shadow_opacity,
                    shadow_bone_offset,
                    &mut self.material_pose_scratch,
                    |draw| {
                        if shadow_admitted {
                            self.shadow_draws.push(draw);
                        }
                        if environment_maps != 0 {
                            self.environment_shadow_draws.push(
                                solarity_rendering::WorldEnvironmentM2Caster {
                                    draw,
                                    maps: environment_maps,
                                },
                            );
                        }
                    },
                )?;
            }
            let has_shadow_bones = self.shadow_draws.len() != first_shadow_draw
                || self.environment_shadow_draws.len() != first_environment_draw;
            if has_shadow_bones {
                self.bone_transforms
                    .extend_from_slice(bone_pose.transforms());
            }
            if environment_maps != 0 && scenery_opacity == 0. {
                continue;
            }
            if doodad_scene_active {
                if !doodad_visible || scenery_opacity == 0.0 {
                    continue;
                }
            } else if !static_visibility_resolved
                || environment_maps != 0
                || (publishes_lights && placement.unit_effect.is_none())
            {
                let (center, radius) =
                    placement_bounding_sphere(&source.model, placement.transform);
                if !frustum.contains_sphere(center, radius)? {
                    continue;
                }
            }

            // 0x00828A00 advances a model from its own previous effect update.
            // The scene clock and subtraction wrap as unsigned milliseconds.
            let effect_time_ms = animation_time_ms as u32;
            let effect_delta_seconds =
                effect_time_ms.wrapping_sub(placement.last_effect_time_ms) as f32 * 0.001;
            placement.last_effect_time_ms = effect_time_ms;

            let bone_offset = shadow_bone_offset;
            let light_bank = placement_light_bank(placement.owner);
            let effect_retiring = placement
                .unit_effect
                .as_ref()
                .is_some_and(|effect| effect.retiring());
            let mut instance_color = placement_mesh_color(placement.owner, placement.color);
            if let Some(animation) = &placement.unit_animation {
                instance_color *= placement_color(animation.model_color().to_le_bytes());
            } else if let Some(pose) = placement
                .retirement
                .as_ref()
                .and_then(|retired| retired.unit_pose)
            {
                instance_color *= placement_color(pose.color.to_le_bytes());
            }
            instance_color.w *= placement_opacity;
            if placement.particles.len() != source.model.animations().particles().len() {
                return Err(RuntimeTerrainFrameError::M2ParticleSimulationCount {
                    model: source.model.path().clone(),
                    simulation_count: placement.particles.len(),
                    emitter_count: source.model.animations().particles().len(),
                });
            }
            if source.particles.len() != source.model.animations().particles().len() {
                return Err(RuntimeTerrainFrameError::M2ParticleResourceCount {
                    model: source.model.path().clone(),
                    resource_count: source.particles.len(),
                    emitter_count: source.model.animations().particles().len(),
                });
            }
            for (particle_index, ((emitter, placement_particle), resources)) in source
                .model
                .animations()
                .particles()
                .iter()
                .zip(&mut placement.particles)
                .zip(&source.particles)
                .enumerate()
            {
                if placement_particle.unsupported.is_some() {
                    if let Some(message) =
                        placement_particle.diagnostic(source.model.path(), particle_index)
                    {
                        self.recoverable_errors.push(message);
                    }
                    continue;
                }
                let simulation = &mut placement_particle.simulation;
                if effect_retiring {
                    simulation.set_emission_enabled(false);
                }
                let pose = M2ParticlePose::sample(source.model.animations(), emitter, clock)?;
                let emitter_transform =
                    bone_pose.particle_emitter_transform(emitter, placement.transform)?;
                let model_lod_position = particle_lod_origin(placement.transform);
                let particle_density = particle_emission_density(
                    emitter.flags(),
                    model_lod_position,
                    camera.camera().position(),
                );
                match emitter.emitter_type() {
                    1 => simulation.advance_planar_bounded(
                        emitter,
                        pose,
                        effect_delta_seconds,
                        emitter_transform,
                        particle_density,
                    ),
                    2 => simulation.advance_sphere_bounded(
                        emitter,
                        pose,
                        effect_delta_seconds,
                        emitter_transform,
                        particle_density,
                    ),
                    emitter_type => {
                        return Err(RuntimeTerrainFrameError::M2ParticleEmitterType {
                            model: source.model.path().clone(),
                            particle_index,
                            emitter_type,
                        });
                    }
                }
                .map_err(|source_error| {
                    RuntimeTerrainFrameError::M2PlacedParticleSimulation {
                        model: source.model.path().clone(),
                        particle_index,
                        time_ms: clock.animation_time_ms(),
                        source: source_error,
                    }
                })?;
                let (emitter_vertex_capacity, emitter_index_capacity) =
                    M2ParticleMeshPlan::buffer_capacity(emitter, simulation.capacity())?;
                particle_vertex_capacity = particle_vertex_capacity
                    .checked_add(emitter_vertex_capacity)
                    .ok_or(M2ParticleMeshPlanError::VertexCount)?;
                particle_index_capacity = particle_index_capacity
                    .checked_add(emitter_index_capacity)
                    .ok_or(M2ParticleMeshPlanError::IndexCount)?;
                self.particle_vertices
                    .reserve(particle_vertex_capacity.saturating_sub(self.particle_vertices.len()));
                self.particle_indices
                    .reserve(particle_index_capacity.saturating_sub(self.particle_indices.len()));
                if simulation.particles().is_empty() {
                    continue;
                }
                let particle_to_world = if emitter.particles_in_model_space() {
                    emitter_transform
                } else {
                    Mat4::IDENTITY
                };
                // Build 12340 `0x0097AC20` retains the complete view-model and
                // animated-emitter axis length. Its later flag-`0x20` card path
                // consequently includes native M2 camera aspect correction.
                let inherited_scale =
                    emitter_transform.x_axis.truncate().length() * effect_scale.factor();
                let first_vertex =
                    u32::try_from(self.particle_vertices.len()).map_err(|_source| {
                        solarity_rendering::VulkanError::M2ParticleDrawVertexRange
                    })?;
                let first_index = u32::try_from(self.particle_indices.len())
                    .map_err(|_source| solarity_rendering::VulkanError::M2ParticleDrawIndexRange)?;
                let (vertex_count, index_count) =
                    M2ParticleMeshPlan::append_transformed_with_particle_color(
                        emitter,
                        pose,
                        simulation.particles(),
                        camera,
                        particle_to_world,
                        inherited_scale,
                        instance_color.w,
                        &self.particle_twinkle,
                        placement.particle_colors.as_ref(),
                        &mut self.particle_sort_indices,
                        &mut self.particle_vertices,
                        &mut self.particle_indices,
                    )?;
                let effect_order = u32::try_from(self.transparent_elements.len())
                    .map_err(|_source| solarity_rendering::VulkanError::M2ParticleDrawIndexRange)?;
                let prepared = renderer
                    .prepare_m2_particle_draw_range(
                        if instance_color.w < 0.999_99 {
                            resources.runtime_fade_pipeline
                        } else {
                            resources.pipeline
                        },
                        resources.texture_set,
                        emitter.blending_type(),
                        emitter.flags(),
                        instance_color.w,
                        M2EffectOrder::new(emitter.priority_plane(), effect_order),
                        first_vertex,
                        first_index,
                        vertex_count,
                        index_count,
                    )?
                    .with_light_bank(light_bank)
                    .with_scene_index(scene_index);
                let prepared_index = self.particle_draws.len();
                let opaque =
                    !M2MaterialState::from_particle(emitter.blending_type(), emitter.flags())
                        .blend_enabled()
                        && M2ElementAlphaState::classify(instance_color.w)
                            == M2ElementAlphaState::Authored;
                let order = scene_element_count(
                    self.visible_draws.len(),
                    self.particle_draws.len(),
                    self.ribbon_draws.len(),
                )?;
                self.particle_draws.push(if opaque {
                    prepared.with_scene_order(
                        u32::try_from(order)
                            .map_err(|_| solarity_rendering::VulkanError::M2DrawIndexRange)?,
                    )
                } else {
                    prepared
                });
                if !opaque {
                    self.transparent_elements.push(M2TransparentElement {
                        pass: M2TransparentPass::for_particle_liquid(
                            emitter.flags(),
                            particle_liquid.above(),
                        ),
                        key: M2TransparentSortKey::new(
                            instance_distance,
                            false,
                            emitter.priority_plane(),
                            instance_distance,
                            instance_identity,
                            0,
                        )
                        .with_scene_element(4, effect_order),
                        draw: M2TransparentDrawIndex::Particle(prepared_index),
                    });
                }
                tracing::trace!(
                    model = %source.model.path(),
                    particle_index,
                    live_count = simulation.particles().len(),
                    vertex_count,
                    index_count,
                    "placement-local particles entered unified world frame"
                );
            }
            advance_ribbons(
                &source.model,
                placement,
                bone_pose,
                clock,
                effect_delta_seconds,
                effect_scale,
                instance_color.w,
            )?;
            if !has_shadow_bones {
                self.bone_transforms
                    .extend_from_slice(bone_pose.transforms());
            }
            if source.mesh.is_some() && !effect_retiring {
                for (draw_index, resources) in source.draws.iter().enumerate() {
                    let Some(resources) = resources else {
                        continue;
                    };
                    let pose = self
                        .material_pose_scratch
                        .get(draw_index)
                        .copied()
                        .flatten()
                        .map_or_else(
                            || {
                                M2MaterialPose::sample(
                                    &source.model,
                                    &source.plan,
                                    draw_index,
                                    clock,
                                )
                            },
                            Ok,
                        )?;
                    let draw = &source.plan.draws()[draw_index];
                    let material_state = M2MaterialState::from_material(draw.material());
                    let element_alpha = pose.mesh_color().w * instance_color.w;
                    let alpha_state = M2ElementAlphaState::classify(element_alpha);
                    if alpha_state == M2ElementAlphaState::Hidden {
                        continue;
                    }
                    let runtime_alpha_fade = alpha_state == M2ElementAlphaState::Translucent
                        && !material_state.blend_enabled();
                    let material = M2MaterialUniform::new(
                        placement.transform,
                        pose.texture_transforms(),
                        model_view,
                        pose.mesh_color() * instance_color,
                        placement_fog_color.extend(1.0),
                        glam::Vec4::new(
                            material_state.alpha_reference(instance_color.w),
                            material_state.fog_mode().shader_code(),
                            0.0,
                            0.0,
                        ),
                    );
                    let template = if runtime_alpha_fade {
                        resources.fade_template.ok_or(
                            RuntimeTerrainFrameError::M2RuntimeFadePipeline {
                                model: source.model.path().clone(),
                                draw_index,
                            },
                        )?
                    } else {
                        resources.template
                    };
                    let prepared = template
                        .instantiate(material, bone_offset, 0)?
                        .with_light_bank(light_bank)
                        .with_scene_index(scene_index);
                    if draw.transparent_sort_unit()
                        || alpha_state == M2ElementAlphaState::Translucent
                    {
                        let section_distance = section_distance_key(draw, bone_pose, model_view)?;
                        let primary_distance = if self
                            .placement_visibility
                            .model_distance_sort(placement_index)
                        {
                            instance_distance
                        } else {
                            section_distance
                        };
                        let producer_order = u32::try_from(self.transparent_elements.len())
                            .map_err(|_source| solarity_rendering::VulkanError::M2DrawIndexRange)?;
                        let key = M2TransparentSortKey::new(
                            primary_distance,
                            false,
                            i16::from(draw.batch().priority_plane),
                            section_distance,
                            instance_identity,
                            draw.batch().material_layer,
                        )
                        .with_scene_element(0, producer_order);
                        for (pass, admitted) in [
                            (M2TransparentPass::One, model_liquid.above()),
                            (M2TransparentPass::Two, model_liquid.below()),
                        ] {
                            if !admitted {
                                continue;
                            }
                            let prepared_index = self.visible_draws.len();
                            self.visible_draws.push(
                                prepared.with_liquid_clip_plane(model_liquid.clip_plane(pass)),
                            );
                            self.transparent_elements.push(M2TransparentElement {
                                pass,
                                key,
                                draw: M2TransparentDrawIndex::Mesh(prepared_index),
                            });
                        }
                    } else {
                        let scene_order = u32::try_from(scene_element_count(
                            self.visible_draws.len(),
                            self.particle_draws.len(),
                            self.ribbon_draws.len(),
                        )?)
                        .map_err(|_source| solarity_rendering::VulkanError::M2DrawIndexRange)?;
                        self.visible_draws
                            .push(prepared.with_scene_order(scene_order));
                    }
                }
            } else {
                debug_assert!(effect_retiring || source.draws.iter().all(Option::is_none));
            }
            for (ribbon_index, ((emitter, trail), passes)) in source
                .model
                .animations()
                .ribbons()
                .iter()
                .zip(&placement.ribbons)
                .zip(&source.ribbons)
                .enumerate()
            {
                // 6F87C0 removes normal model submission; 828A00 retains only
                // the particle pass while model bit 0x400 remains live.
                if effect_retiring {
                    continue;
                }
                let Some(root_pass) = passes.first() else {
                    continue;
                };
                if trail.sections().len() < 2 {
                    continue;
                }
                let first_vertex = u32::try_from(self.ribbon_vertices.len())
                    .map_err(|_source| solarity_rendering::VulkanError::M2RibbonDrawVertexRange)?;
                let vertex_count =
                    M2RibbonMeshPlan::append(emitter, trail, &mut self.ribbon_vertices)?;
                let ribbon_alpha = M2RibbonPose::sample(source.model.animations(), emitter, clock)?
                    .color()
                    .w
                    * instance_color.w;
                // 821A20 registers one type-3 scene element for the emitter.
                // Its first material and owner/track alpha classify the whole
                // ribbon; 820F40/980B70 then submit every pass consecutively.
                let effect_order = u32::try_from(self.transparent_elements.len())
                    .map_err(|_source| solarity_rendering::VulkanError::M2RibbonDrawVertexRange)?;
                let first_draw = self.ribbon_draws.len();
                let opaque = !M2MaterialState::from_material(root_pass.material).blend_enabled()
                    && M2ElementAlphaState::classify(ribbon_alpha) == M2ElementAlphaState::Authored;
                let order = scene_element_count(
                    self.visible_draws.len(),
                    self.particle_draws.len(),
                    first_draw,
                )?;
                for (pass_index, pass) in passes.iter().enumerate() {
                    let prepared = renderer
                        .prepare_m2_ribbon_draw_range(
                            pass.pipeline,
                            pass.texture_set,
                            pass.material,
                            M2EffectOrder::new(emitter.priority_plane(), effect_order),
                            first_vertex,
                            vertex_count,
                        )?
                        .with_light_bank(light_bank)
                        .with_scene_index(scene_index)
                        .with_first_material_pass(pass_index == 0);
                    self.ribbon_draws.push(if opaque {
                        prepared.with_scene_order(
                            u32::try_from(order)
                                .map_err(|_| solarity_rendering::VulkanError::M2DrawIndexRange)?,
                        )
                    } else {
                        prepared
                    });
                }
                if !opaque {
                    self.transparent_elements.push(M2TransparentElement {
                        pass: model_liquid.ribbon_pass(),
                        key: M2TransparentSortKey::new(
                            instance_distance,
                            false,
                            emitter.priority_plane(),
                            instance_distance,
                            instance_identity,
                            0,
                        )
                        .with_scene_element(3, effect_order),
                        draw: M2TransparentDrawIndex::Ribbon {
                            first: first_draw,
                            count: passes.len(),
                        },
                    });
                }
                tracing::trace!(
                    model = %source.model.path(),
                    ribbon_index,
                    vertex_count,
                    pass_count = passes.len(),
                    "placement-local ribbon entered unified world frame"
                );
            }
        }
        frame_profile.mark("instance traversal");
        self.transparent_elements.sort_unstable_by(|left, right| {
            (left.pass != first_transparent_pass)
                .cmp(&(right.pass != first_transparent_pass))
                .then_with(|| compare_m2_transparent(&left.key, &right.key))
        });
        let first_transparent_order = scene_element_count(
            self.visible_draws.len(),
            self.particle_draws.len(),
            self.ribbon_draws.len(),
        )?;
        let water_scene_order = first_transparent_order
            .checked_add(
                self.transparent_elements
                    .iter()
                    .take_while(|element| element.pass == first_transparent_pass)
                    .count(),
            )
            .and_then(|index| u32::try_from(index).ok())
            .ok_or(solarity_rendering::VulkanError::M2DrawIndexRange)?;
        for (index, element) in self.transparent_elements.iter().enumerate() {
            let scene_order = first_transparent_order
                .checked_add(index)
                .and_then(|index| u32::try_from(index).ok())
                .ok_or(solarity_rendering::VulkanError::M2DrawIndexRange)?;
            match element.draw {
                M2TransparentDrawIndex::Mesh(draw_index) => {
                    let draw = self
                        .visible_draws
                        .get_mut(draw_index)
                        .ok_or(solarity_rendering::VulkanError::M2DrawIndexRange)?;
                    *draw = draw.with_scene_order(scene_order);
                }
                M2TransparentDrawIndex::Particle(draw_index) => {
                    let draw = self
                        .particle_draws
                        .get_mut(draw_index)
                        .ok_or(solarity_rendering::VulkanError::M2ParticleDrawIndexRange)?;
                    *draw = draw.with_scene_order(scene_order);
                }
                M2TransparentDrawIndex::Ribbon { first, count } => {
                    let draws = self
                        .ribbon_draws
                        .get_mut(first..first + count)
                        .ok_or(solarity_rendering::VulkanError::M2RibbonDrawVertexRange)?;
                    for draw in draws {
                        *draw = draw.with_scene_order(scene_order);
                    }
                }
            }
        }
        self.visible_draws.sort_by_key(|draw| draw.scene_order());
        self.particle_draws.sort_by_key(|draw| draw.scene_order());
        self.ribbon_draws.sort_by_key(|draw| draw.scene_order());
        frame_profile.mark("transparent order");
        if let Some((base, exterior)) = world_lighting {
            self.scene_lighting.finish(base, exterior)?;
        }
        frame_profile.mark("scene lights");
        Ok(M2VisibleFrame {
            instance_scenes: &self.scene_lighting.scenes,
            scene_points: &self.scene_lighting.points,
            scene_directionals: self.scene_lighting.directionals(),
            water_scene_order,
            bone_transforms: &self.bone_transforms,
            draws: &self.visible_draws,
            shadow_draws: &self.shadow_draws,
            environment_shadow_draws: &self.environment_shadow_draws,
            particle_vertices: &self.particle_vertices,
            particle_indices: &self.particle_indices,
            particle_draws: &self.particle_draws,
            particle_vertex_capacity,
            particle_index_capacity,
            ribbon_vertices: &self.ribbon_vertices,
            ribbon_draws: &self.ribbon_draws,
            glue_directional_lights: &self.glue_directional_lights,
            glue_point_lights: &self.glue_point_lights,
        })
    }
}
