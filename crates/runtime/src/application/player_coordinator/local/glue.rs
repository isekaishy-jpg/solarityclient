//! Exact selection-to-world transfer keeps existing decoded resources resident.

use super::super::{
    PlayerAppearanceInputs, ResidentGlueCharacterModel, ResidentPlayerModel, RuntimePlayerError,
    RuntimePlayerPresentation, UnitPresentationGeneration, resolve_resident_animation,
};
use super::LocalAppearance;
use solarity_ecs::PlayerViewState;
use solarity_systems::{PlayerCameraHeightState, resolve_model_camera_subject_height};

impl RuntimePlayerPresentation {
    /// Only an exact selection match transfers the existing decoded resources.
    pub(super) fn take_local_glue(
        &mut self,
        appearance: &LocalAppearance,
        inputs: PlayerAppearanceInputs,
    ) -> Result<Option<ResidentPlayerModel>, RuntimePlayerError> {
        let matching = appearance
            .glue_plans
            .as_ref()
            .is_some_and(|(texture, geosets)| {
                self.glue_character.as_ref().is_some_and(|resident| {
                    resident.texture_plan == *texture
                        && resident.geosets == *geosets
                        && resident.attachment_plan == appearance.desired.attachment_plan
                })
            });
        if !matching {
            return Ok(None);
        }
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
        let desired = &appearance.desired;
        let camera_height = resolve_model_camera_subject_height(&model, desired.object_scale)?;
        let animation = resolve_resident_animation(
            &self.animations,
            &model,
            desired.requested_animation,
            desired.animation_tier,
        )?;
        tracing::info!(
            guid = desired.guid,
            "transferred character-selection representation into active world"
        );
        Ok(Some(ResidentPlayerModel {
            appearance_inputs: Some(inputs),
            generation: UnitPresentationGeneration::new(),
            identity: desired.identity,
            guid: desired.guid,
            object_scale: desired.object_scale,
            particle_color_id: desired.particle_color_id,
            particle_colors,
            base_texture_plan: desired.base_texture_plan.clone(),
            base_geosets: desired.base_geosets.clone(),
            equipment_key: desired.equipment_key.clone(),
            attachment_plan,
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
            mount_key: None,
            mount: None,
        }))
    }
}
