//! Selected-placement preparation; source admission and topology publication live separately.

use super::super::super::{
    CrtRand, GameObjectFrameInput, M2AnimationClock, M2BonePoseOverrides, M2Frame,
    M2GpuPlacementOwner, M2PlaybackStorage, M2TransparentPass, Mat4, RuntimeTerrainFrameError,
    append_triggered_events, held_item_finger_pose, m2_model_distance_key,
    placement_bounding_sphere, placement_color, placement_light_bank, placement_mesh_color,
    placement_owner_guid, shadow,
};

use super::input::{FrameAdmission, FrameView, SelectedPlacement};

impl M2Frame {
    /// Selects clocks once, then suspends at either an existing root or a discovered
    /// bone dependency. No pause repeats callbacks, RNG, attachments or event windows.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn admit_visible_draws(
        &mut self,
        cpu: &solarity_cpu::CpuExecutor,
        view: FrameView,
        admission: &mut FrameAdmission,
        random: &mut CrtRand,
        game_objects: Option<GameObjectFrameInput<'_>>,
        mut spatial_lighting: Option<(
            &mut crate::application::terrain_coordinator::RuntimeTerrainCoordinator,
            solarity_systems::WorldEntityLightEnvironment,
            glam::Vec3,
            &solarity_asset::LiquidTypeCatalog,
        )>,
        scenery_shadows: Option<
            crate::application::terrain_frame::shadow::SceneryShadowQueries<'_>,
        >,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let FrameView {
            frustum,
            liquid_clipping_enabled,
            camera,
            first_transparent_pass,
            fog_color,
            animation_time_ms,
            effect_scale,
            world_lighting,
            shadow_projection,
        } = view;
        let mut frame_profile = solarity_profiling::profile!("m2.frame_admission");
        let _cycles = solarity_profiling::profile_cycles!("m2.prepare_cpu");
        let effect_start = self.placement_visibility.effect_start();
        let complete = (|| -> Result<bool, RuntimeTerrainFrameError> {
            loop {
                if admission.pending.is_some() {
                    if !self.late_pose.is_ready() {
                        return Ok(false);
                    }
                    self.late_pose
                        .finish(&mut crate::application::frame_pipeline::FrameWait::Offline)?;
                }
                let pending = admission.pending.take();
                if pending.is_none() {
                    if !admission.effects_published
                        && self
                            .frame_work
                            .next_index()
                            .is_none_or(|index| index >= effect_start)
                    {
                        admission.effects_published = true;
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
                    // Poll before any per-owner mutation. Resuming must neither tick
                    // animation twice nor consume a second RNG/event window.
                    if let Some(index) = self.frame_work.next_index()
                        && !self.spatial_batch.is_ready(index)?
                    {
                        frame_profile.mark("spatial readiness yield");
                        solarity_profiling::profile_value!("m2.spatial_readiness_yield", 1);
                        return Ok(false);
                    }
                    if let Some(index) = self.frame_work.next_index()
                        && !self.pose_batch.is_ready(index)?
                    {
                        frame_profile.mark("pose readiness yield");
                        solarity_profiling::profile_value!("m2.pose_readiness_yield", 1);
                        return Ok(false);
                    }
                }
                if pending.is_none() && self.frame_work.next_index().is_none() {
                    break;
                }
                let mut observed = admission
                    .work
                    .resume_placement(pending.as_ref().map(|(_, state)| *state));
                let mut selected = if let Some((selected, _)) = pending {
                    selected
                } else {
                    let Some(placement_index) = self.frame_work.next() else {
                        break;
                    };
                    let mut placement_profile = solarity_profiling::detail_profile!("m2.placement");
                    placement_profile.trace_owner(
                        placement_index as u64 + 1,
                        self.placements[placement_index].source_index as u64 + 1,
                    );
                    let static_admission = self.spatial_batch.take(placement_index)?;
                    // Moving-parent transforms have already been resolved. Forward
                    // attachments query that same root; earlier roots reuse admission.
                    let environment_maps = if let Some(prepared) = static_admission {
                        prepared.environment_maps
                    } else if let Some(queries) = scenery_shadows {
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
                                !scenery.admits_shadow(
                                    camera.camera().position(),
                                    self.environment_detail,
                                )
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
                    if static_admission.is_some_and(|prepared| prepared.rejected) {
                        continue;
                    }
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
                    let placement_fog_color = if doodad_scene_active && doodad_fog == Some(false) {
                        spatial_lighting
                            .as_ref()
                            .map_or(fog_color, |(_, _, ordinary, _)| *ordinary)
                    } else {
                        fog_color
                    };
                    let scenery_opacity = if let Some(prepared) = static_admission {
                        prepared.scenery_opacity
                    } else if doodad_scene_active {
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
                        && static_admission.is_none()
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
                            .find_map(|(id, transform)| {
                                (*id == attachment_id).then_some(*transform)
                            })
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
                        let transform = self.rider_transforms.first(guid).ok_or(
                            RuntimeTerrainFrameError::MissingMountM2AttachmentPose {
                                guid,
                                attachment_id: 0,
                            },
                        )?;
                        let Some(transform) = transform else {
                            continue;
                        };
                        placement.transform =
                            transform * Mat4::from_scale(glam::Vec3::splat(placement.rider_scale));
                    }
                    if let M2GpuPlacementOwner::UnitItem { guid, point } = placement.owner {
                        if self.rider_transforms.any_hidden(guid) {
                            continue;
                        }
                        let transform = self.item_transforms.first((guid, point)).ok_or(
                            RuntimeTerrainFrameError::MissingPlayerM2AttachmentPose {
                                guid,
                                attachment_id: point.id(),
                            },
                        )?;
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
                        if self.rider_transforms.any_hidden(guid) {
                            continue;
                        }
                        let transform = self
                            .visual_transforms
                            .first((guid, item_point, effect_point))
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
                    self.bone_samples_scratch.reserve_cpu_storage(
                        &admission.storage,
                        source.model.animations().bones().len(),
                    )?;
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
                    let scene_sample = if let M2GpuPlacementOwner::GameObject { identity, .. } =
                        owner
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
                        let advance = if let Some(advance) =
                            placement.passenger_playback_advance.take()
                        {
                            advance
                        } else if let Some(effect) = &mut placement.unit_effect {
                            effect.advance(
                                &mut playback,
                                &source.model,
                                animation_time_ms,
                                random,
                                Some(
                                    self.callback_scratch
                                        .bind(&admission.storage, source.callback_queue_capacity),
                                ),
                            )?
                        } else if matches!(owner, M2GpuPlacementOwner::GlueModel { .. }) {
                            self.pending_glue_playback_advance.take().map_or_else(
                                || {
                                    playback.clock_with_storage(
                                        &source.model,
                                        animation_time_ms,
                                        random,
                                        self.callback_scratch.bind(
                                            &admission.storage,
                                            source.callback_queue_capacity,
                                        ),
                                    )
                                },
                                Ok,
                            )?
                        } else {
                            playback.clock_with_storage(
                                &source.model,
                                animation_time_ms,
                                random,
                                self.callback_scratch
                                    .bind(&admission.storage, source.callback_queue_capacity),
                            )?
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
                    let clock = advance.clock;
                    self.bone_clock_scratch.capture(
                        &admission.storage,
                        playback.bone_sequence_clock_iter(
                            &source.model,
                            clock,
                            animation_time_ms as u32,
                        ),
                    )?;
                    let finger_pose_hands = placement
                        .retirement
                        .as_ref()
                        .and_then(|retired| retired.finger_hands)
                        .or_else(|| held_item_finger_pose(&self.requested_items, owner));
                    let finger_pose =
                        finger_pose_hands.and_then(|hands| {
                            source.model.animations().sequence_for_variation(15, 0).map(
                                |sequence| {
                                    (
                                        M2AnimationClock::new_with_global_tick(
                                            sequence,
                                            0.0,
                                            playback.global_tick(animation_time_ms as u32),
                                        ),
                                        hands,
                                    )
                                },
                            )
                        });
                    let event_window = prepared_event_window
                        .unwrap_or_else(|| playback.event_window(animation_time_ms));
                    drop(playback);
                    let model_view = camera.view() * placement.transform;
                    let instance_identity = std::ptr::from_ref(&*placement).addr();
                    let instance_distance = inherited_model_distance
                        .unwrap_or_else(|| m2_model_distance_key(model_view));
                    // 832450 callbacks remain active independently of 823F10 render
                    // registration. Resolve geometry demand before full-palette work.
                    let hidden = placement
                        .entity_opacity
                        .as_ref()
                        .is_some_and(|owner| owner.hidden());
                    // Native shadow traversal uses the light volume independently of
                    // camera visibility, and attached models inherit root admission.
                    let shadow_admitted =
                        if let Some(projection) = shadow_projection.filter(|_| !hidden) {
                            if let Some(parent) =
                                self.placement_visibility.light_parent(placement_index)
                            {
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
                    observed.admitted = true;
                    observed.visible = visible;
                    observed.primary_shadow = shadow_admitted;
                    observed.palette = needs_palette;
                    SelectedPlacement {
                        expired: advance.expired_variations,
                        expired_next: 0,
                        expired_started: false,
                        placement_index,
                        environment_maps,
                        publishes_lights,
                        doodad_scene_active,
                        placement_fog_color,
                        shadow_opacity,
                        placement_opacity,
                        clock,
                        body_pose,
                        finger_pose,
                        event_window,
                        model_view,
                        instance_identity,
                        instance_distance,
                        shadow_admitted,
                        visible,
                        needs_palette,
                    }
                };
                let placement_index = selected.placement_index;
                let environment_maps = selected.environment_maps;
                let publishes_lights = selected.publishes_lights;
                let doodad_scene_active = selected.doodad_scene_active;
                let mut placement_fog_color = selected.placement_fog_color;
                let shadow_opacity = selected.shadow_opacity;
                let placement_opacity = selected.placement_opacity;
                let clock = selected.clock;
                let finger_pose = selected.finger_pose;
                let event_window = selected.event_window;
                let model_view = selected.model_view;
                let instance_identity = selected.instance_identity;
                let instance_distance = selected.instance_distance;
                let shadow_admitted = selected.shadow_admitted;
                let visible = selected.visible;
                let needs_palette = selected.needs_palette;
                let placement = &mut self.placements[placement_index];
                let source = self.sources[placement.source_index]
                    .as_ref()
                    .unwrap_or_else(|| unreachable!("selected placement retains source"));
                if !selected.expired.is_empty() {
                    if !selected.expired_started {
                        for (ordinal, expired) in selected.expired.iter().enumerate() {
                            self.bone_demand.begin(
                                &admission.storage,
                                source.model.animations().bones().len(),
                            )?;
                            self.bone_demand.events(
                                &source.model,
                                placement.owner,
                                expired.event_window,
                            );
                            self.event_poses.seed(
                                cpu,
                                ordinal,
                                source,
                                expired.clock,
                                model_view,
                                M2BonePoseOverrides {
                                    model_oriented_billboard_bones: &source
                                        .model_oriented_billboard_bones,
                                    bone_transforms: selected
                                        .body_pose
                                        .as_ref()
                                        .map_or(&[], |pose| pose.bone_transforms()),
                                    bone_sequences: &expired.bone_sequences,
                                    ..Default::default()
                                },
                                self.bone_demand.bones(),
                            )?;
                        }
                        self.event_poses.start(cpu)?;
                        selected.expired_started = true;
                    }
                    while selected.expired_next < selected.expired.len() {
                        let ordinal = selected.expired_next;
                        if !self.event_poses.is_ready(ordinal)? {
                            admission.pending = Some((selected, observed.pause()));
                            return Ok(false);
                        }
                        let expired = &selected.expired[ordinal];
                        self.bone_demand
                            .begin(&admission.storage, source.model.animations().bones().len())?;
                        self.bone_demand.events(
                            &source.model,
                            placement.owner,
                            expired.event_window,
                        );
                        let samples = self.event_poses.sample(
                            Some(cpu),
                            &mut crate::application::frame_pipeline::FrameWait::Offline,
                            ordinal,
                            source,
                            expired.clock,
                            model_view,
                            M2BonePoseOverrides {
                                model_oriented_billboard_bones: &source
                                    .model_oriented_billboard_bones,
                                bone_transforms: selected
                                    .body_pose
                                    .as_ref()
                                    .map_or(&[], |pose| pose.bone_transforms()),
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
                            samples,
                            expired.event_window,
                        )?;
                        if let Some(effect) = &placement.unit_effect {
                            effect.bind_sound_events(&mut self.triggered_events[first_event..]);
                        }
                        selected.expired_next += 1;
                    }
                    self.event_poses
                        .finish(&mut crate::application::frame_pipeline::FrameWait::Offline)?;
                    selected.expired.clear();
                }
                let overrides = M2BonePoseOverrides {
                    model_oriented_billboard_bones: &source.model_oriented_billboard_bones,
                    finger_pose,
                    bone_transforms: selected
                        .body_pose
                        .as_ref()
                        .map_or(&[], |pose| pose.bone_transforms()),
                    bone_sequences: self.bone_clock_scratch.values(),
                };
                let mut placement_profile =
                    solarity_profiling::detail_profile!("m2.selected_placement");
                placement_profile.trace_owner(
                    placement_index as u64 + 1,
                    placement.source_index as u64 + 1,
                );
                // Ordered callbacks consume only their named bones. Complete render
                // palettes belong to the owned draw phase, including offscreen shadows;
                // an already prepared root palette can serve both consumers directly.
                let batch_hit = needs_palette
                    && self.pose_batch.take(
                        placement_index,
                        &source.model,
                        clock,
                        model_view,
                        overrides,
                        &mut self.bone_pose_scratch,
                    )?;
                let batch_hit = batch_hit
                    || self.late_pose.take(
                        placement_index,
                        &source.model,
                        clock,
                        model_view,
                        overrides,
                        &mut self.bone_pose_scratch,
                    )?;
                if !batch_hit {
                    self.bone_demand.model(
                        &admission.storage,
                        super::super::demand::CpuModelInputs {
                            placement,
                            model: &source.model,
                            window: event_window,
                            items: &self.requested_items,
                            visuals: &self.requested_visuals,
                            glue_ids: &self.glue_attachment_ids,
                            effects: &self.unit_effects,
                            publishes_lights,
                        },
                    )?;
                }
                let samples_hit = !batch_hit
                    && self.pose_batch.take_samples(
                        placement_index,
                        &source.model,
                        clock,
                        model_view,
                        overrides,
                        self.bone_demand.bones(),
                        &mut self.bone_samples_scratch,
                    )?;
                let samples_hit = samples_hit
                    || (!batch_hit
                        && self.late_pose.take_samples(
                            placement_index,
                            &source.model,
                            clock,
                            model_view,
                            overrides,
                            self.bone_demand.bones(),
                            &mut self.bone_samples_scratch,
                        )?);
                if !batch_hit && !samples_hit && !self.bone_demand.bones().is_empty() {
                    self.late_pose.start(
                        cpu,
                        placement_index,
                        source,
                        clock,
                        model_view,
                        overrides,
                        self.bone_demand.bones(),
                        needs_palette,
                    )?;
                    admission.pending = Some((selected, observed.pause()));
                    return Ok(false);
                }
                observed.batch_hit = batch_hit || samples_hit;
                let deferred_palette = needs_palette && !batch_hit;
                let unrequested =
                    super::super::demand::UnrequestedBones(source.model.animations().bones().len());
                if solarity_profiling::TraceContext::capture().is_sampled() {
                    solarity_profiling::TraceContext::capture().value(
                        "m2.cpu_bone_demand",
                        placement_index as u64 + 1,
                        u64::from(deferred_palette),
                        self.bone_demand.bones().len() as u64,
                    );
                }
                let bone_pose: &dyn solarity_rendering::M2BoneTransforms = if batch_hit {
                    &self.bone_pose_scratch
                } else if samples_hit {
                    &self.bone_samples_scratch
                } else {
                    debug_assert!(self.bone_demand.bones().is_empty());
                    &unrequested
                };
                placement_profile.mark("pose");
                let first_cpu_output = self.triggered_events.len()
                    + self.rider_transforms.len()
                    + self.item_transforms.len()
                    + self.visual_transforms.len()
                    + self.glue_attachment_transforms.len();
                super::super::publication::CpuPublication {
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
                    super::super::publication::CpuSample {
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
                        cpu.storage(),
                        placement_index,
                        self.placement_visibility.light_parent(placement_index),
                        placement.transform.w_axis.truncate(),
                        doodad_scene_active.then_some(placement_fog_color),
                    )?)
                } else {
                    None
                };
                // Both shadow banks consume the same frozen material clock/color.
                // They are independent of callbacks and camera-visible effects.
                let effect_retiring = placement
                    .unit_effect
                    .as_ref()
                    .is_some_and(|effect| effect.retiring());
                let mut model_color = placement_mesh_color(placement.owner, placement.color);
                if let Some(animation) = &placement.unit_animation {
                    model_color *= placement_color(animation.model_color().to_le_bytes());
                } else if let Some(pose) = placement
                    .retirement
                    .as_ref()
                    .and_then(|retired| retired.unit_pose)
                {
                    model_color *= placement_color(pose.color.to_le_bytes());
                }
                let mut shadow_color = model_color;
                shadow_color.w *= shadow_opacity;
                let mut input = super::super::geometry::GeometryInput {
                    placement_index,
                    trace: solarity_profiling::TraceContext::capture(),
                    source_index: placement.source_index,
                    clock,
                    transform: placement.transform,
                    model_view,
                    shadow: shadow::ShadowInput {
                        transform: placement.transform,
                        model_view,
                        clock,
                        instance_color: shadow_color,
                        retiring: effect_retiring,
                        bone_offset: 0,
                    },
                    primary_shadow: shadow_admitted,
                    environment_maps,
                    visible: None,
                };
                if !visible {
                    if shadow_admitted || environment_maps != 0 {
                        observed.geometry_pending = true;
                        self.geometry_batch.queue(
                            input,
                            &mut self.placements[placement_index],
                            &mut self.bone_pose_scratch,
                            deferred_palette.then_some(overrides),
                            source,
                            camera,
                            effect_scale,
                            &self.particle_twinkle,
                        )?;
                    }
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
                    liquid_clipping_enabled,
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

                let light_bank = placement_light_bank(placement.owner);
                let mut instance_color = model_color;
                instance_color.w *= placement_opacity;
                input.visible = Some(super::super::geometry::VisibleGeometryInput {
                    effect_delta_seconds,
                    instance_color,
                    placement_fog_color,
                    light_bank,
                    scene_index,
                    effect_retiring,
                    particle_liquid,
                    model_liquid,
                    instance_distance,
                    instance_identity,
                    model_distance_sort: self
                        .placement_visibility
                        .model_distance_sort(placement_index),
                    particle_colors: placement.particle_colors,
                });
                observed.geometry_pending = true;
                self.geometry_batch.queue(
                    input,
                    &mut self.placements[placement_index],
                    &mut self.bone_pose_scratch,
                    deferred_palette.then_some(overrides),
                    source,
                    camera,
                    effect_scale,
                    &self.particle_twinkle,
                )?;
            }
            Ok(true)
        })()?;
        if complete {
            self.close_geometry()?;
            self.late_pose.release_model();
            admission.complete = true;
        }
        Ok(())
    }
}
