//! Shared owned character construction for local and remote player consumers.

use super::super::{
    CharacterCustomization, CharacterEquipmentItem, CharacterGeosetContext, CharacterGeosetPlan,
    CharacterTabardMode, CharacterTexturePlan, DesiredPlayerModel, M2ParticleColorReplacement,
    OptionalTextureBinding, PlayerAppearanceInputs, PlayerCameraHeightState,
    PlayerEquipmentAppearanceError, PlayerViewState, ResidentPlayerModel, RuntimePlayerError,
    RuntimePlayerPresentation, UnitLocomotionAnimation, UnitModelAppearanceError,
    UnitPresentationGeneration, load_mount_model, load_optional_texture, load_player_attachments,
    prepare_model_textures, resolve_model_camera_subject_height, resolve_resident_animation,
};

impl RuntimePlayerPresentation {
    /// Composes textures and attachments after the shared primary source is ready.
    pub(in crate::application::player_coordinator) fn prepare_player(
        &mut self,
        desired: DesiredPlayerModel,
        inputs: PlayerAppearanceInputs,
        model: solarity_asset::ResourceLease<solarity_asset::DecodedM2Model>,
    ) -> Result<ResidentPlayerModel, RuntimePlayerError> {
        let _profile = solarity_profiling::profile!("player.appearance.prepare");
        let appearance = inputs.appearance;
        let character = self.characters.resolve_player(
            u32::from(inputs.unit.race_id()),
            u32::from(inputs.unit.gender_id()),
            CharacterCustomization::new(
                appearance.skin_id(),
                appearance.face_id(),
                appearance.hair_style_id(),
                appearance.hair_color_id(),
                appearance.facial_hair_style_id(),
            ),
        )?;
        let class_id = inputs.unit.class_id();
        let mut equipment_items = Vec::new();
        for slot in solarity_ecs::PlayerEquipmentSlot::ALL {
            let visible = inputs.equipment.item(slot);
            if visible.entry_id() == 0 {
                continue;
            }
            let definition = self.item_definitions.item(visible.entry_id()).ok_or(
                PlayerEquipmentAppearanceError::MissingItem {
                    slot,
                    entry_id: visible.entry_id(),
                },
            )?;
            let display = self
                .item_displays
                .display(definition.display_info_id())
                .ok_or(PlayerEquipmentAppearanceError::MissingDisplay {
                    slot,
                    entry_id: visible.entry_id(),
                    display_id: definition.display_info_id(),
                })?;
            equipment_items.push(CharacterEquipmentItem::new_visible(
                slot, visible, definition, display,
            ));
        }
        let mount_appearance = (inputs.displays[2] != 0)
            .then(|| self.creatures.resolve_model(inputs.displays[2]))
            .transpose()
            .map_err(UnitModelAppearanceError::from)?;
        let mut assets = self.assets.borrow_mut();
        let texture_plan =
            CharacterTexturePlan::equipped(&character, &assets, equipment_items.iter().copied())?;
        let geosets = CharacterGeosetPlan::equipped(
            &character,
            CharacterGeosetContext::new(class_id, CharacterTabardMode::Equipment),
            &self.helmet_visibility,
            equipment_items.iter().copied(),
        )?;
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
            &desired.attachment_plan,
            &self.item_visuals,
            &self.particle_colors,
            &mut self.models,
            &mut self.textures,
            &mut assets,
        )?;
        let mount = load_mount_model(
            mount_appearance.as_ref(),
            desired.mount_key.as_ref(),
            desired.requested_animation,
            desired.animation_tier,
            &self.animations,
            &self.particle_colors,
            &mut self.models,
            &mut self.textures,
            &mut assets,
        )?;
        drop(assets);
        let camera_height = resolve_model_camera_subject_height(&model, desired.object_scale)?;
        let particle_colors =
            M2ParticleColorReplacement::resolve(&self.particle_colors, desired.particle_color_id);
        let animation = resolve_resident_animation(
            &self.animations,
            &model,
            if mount.is_some() {
                UnitLocomotionAnimation::MOUNT
            } else {
                desired.requested_animation
            },
            desired.animation_tier,
        )?;
        let generation = UnitPresentationGeneration::prepare(&model, &attachments, mount.as_ref())?;
        Ok(ResidentPlayerModel {
            appearance_inputs: Some(inputs),
            generation,
            identity: desired.identity,
            guid: desired.guid,
            object_scale: desired.object_scale,
            particle_color_id: desired.particle_color_id,
            particle_colors,
            base_texture_plan: desired.base_texture_plan,
            base_geosets: desired.base_geosets,
            equipment_key: desired.equipment_key,
            attachment_plan: desired.attachment_plan,
            texture_plan,
            geosets,
            atlas,
            hair,
            extra_skin,
            textures,
            attachments,
            world_transform: desired.world_transform,
            view: PlayerViewState::STOCK_VIEW_2,
            animation,
            camera_height,
            camera_height_state: PlayerCameraHeightState::new(camera_height),
            camera_time_ms: 0.0,
            camera_pose: None,
            model,
            mount_key: desired.mount_key,
            mount,
        })
    }
}
