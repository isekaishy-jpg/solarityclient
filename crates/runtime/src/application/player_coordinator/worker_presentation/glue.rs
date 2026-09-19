//! A frozen selected appearance consumes its admitted primary source exactly once.

use super::super::glue_character::GlueModelSource;
use super::super::{
    ResidentGlueCharacterKey, ResidentGlueCharacterModel, RuntimePlayerError,
    RuntimePlayerPresentation,
};
use solarity_asset::{DecodedM2Model, ResourceLease};

/// The source gate already returned its shared lease; nested assets retain their existing owners.
pub(in crate::application::player_coordinator) fn prepare_glue_character(
    presentation: &mut RuntimePlayerPresentation,
    key: ResidentGlueCharacterKey,
    model: ResourceLease<DecodedM2Model>,
) -> Result<ResidentGlueCharacterModel, RuntimePlayerError> {
    presentation.requested_glue_character = Some(key.clone());
    match &key {
        ResidentGlueCharacterKey::Creation(preview) => {
            presentation.synchronize_character_creation_source(
                Some(preview),
                GlueModelSource::Shared(model),
            )?;
        }
        ResidentGlueCharacterKey::Selection(preview) => {
            presentation.synchronize_character_selection_source(
                Some(preview),
                GlueModelSource::Shared(model),
            )?;
        }
    }
    presentation
        .glue_character
        .take()
        .ok_or(RuntimePlayerError::MissingGlueCharacterWorkerResult)
}
