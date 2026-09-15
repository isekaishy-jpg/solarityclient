//! Selected-placement preparation; source admission and topology publication live separately.

use super::super::{
    CrtRand, GameObjectFrameInput, M2AnimationClock, M2BonePoseOverrides, M2CameraEffectScale,
    M2Frame, M2GpuPlacementOwner, M2PlaybackStorage, M2TransparentDrawIndex, M2TransparentPass,
    M2VisibleFrame, Mat4, RuntimeTerrainFrameError, VulkanRenderer, WorldCameraFrame, WorldFrustum,
    append_triggered_events, compare_m2_transparent, held_item_finger_pose, m2_model_distance_key,
    placement_bounding_sphere, placement_color, placement_light_bank, placement_mesh_color,
    placement_owner_guid, scene_element_count, shadow, unit_effects,
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
        let mut frame_profile = solarity_profiling::profile!("M2 frame preparation");
        let _cycles = solarity_profiling::profile_cycles!("m2.prepare_cpu");
        let mut work = super::diagnostics::Work::new();
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
        self.receiver_frame.clear();
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
                    .replace_effect_tail(&mut self.placements, &self.sources);
            }
            self.compact_sources();
        }
        self.publish_placement_topology();
        frame_profile.mark("residency and topology");
        self.vehicle_passengers.prepare_timing(
            self.placements.as_mut_slice(),
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
            self.placements.as_mut_slice(),
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
        self.prepare_unit_poses(
            cpu,
            super::poses::PoseAdmission::new(
                camera,
                frustum,
                shadow_projection,
                scenery_shadows,
                self.environment_detail,
            ),
            animation_time_ms as u32,
        )?;
        frame_profile.mark("unit pose batch");
        if let Some((terrain, ..)) = spatial_lighting.as_ref() {
            self.doodad_scene.prepare(
                terrain,
                &self.placement_visibility,
                self.placements.as_mut_slice(),
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
        self.begin_geometry(cpu)?;
        let traversal_result = (|| -> Result<(), RuntimeTerrainFrameError> {
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
                    if self.unit_effects.publish(
                        &mut self.placements,
                        &mut self.sources,
                        effect_start,
                    ) {
                        self.placement_visibility
                            .replace_effect_tail(&mut self.placements, &self.sources);
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
                let mut placement_profile = solarity_profiling::detail_profile!("m2.placement");
                placement_profile.trace_owner(
                    placement_index as u64 + 1,
                    self.placements[placement_index].source_index as u64 + 1,
                );
                let mut observed = work.placement();
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
                            !scenery
                                .admits_shadow(camera.camera().position(), self.environment_detail)
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
                let publishes_lights = world_lighting.is_some()
                    && self.placement_visibility.has_lights(placement_index);
                observed.light_owner = publishes_lights;
                observed.environment_shadow = environment_maps != 0;
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
                    .map(|root| {
                        m2_model_distance_key(camera.view() * self.placements[root].transform)
                    });
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
                    && let Some(event) =
                        effect.take_ready_sound(placement.transform.w_axis.truncate())
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
                        .find_map(|(owner_guid, transform)| {
                            (*owner_guid == guid).then_some(*transform)
                        })
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
                if solarity_profiling::TraceContext::capture().is_sampled()
                    && let Some(guid) = placement_owner_guid(placement.owner)
                {
                    solarity_profiling::TraceContext::capture().value(
                        "m2.owner.guid",
                        placement_index as u64 + 1,
                        0,
                        guid,
                    );
                }
                observed.source(
                    placement.source_index as u64 + 1,
                    source.model.path().as_str(),
                );
                observed.callback_owner = placement.unit_animation.is_some()
                    || placement.unit_effect.is_some()
                    || matches!(owner, M2GpuPlacementOwner::GameObject { .. });
                observed.particle_owner = !placement.particles.is_empty();
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
                    let advance = if let Some(advance) = placement.passenger_playback_advance.take()
                    {
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
                    self.bone_demand.clear();
                    self.bone_demand
                        .events(&source.model, owner, expired.event_window);
                    self.bone_samples_scratch.recompose(
                        source.model.animations(),
                        expired.clock,
                        camera.view() * placement.transform,
                        M2BonePoseOverrides {
                            model_oriented_billboard_bones: &source.model_oriented_billboard_bones,
                            bone_transforms,
                            bone_sequences: &expired.bone_sequences,
                            ..Default::default()
                        },
                        self.bone_demand.bones(),
                    )?;
                    let first_event = self.triggered_events.len();
                    append_triggered_events(
                        &mut self.triggered_events,
                        &source.model,
                        placement.owner,
                        placement.transform,
                        &placement.sound_lifetime,
                        &self.bone_samples_scratch,
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
                let event_window = prepared_event_window
                    .unwrap_or_else(|| playback.event_window(animation_time_ms));
                drop(playback);
                let model_view = camera.view() * placement.transform;
                let instance_identity = std::ptr::from_ref(&*placement).addr();
                let instance_distance =
                    inherited_model_distance.unwrap_or_else(|| m2_model_distance_key(model_view));
                // 832450 callbacks remain active independently of 823F10 render
                // registration. Resolve geometry demand before full-palette work.
                let hidden = placement
                    .entity_opacity
                    .as_ref()
                    .is_some_and(|owner| owner.hidden());
                // Native shadow traversal uses the light volume independently of
                // camera visibility, and attached models inherit root admission.
                let shadow_admitted = if let Some(projection) =
                    shadow_projection.filter(|_| !hidden)
                {
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
                let visible = !hidden
                    && if environment_maps != 0 && scenery_opacity == 0. {
                        false
                    } else if doodad_scene_active {
                        doodad_visible && scenery_opacity != 0.
                    } else if !static_visibility_resolved
                        || environment_maps != 0
                        || (publishes_lights && placement.unit_effect.is_none())
                    {
                        let (center, radius) =
                            placement_bounding_sphere(&source.model, placement.transform);
                        frustum.contains_sphere(center, radius)?
                    } else {
                        true
                    };
                let needs_palette =
                    !hidden && (visible || shadow_admitted || environment_maps != 0);
                let overrides = M2BonePoseOverrides {
                    model_oriented_billboard_bones: &source.model_oriented_billboard_bones,
                    finger_pose,
                    bone_transforms,
                    bone_sequences: &bone_sequences,
                };
                observed.admitted = true;
                observed.visible = visible;
                observed.primary_shadow = shadow_admitted;
                observed.palette = needs_palette;
                placement_profile.mark("admission and animation");
                // Ordered callbacks require only their named bones. When there are
                // no CPU bone consumers, full visible palettes can join the same
                // owned job as effects and mesh packets after scene publication.
                let batch_hit = needs_palette
                    && self.pose_batch.take(
                        placement_index,
                        &source.model,
                        clock,
                        model_view,
                        overrides,
                        &mut self.bone_pose_scratch,
                    )?;
                observed.batch_hit = batch_hit;
                if !batch_hit {
                    self.bone_demand.model(super::demand::CpuModelInputs {
                        placement,
                        model: &source.model,
                        window: event_window,
                        items: &self.requested_items,
                        visuals: &self.requested_visuals,
                        glue_ids: &self.glue_attachment_ids,
                        effects: &self.unit_effects,
                        publishes_lights,
                    });
                }
                let deferred_palette =
                    needs_palette && visible && !batch_hit && self.bone_demand.bones().is_empty();
                let unrequested =
                    super::demand::UnrequestedBones(source.model.animations().bones().len());
                if solarity_profiling::TraceContext::capture().is_sampled() {
                    solarity_profiling::TraceContext::capture().value(
                        "m2.cpu_bone_demand",
                        placement_index as u64 + 1,
                        u64::from(deferred_palette),
                        self.bone_demand.bones().len() as u64,
                    );
                }
                let bone_pose: &dyn solarity_rendering::M2BoneTransforms = if deferred_palette {
                    &unrequested
                } else if needs_palette {
                    if !batch_hit {
                        self.bone_pose_scratch.recompose_with_overrides(
                            source.model.animations(),
                            clock,
                            model_view,
                            overrides,
                        )?;
                    }
                    &self.bone_pose_scratch
                } else {
                    self.bone_samples_scratch.recompose(
                        source.model.animations(),
                        clock,
                        model_view,
                        overrides,
                        self.bone_demand.bones(),
                    )?;
                    &self.bone_samples_scratch
                };
                placement_profile.mark("pose");
                let first_cpu_output = self.triggered_events.len()
                    + self.rider_transforms.len()
                    + self.item_transforms.len()
                    + self.visual_transforms.len()
                    + self.glue_attachment_transforms.len();
                super::publication::CpuPublication {
                    retirement: &mut self.retirement,
                    triggered_events: &mut self.triggered_events,
                    unit_effects: &mut self.unit_effects,
                    scene_lighting: &mut self.scene_lighting,
                    glue_directional_lights: &mut self.glue_directional_lights,
                    glue_point_lights: &mut self.glue_point_lights,
                    glue_attachment_ids: &self.glue_attachment_ids,
                    glue_attachment_transforms: &mut self.glue_attachment_transforms,
                    mount_camera_sample: &mut self.mount_camera_sample,
                    rider_transforms: &mut self.rider_transforms,
                    requested_items: &self.requested_items,
                    item_transforms: &mut self.item_transforms,
                    requested_visuals: &self.requested_visuals,
                    visual_transforms: &mut self.visual_transforms,
                }
                .publish(
                    &source.model,
                    placement,
                    bone_pose,
                    super::publication::CpuSample {
                        clock,
                        event_window,
                        animation_time_ms,
                        placement_opacity,
                        publishes_lights,
                    },
                )?;
                observed.cpu_output = self.triggered_events.len()
                    + self.rider_transforms.len()
                    + self.item_transforms.len()
                    + self.visual_transforms.len()
                    + self.glue_attachment_transforms.len()
                    != first_cpu_output;
                placement_profile.mark("CPU publication");
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
                // Receiver callbacks belong to submitted geometry. Retain only the
                // ancestry inputs here; the final demand pass selects actual consumers.
                let scene_index = if world_lighting.is_some() {
                    Some(self.receiver_frame.record(
                        placement_index,
                        self.placement_visibility.light_parent(placement_index),
                        placement.transform.w_axis.truncate(),
                        doodad_scene_active.then_some(placement_fog_color),
                    )?)
                } else {
                    None
                };
                let bone_pose = &self.bone_pose_scratch;
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
                observed.shadow_output = has_shadow_bones;
                if has_shadow_bones {
                    if deferred_palette {
                        let end = self
                            .bone_transforms
                            .len()
                            .checked_add(source.model.animations().bones().len())
                            .ok_or(solarity_rendering::VulkanError::M2BoneTransformRange)?;
                        self.bone_transforms.resize(end, Mat4::IDENTITY);
                    } else {
                        self.bone_transforms
                            .extend_from_slice(bone_pose.transforms());
                    }
                }
                placement_profile.mark("shadow preparation");
                if !visible {
                    continue;
                }

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
                if world_lighting.is_some() {
                    self.receiver_frame.set_fog(
                        placement_index,
                        (doodad_scene_active || owner_fog.is_some()).then_some(placement_fog_color),
                    );
                }

                placement_profile.mark("liquid and fog");
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
                if !has_shadow_bones {
                    if deferred_palette {
                        let end = self
                            .bone_transforms
                            .len()
                            .checked_add(source.model.animations().bones().len())
                            .ok_or(solarity_rendering::VulkanError::M2BoneTransformRange)?;
                        self.bone_transforms.resize(end, Mat4::IDENTITY);
                    } else {
                        self.bone_transforms
                            .extend_from_slice(bone_pose.transforms());
                    }
                }
                let input = super::geometry::GeometryInput {
                    placement_index,
                    trace: solarity_profiling::TraceContext::capture(),
                    source_index: placement.source_index,
                    clock,
                    effect_delta_seconds,
                    transform: placement.transform,
                    model_view,
                    instance_color,
                    placement_fog_color,
                    light_bank,
                    scene_index,
                    bone_offset,
                    effect_retiring,
                    particle_liquid,
                    model_liquid,
                    instance_distance,
                    instance_identity,
                    model_distance_sort: self
                        .placement_visibility
                        .model_distance_sort(placement_index),
                    particle_colors: placement.particle_colors,
                    has_shadow_bones,
                };
                observed.geometry_pending = true;
                self.geometry_batch.queue(
                    input,
                    &mut self.placements[placement_index],
                    &mut self.bone_pose_scratch,
                    &mut self.material_pose_scratch,
                    deferred_palette.then_some(overrides),
                    source,
                    camera,
                    effect_scale,
                    &self.particle_twinkle,
                )?;
            }
            Ok(())
        })();
        let geometry_result = self.finish_geometry();
        self.restore_geometry_states();
        traversal_result?;
        geometry_result?;
        let (particle_vertex_capacity, particle_index_capacity) =
            self.publish_geometry(&mut work)?;
        self.pose_batch.finish()?;
        self.pose_batch.report_consumption();
        frame_profile.mark("instance traversal");
        solarity_profiling::profile_value!("m2.resident_placements", self.placements.len());
        solarity_profiling::profile_value!(
            "m2.dynamic_placements",
            self.placement_visibility.dynamic_indices().len()
        );
        solarity_profiling::profile_value!("m2.particle_vertices", self.particle_vertices.len());
        solarity_profiling::profile_value!("m2.bone_transforms", self.bone_transforms.len());
        if world_lighting.is_some() {
            self.prepare_visible_receivers(
                animation_time_ms,
                spatial_lighting
                    .as_mut()
                    .map(|(terrain, environment, ..)| (&mut **terrain, *environment)),
            )?;
        }
        frame_profile.mark("visible receiver queries");
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
            trace: solarity_profiling::TraceContext::capture(),
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
