//! Stock character creation/selection preparation with explicit primary-source ownership.

use super::super::{
    OptionalTextureBinding, ResidentGlueCharacterKey, ResidentGlueCharacterModel,
    RuntimePlayerError, RuntimePlayerPresentation, load_optional_texture, load_player_attachments,
    prepare_model_textures, resolve_creation_equipment, resolve_resident_animation,
    resolve_selection_equipment, resolve_selection_quiver,
};
use super::source::GlueModelSource;
use solarity_asset::{CharacterCustomization, InventoryType};
use solarity_ecs::{PlayerEquipmentSlot, UnitAnimationTier, UnitSheathState};
use solarity_rendering::{
    CharacterAttachmentPlan, CharacterGeosetContext, CharacterGeosetPlan, CharacterTabardMode,
    CharacterTexturePlan, CharacterWeaponState, M2ParticleColorReplacement,
};
use solarity_systems::UnitLocomotionAnimation;
use solarity_ui::{UiCharacterCreationPreview, UiCharacterSelectionPreview};

impl RuntimePlayerPresentation {
    /// Synchronizes the unequipped character-creation body from Glue choices.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimePlayerError`] when DBC joins, M2 loading, component
    /// texture composition, or animation selection fails.
    pub fn synchronize_character_creation(
        &mut self,
        preview: Option<&UiCharacterCreationPreview>,
    ) -> Result<bool, RuntimePlayerError> {
        self.synchronize_character_creation_source(preview, GlueModelSource::LocalCache)
    }

    /// The caller selects source ownership explicitly; shared jobs supply their admitted lease.
    pub(in crate::application::player_coordinator) fn synchronize_character_creation_source(
        &mut self,
        preview: Option<&UiCharacterCreationPreview>,
        source: GlueModelSource,
    ) -> Result<bool, RuntimePlayerError> {
        let Some(preview) = preview else {
            return Ok(self.glue_character.take().is_some());
        };
        if !preview.facing_degrees().is_finite() {
            return Err(RuntimePlayerError::InvalidCreationFacing {
                facing_degrees: preview.facing_degrees(),
            });
        }
        if let Some(resident) = self.glue_character.as_mut()
            && resident.key.matches_creation(preview)
        {
            // Wow.exe's SetCharacterCreateFacing path mutates the registered
            // model transform; it does not recreate appearance GPU resources.
            resident.key = ResidentGlueCharacterKey::Creation(preview.clone());
            resident.facing_radians = preview.facing_degrees().to_radians() as f32;
            return Ok(false);
        }
        let race = self.races.race(u32::from(preview.race_id())).ok_or(
            RuntimePlayerError::MissingCharacterRace {
                race_id: u32::from(preview.race_id()),
            },
        )?;
        let display_id = match preview.gender_id() {
            0 => race.male_display_id(),
            1 => race.female_display_id(),
            gender_id => {
                return Err(RuntimePlayerError::InvalidCreationGender { gender_id });
            }
        };
        let body = self.creatures.resolve_model(display_id)?;
        let body_particle_color_id = body.display().particle_color_id();
        let [skin, face, hair_style, hair_color, facial_hair] = preview.appearance();
        let customization = solarity_asset::CharacterCustomization::new(
            skin,
            face,
            hair_style,
            hair_color,
            facial_hair,
        );
        let appearance = self.characters.resolve_player(
            u32::from(preview.race_id()),
            u32::from(preview.gender_id()),
            customization,
        )?;
        let mut equipment_items = resolve_creation_equipment(
            preview,
            &self.start_outfits,
            &self.item_definitions,
            &self.item_displays,
        )?;
        // CCharacterCreation presents starter clothing with showHelmet false.
        // Excluding head inventory here keeps both the helmet child model and
        // its hair/ear visibility masks out of the creation representation.
        equipment_items.retain(|item| item.inventory_type() != Some(InventoryType::Head));
        let texture_plan = CharacterTexturePlan::equipped(
            &appearance,
            &self.assets.borrow(),
            equipment_items.iter().copied(),
        )?;
        let geosets = CharacterGeosetPlan::equipped(
            &appearance,
            CharacterGeosetContext::new(preview.class_id(), CharacterTabardMode::Equipment),
            &self.helmet_visibility,
            equipment_items.iter().copied(),
        )?;
        let attachment_plan = CharacterAttachmentPlan::equipped_items(
            equipment_items.iter().copied(),
            race,
            u32::from(preview.gender_id()),
            CharacterWeaponState::new(UnitSheathState::Unarmed),
        )?;
        let mut assets = self.assets.borrow_mut();
        let model = source.load(&mut self.models, &mut assets, body.model_path())?;
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
        drop(assets);
        let animation = resolve_resident_animation(
            &self.animations,
            &model,
            UnitLocomotionAnimation::STAND,
            UnitAnimationTier::Ground,
        )?;
        self.glue_character = Some(ResidentGlueCharacterModel {
            key: ResidentGlueCharacterKey::Creation(preview.clone()),
            model,
            textures,
            atlas,
            texture_plan,
            geosets,
            attachment_plan,
            animation,
            facing_radians: preview.facing_degrees().to_radians() as f32,
            hair,
            extra_skin,
            attachments,
            pet: None,
            particle_colors: M2ParticleColorReplacement::resolve(
                &self.particle_colors,
                body_particle_color_id,
            ),
        });
        Ok(true)
    }

