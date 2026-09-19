//! Glue's synchronous callers and admitted workers select source ownership explicitly.

use super::super::{ResidentGlueCharacterKey, RuntimePlayerError, RuntimePlayerPresentation};
use solarity_asset::{AssetPath, AssetStore, DecodedM2Model, M2ModelCache, ResourceLease};

/// Existing synchronous consumers own their cache; live workers consume a shared lease.
pub(in crate::application::player_coordinator) enum GlueModelSource {
    LocalCache,
    Shared(ResourceLease<DecodedM2Model>),
}

impl GlueModelSource {
    /// Supplies the primary model at the original residency operation's load boundary.
    pub(super) fn load(
        self,
        cache: &mut M2ModelCache,
        assets: &mut AssetStore,
        path: &AssetPath,
    ) -> Result<ResourceLease<DecodedM2Model>, RuntimePlayerError> {
        match self {
            Self::LocalCache => Ok(cache.load(assets, path)?),
            Self::Shared(model) => Ok(model),
        }
    }
}

impl RuntimePlayerPresentation {
    /// Resolves the same facing/race/gender/body prefix used by synchronous Glue.
    /// This reads resident catalogs only; no archive decoding happens on main.
    pub(super) fn glue_primary_path(
        &self,
        key: &ResidentGlueCharacterKey,
    ) -> Result<AssetPath, RuntimePlayerError> {
        let (race, gender) = match key {
            ResidentGlueCharacterKey::Creation(preview) => {
                if !preview.facing_degrees().is_finite() {
                    return Err(RuntimePlayerError::InvalidCreationFacing {
                        facing_degrees: preview.facing_degrees(),
                    });
                }
                (preview.race_id(), preview.gender_id())
            }
            ResidentGlueCharacterKey::Selection(preview) => {
                if !preview.facing_degrees().is_finite() {
                    return Err(RuntimePlayerError::InvalidSelectionFacing {
                        facing_degrees: preview.facing_degrees(),
                    });
                }
                (preview.race_id(), preview.gender_id())
            }
        };
        let race =
            self.races
                .race(u32::from(race))
                .ok_or(RuntimePlayerError::MissingCharacterRace {
                    race_id: u32::from(race),
                })?;
        let display = match gender {
            0 => race.male_display_id(),
            1 => race.female_display_id(),
            gender_id => return Err(RuntimePlayerError::InvalidCreationGender { gender_id }),
        };
        Ok(self.creatures.resolve_model(display)?.model_path().clone())
    }
}
