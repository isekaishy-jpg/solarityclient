//! Local appearance admission and independent motion publication.

mod pose;

use super::{
    OptionalTextureBinding, PlayerAppearanceInputs, ResidentGlueCharacterKey,
    ResidentGlueCharacterModel, ResidentPlayerModel, RuntimePlayerError, RuntimePlayerPoll,
    RuntimePlayerPresentation, UnitPresentationGeneration, load_mount_model, load_optional_texture,
    load_player_attachments, mount_model_key, prepare_model_textures, resolve_resident_animation,
};
use solarity_asset::M2ModelCache;
use solarity_ecs::ActiveWorld;
use solarity_rendering::{
    CharacterAttachmentPlan, CharacterEquipmentItem, CharacterGeosetContext, CharacterGeosetPlan,
    CharacterTabardMode, CharacterTexturePlan, CharacterWeaponState, M2ParticleColorReplacement,
};
use solarity_systems::{
    PlayerCameraHeightState, UnitLocomotionAnimation, UnitModelAppearanceError,
    resolve_model_camera_subject_height, resolve_mounted_player_camera_pose,
    resolve_player_equipment, resolve_unit_locomotion_animation, resolve_unit_model,
};

impl RuntimePlayerPresentation {
    /// Synchronizes the local player's exact body M2 and stable camera height.
    ///
    /// Display and customization joins are resolved from projected ECS fields.
    /// The model cache uses the ordinary archive-selected path, so a same-name
    /// HD model remains the same logical residency key with larger source data.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimePlayerError`] when authoritative ECS state, DBC joins,
    /// M2/SKIN loading, or camera-height arithmetic is invalid.
    pub fn synchronize(
        &mut self,
        world: Option<&ActiveWorld>,
    ) -> Result<RuntimePlayerPoll, RuntimePlayerError> {
        let Some(world) = world else {
            self.resident = None;
            self.unit_animations.clear();
            self.textures.collect_unused();
            return Ok(RuntimePlayerPoll::Idle);
        };
        self.unit_animations.synchronize_passenger_inputs(
            world,
            &self.vehicles,
            &self.passenger_frames,
        );
        let guid = world.local_player_guid()?;
        let Some(identity) = world.object_identity(guid) else {
            return Ok(RuntimePlayerPoll::Pending);
        };
        let inputs = PlayerAppearanceInputs::read(world, guid);
        // Compare projected inputs before allocating equipment, texture or geoset plans.
        // Motion, native animation callbacks and camera feedback remain independent.
        if inputs.is_some()
            && self
                .resident
                .as_ref()
                .is_some_and(|resident| resident.appearance_inputs == inputs)
        {
            return self.update_local_pose(world);
        }
        let appearance = match resolve_unit_model(world, guid, &self.creatures, &self.characters) {
            Ok(appearance) => appearance,
            Err(
                UnitModelAppearanceError::MissingObjectKind { .. }
                | UnitModelAppearanceError::MissingObjectPresentation { .. }
                | UnitModelAppearanceError::MissingUnitPresentation { .. }
                | UnitModelAppearanceError::MissingUnitIdentity { .. }
                | UnitModelAppearanceError::MissingPlayerAppearance { .. },
            ) => return Ok(RuntimePlayerPoll::Pending),
            Err(error) => return Err(error.into()),
        };
        let path = M2ModelCache::canonical_path(appearance.body().model_path())?;
        let Some(body_scale) = solarity_systems::resolve_unit_body_scale(
            world,
            guid,
            &self.creatures,
            &self.races,
            None,
        ) else {
            return Ok(RuntimePlayerPoll::Pending);
        };
        // 73FCC0 installs the same authored body multiplier for players and
        // creatures; 71C0E0 combines it with the independent instance scale.
        let scale = body_scale * appearance.object_scale();
        let body_model = appearance.body().model();
        let body_display = appearance.body().display();
        let authored_scale = body_display.model_scale() * body_model.model_scale();
        // CGUnit_C multiplies the authored display/model scale by the live
        // OBJECT_FIELD_SCALE_X before publishing its collision dimensions.
        let collision_scale = authored_scale * appearance.object_scale().max(1.0);
        let collision_extent = body_model
            .collision_extent()
            .map(|extent| extent * collision_scale);
        let particle_color_id = appearance.body().display().particle_color_id();
        let mount_key = appearance
            .mount()
            .map(|mount| mount_model_key(mount, body_scale, appearance.object_scale()));
        let character = appearance
            .character()
            .ok_or(RuntimePlayerError::MissingCharacterAppearance { guid })?;
        let class_id = appearance
            .player_class_id()
            .ok_or(RuntimePlayerError::MissingCharacterAppearance { guid })?;
        let Some(unit_presentation) = world.local_player_presentation() else {
            return Ok(RuntimePlayerPoll::Pending);
        };
        let equipment =
            resolve_player_equipment(world, guid, &self.item_definitions, &self.item_displays)?;
        let equipment_items = equipment
            .items()
            .iter()
            .map(|item| {
                CharacterEquipmentItem::new_visible(
                    item.slot(),
                    item.visible(),
                    item.definition(),
                    item.display(),
                )
            })
            .collect::<Vec<_>>();
        let equipment_key = equipment
            .items()
            .iter()
            .map(|item| (item.slot(), item.visible()))
            .collect::<Vec<_>>();
        let race = self.races.race(character.race_id()).ok_or(
            RuntimePlayerError::MissingCharacterRace {
                race_id: character.race_id(),
            },
        )?;
        let attachment_plan = CharacterAttachmentPlan::equipped_items(
            equipment_items.iter().copied(),
            race,
            character.gender_id(),
            CharacterWeaponState::new(unit_presentation.sheath_state()),
        )?;
        let base_texture_plan = CharacterTexturePlan::base(character)?;
        let base_geosets = CharacterGeosetPlan::equipped(
            character,
            CharacterGeosetContext::new(class_id, CharacterTabardMode::Equipment),
            &self.helmet_visibility,
            std::iter::empty(),
        )?;
        if self.resident.as_ref().is_some_and(|resident| {
            resident.guid == guid
                && resident.identity == identity
                && resident.path() == &path
                && resident.object_scale == scale
                && resident.particle_color_id == particle_color_id
                && resident.base_texture_plan == base_texture_plan
                && resident.base_geosets == base_geosets
                && resident.equipment_key == equipment_key
                && resident.attachment_plan == attachment_plan
                && resident.mount_key == mount_key
        }) {
            if let Some(resident) = self.resident.as_mut() {
                resident.appearance_inputs = inputs;
            }
            return self.update_local_pose(world);
        }

        let requested_animation = world.movement_state(guid).map_or(
            UnitLocomotionAnimation::STAND,
            resolve_unit_locomotion_animation,
        );
        let texture_plan = CharacterTexturePlan::equipped(
            character,
            &self.assets.borrow(),
            equipment_items.iter().copied(),
        )?;
        let geosets = CharacterGeosetPlan::equipped(
            character,
            CharacterGeosetContext::new(class_id, CharacterTabardMode::Equipment),
            &self.helmet_visibility,
            equipment_items.iter().copied(),
        )?;
        let reusable_glue_character = mount_key.is_none()
            && self.glue_character.as_ref().is_some_and(|resident| {
                matches!(
                    &resident.key,
                    ResidentGlueCharacterKey::Selection(preview) if preview.guid() == guid
                ) && resident.model.path() == &path
                    && resident.texture_plan == texture_plan
                    && resident.geosets == geosets
                    && resident.attachment_plan == attachment_plan
            });
        if reusable_glue_character {
            let glue = self
                .glue_character
                .take()
                .ok_or(RuntimePlayerError::MissingGlueCharacterWorkerResult)?;
            let ResidentGlueCharacterModel {
                model,
                textures,
                atlas,
                texture_plan,
                geosets,
                attachment_plan,
                hair,
                extra_skin,
                attachments,
                particle_colors,
                ..
            } = glue;
            let base_camera_height = resolve_model_camera_subject_height(&model, scale)?;
            let previous_camera = self.resident.as_ref().and_then(|resident| {
                (resident.path() == &path && resident.object_scale == scale).then_some((
                    resident.camera_height_state,
                    resident.camera_time_ms,
                    resident.mount_key.clone(),
                ))
            });
            let (mut camera_height_state, camera_time_ms, previous_mount_key) = previous_camera
                .unwrap_or((PlayerCameraHeightState::new(base_camera_height), 0.0, None));
            if previous_mount_key.is_some() {
                camera_height_state.set_mounted(false, camera_time_ms)?;
            }
            let camera_heights = camera_height_state.sample(camera_time_ms)?;
            let camera_height = camera_heights.subject_height();
            let world_transform = world.local_player_transform()?;
            let view = world.local_player_view()?;
            let camera_pose =
                resolve_mounted_player_camera_pose(world_transform, view, camera_heights)?;
            let animation = resolve_resident_animation(
                &self.animations,
                &model,
                requested_animation,
                unit_presentation.animation_tier(),
            )?;
            self.resident = Some(ResidentPlayerModel {
                appearance_inputs: inputs,
                generation: UnitPresentationGeneration::new(),
                identity,
                guid,
                object_scale: scale,
                collision_extent,
                particle_color_id,
                particle_colors,
                base_texture_plan,
                base_geosets,
                equipment_key,
                attachment_plan,
                texture_plan,
                geosets,
                atlas,
                hair,
                extra_skin,
                textures,
                attachments,
                world_transform,
                view,
                animation,
                camera_height,
                camera_height_state,
                camera_time_ms,
                camera_pose: Some(camera_pose),
                model,
                mount_key: None,
                mount: None,
            });
            tracing::info!(
                guid,
                "transferred character-selection representation into active world"
            );
            self.synchronize_local_animation(world)?;
            self.textures.collect_unused();
            return Ok(RuntimePlayerPoll::ModelLoaded);
        }
        let mut assets = self.assets.borrow_mut();
        let model = self.models.load(&mut assets, &path)?;
        let atlas = texture_plan.compose_at_level(
            &mut assets,
            &mut self.textures,
            self.component_texture_level,
        )?;
        let hair = load_optional_texture(texture_plan.hair(), &mut assets, &mut self.textures)?;
        let extra_skin =
            load_optional_texture(texture_plan.extra_skin(), &mut assets, &mut self.textures)?;
        let cape = load_optional_texture(texture_plan.cape(), &mut assets, &mut self.textures)?;
        let textures = prepare_model_textures(
            &model,
            OptionalTextureBinding::new(texture_plan.hair(), hair.as_ref()),
            OptionalTextureBinding::new(texture_plan.extra_skin(), extra_skin.as_ref()),
            OptionalTextureBinding::new(texture_plan.cape(), cape.as_ref()),
            &mut assets,
            &mut self.textures,
        )?;
        let attachments = load_player_attachments(
            &attachment_plan,
            &self.item_visuals,
            &self.particle_colors,
            &mut self.models,
            &mut self.textures,
            &mut assets,
        )?;
        let mount = load_mount_model(
            appearance.mount(),
            mount_key.as_ref(),
            requested_animation,
            unit_presentation.animation_tier(),
            &self.animations,
            &self.particle_colors,
            &mut self.models,
            &mut self.textures,
            &mut assets,
        )?;
        drop(assets);
        let base_camera_height = resolve_model_camera_subject_height(&model, scale)?;
        let previous_camera = self.resident.as_ref().and_then(|resident| {
            (resident.path() == &path && resident.object_scale == scale).then_some((
                resident.camera_height_state,
                resident.camera_time_ms,
                resident.mount_key.clone(),
            ))
        });
        let (mut camera_height_state, camera_time_ms, previous_mount_key) = previous_camera
            .unwrap_or((PlayerCameraHeightState::new(base_camera_height), 0.0, None));
        if previous_mount_key != mount_key {
            if mount_key.is_some() {
                camera_height_state.begin_mount_generation(camera_time_ms)?;
            } else {
                camera_height_state.set_mounted(false, camera_time_ms)?;
            }
        }
        let camera_heights = camera_height_state.sample(camera_time_ms)?;
        let camera_height = camera_heights.subject_height();
        let world_transform = world.local_player_transform()?;
        let view = world.local_player_view()?;
        let camera_pose =
            resolve_mounted_player_camera_pose(world_transform, view, camera_heights)?;
        let particle_colors =
            M2ParticleColorReplacement::resolve(&self.particle_colors, particle_color_id);
        let animation = resolve_resident_animation(
            &self.animations,
            &model,
            if mount.is_some() {
                UnitLocomotionAnimation::MOUNT
            } else {
                requested_animation
            },
            unit_presentation.animation_tier(),
        )?;
        self.resident = Some(ResidentPlayerModel {
            appearance_inputs: inputs,
            generation: UnitPresentationGeneration::new(),
            identity,
            guid,
            object_scale: scale,
            collision_extent,
            particle_color_id,
            particle_colors,
            base_texture_plan,
            base_geosets,
            equipment_key,
            attachment_plan,
            texture_plan,
            geosets,
            atlas,
            hair,
            extra_skin,
            textures,
            attachments,
            world_transform,
            view,
            animation,
            camera_height,
            camera_height_state,
            camera_time_ms,
            camera_pose: Some(camera_pose),
            model,
            mount_key,
            mount,
        });
        self.textures.collect_unused();
        self.synchronize_local_animation(world)?;
        Ok(RuntimePlayerPoll::ModelLoaded)
    }
}