    /// Synchronizes the selected roster character and its enum-time equipment.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimePlayerError`] when the roster fields cannot be joined
    /// to the pinned client DBCs or their model resources cannot be prepared.
    pub fn synchronize_character_selection(
        &mut self,
        preview: Option<&UiCharacterSelectionPreview>,
    ) -> Result<bool, RuntimePlayerError> {
        self.synchronize_character_selection_source(preview, GlueModelSource::LocalCache)
    }

    /// The caller selects source ownership explicitly; shared jobs supply their admitted lease.
    pub(in crate::application::player_coordinator) fn synchronize_character_selection_source(
        &mut self,
        preview: Option<&UiCharacterSelectionPreview>,
        source: GlueModelSource,
    ) -> Result<bool, RuntimePlayerError> {
        let Some(preview) = preview else {
            return Ok(self.glue_character.take().is_some());
        };
        if !preview.facing_degrees().is_finite() {
            return Err(RuntimePlayerError::InvalidSelectionFacing {
                facing_degrees: preview.facing_degrees(),
            });
        }
        if let Some(resident) = self.glue_character.as_mut()
            && resident.key.matches_selection(preview)
        {
            // Wow.exe 0x004E3030 stores the narrowed facing directly on the
            // selected model, leaving its body and equipment residency intact.
            resident.key = ResidentGlueCharacterKey::Selection(Box::new(preview.clone()));
            resident.facing_radians = preview.facing_degrees().to_radians() as f32;
            return Ok(false);
        }
        let race = self.races.race(u32::from(preview.race_id())).ok_or(
            RuntimePlayerError::MissingCharacterRace {
                race_id: u32::from(preview.race_id()),
            },
        )?;
        let display_id = match preview.gender_id() {
            0 => race.male_display_id(),
            1 => race.female_display_id(),
            gender_id => {
                return Err(RuntimePlayerError::InvalidCreationGender { gender_id });
            }
        };
        let body = self.creatures.resolve_model(display_id)?;
        let body_particle_color_id = body.display().particle_color_id();
        let [skin, face, hair_style, hair_color, facial_hair] = preview.appearance();
        let customization =
            CharacterCustomization::new(skin, face, hair_style, hair_color, facial_hair);
        let appearance = self.characters.resolve_player(
            u32::from(preview.race_id()),
            u32::from(preview.gender_id()),
            customization,
        )?;
        let mut equipment_items = resolve_selection_equipment(preview, &self.item_displays)?;
        equipment_items.retain(|item| {
            (preview.show_helmet() || item.slot() != PlayerEquipmentSlot::Head)
                && (preview.show_cloak() || item.slot() != PlayerEquipmentSlot::Back)
        });
        let texture_plan = CharacterTexturePlan::equipped(
            &appearance,
            &self.assets.borrow(),
            equipment_items.iter().copied(),
        )?;
        let geosets = CharacterGeosetPlan::equipped(
            &appearance,
            CharacterGeosetContext::new(preview.class_id(), CharacterTabardMode::Equipment),
            &self.helmet_visibility,
            equipment_items.iter().copied(),
        )?;
        let attachment_plan = CharacterAttachmentPlan::character_selection(
            equipment_items.iter().copied(),
            resolve_selection_quiver(preview, &self.item_displays)?,
            race,
            u32::from(preview.gender_id()),
            preview.class_id(),
        )?;
        let mut assets = self.assets.borrow_mut();
        let model = source.load(&mut self.models, &mut assets, body.model_path())?;
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
        drop(assets);
        let pet = self.load_glue_pet(preview.pet())?;
        let animation = resolve_resident_animation(
            &self.animations,
            &model,
            UnitLocomotionAnimation::STAND,
            UnitAnimationTier::Ground,
        )?;
        self.glue_character = Some(ResidentGlueCharacterModel {
            key: ResidentGlueCharacterKey::Selection(Box::new(preview.clone())),
            model,
            textures,
            atlas,
            texture_plan,
            geosets,
            attachment_plan,
            animation,
            facing_radians: preview.facing_degrees().to_radians() as f32,
            hair,
            extra_skin,
            attachments,
            pet,
            particle_colors: M2ParticleColorReplacement::resolve(
                &self.particle_colors,
                body_particle_color_id,
            ),
        });
        Ok(true)
    }
}
