//! Main captures local-only scaling and table joins without loading model resources.

use super::super::{
    DesiredPlayerModel, ResidentGlueCharacterKey, RuntimePlayerError, RuntimePlayerPresentation,
    mount_model_key,
};
use super::LocalAppearance;
use solarity_asset::M2ModelCache;
use solarity_ecs::ActiveWorld;
use solarity_rendering::{
    CharacterAttachmentPlan, CharacterEquipmentItem, CharacterGeosetContext, CharacterGeosetPlan,
    CharacterTabardMode, CharacterTexturePlan, CharacterWeaponState,
};
use solarity_systems::{
    UnitLocomotionAnimation, UnitModelAppearanceError, resolve_player_equipment,
    resolve_unit_locomotion_animation, resolve_unit_model,
};

impl RuntimePlayerPresentation {
    /// Retains the local collision clamp and missing-field/error behavior.
    pub(super) fn resolve_local_appearance(
        &self,
        world: &ActiveWorld,
    ) -> Result<Option<LocalAppearance>, RuntimePlayerError> {
        let guid = world.local_player_guid()?;
        let Some(identity) = world.object_identity(guid) else {
            return Ok(None);
        };
        let appearance = match resolve_unit_model(world, guid, &self.creatures, &self.characters) {
            Ok(appearance) => appearance,
            Err(
                UnitModelAppearanceError::MissingObjectKind { .. }
                | UnitModelAppearanceError::MissingObjectPresentation { .. }
                | UnitModelAppearanceError::MissingUnitPresentation { .. }
                | UnitModelAppearanceError::MissingUnitIdentity { .. }
                | UnitModelAppearanceError::MissingPlayerAppearance { .. },
            ) => return Ok(None),
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
            return Ok(None);
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
            return Ok(None);
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
        let requested_animation = world.movement_state(guid).map_or(
            UnitLocomotionAnimation::STAND,
            resolve_unit_locomotion_animation,
        );
        let desired = DesiredPlayerModel {
            identity,
            guid,
            object_scale: scale,
            collision_extent,
            particle_color_id,
            path,
            base_texture_plan,
            base_geosets,
            equipment_key,
            attachment_plan,
            world_transform: world.local_player_transform()?,
            requested_animation,
            animation_tier: unit_presentation.animation_tier(),
            mount_key,
        };
        // Ordinary appearance changes do no atlas/path planning on main. The
        // existing Glue transfer needs exact plan equality before moving owners.
        let glue_plans = if desired.mount_key.is_none()
            && self.glue_character.as_ref().is_some_and(|resident| {
                matches!(&resident.key, ResidentGlueCharacterKey::Selection(preview)
                    if preview.guid() == guid)
                    && resident.model.path() == &desired.path
            }) {
            Some((
                CharacterTexturePlan::equipped(
                    character,
                    &self.assets.borrow(),
                    equipment_items.iter().copied(),
                )?,
                CharacterGeosetPlan::equipped(
                    character,
                    CharacterGeosetContext::new(class_id, CharacterTabardMode::Equipment),
                    &self.helmet_visibility,
                    equipment_items.iter().copied(),
                )?,
            ))
        } else {
            None
        };
        Ok(Some(LocalAppearance {
            desired,
            glue_plans,
        }))
    }
}
