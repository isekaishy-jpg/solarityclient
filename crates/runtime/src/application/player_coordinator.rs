//! Local-player model residency and authored presentation measurements.

use std::sync::Arc;

use solarity_asset::{
    AssetError, AssetPath, AssetStoreHandle, CharacterAppearanceCatalog, CreatureCatalog,
    DecodedM2Model, M2ModelCache,
};
use solarity_ecs::{ActiveWorld, WorldStateError};
use solarity_systems::{
    CameraSubjectHeight, CameraSubjectHeightError, UnitModelAppearanceError,
    resolve_model_camera_subject_height, resolve_unit_model,
};
use thiserror::Error;

/// Failure while resolving the local player's authored presentation model.
#[derive(Debug, Error)]
pub enum RuntimePlayerError {
    /// The active ECS world lost a required player invariant.
    #[error(transparent)]
    World(#[from] WorldStateError),
    /// Projected unit state or a required DBC join is incomplete.
    #[error(transparent)]
    Appearance(#[from] UnitModelAppearanceError),
    /// The selected M2 or one of its SKIN companions failed strict loading.
    #[error(transparent)]
    Asset(#[from] AssetError),
    /// Authored M2 geometry cannot produce a finite stock camera height.
    #[error(transparent)]
    CameraHeight(#[from] CameraSubjectHeightError),
}

/// Observable result of one local-player presentation synchronization pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimePlayerPoll {
    /// No active world exists and no player model is resident.
    Idle,
    /// The player exists but has not received every create-time presentation field.
    Pending,
    /// A new body M2 and its authored camera height became resident.
    ModelLoaded,
    /// The already-resident model and object scale remain current.
    Current,
}

/// Resolves ECS appearance into a shared model without putting assets in ECS.
pub struct RuntimePlayerPresentation {
    assets: AssetStoreHandle,
    creatures: CreatureCatalog,
    characters: CharacterAppearanceCatalog,
    models: M2ModelCache,
    resident: Option<ResidentPlayerModel>,
}

impl RuntimePlayerPresentation {
    /// Creates an empty owner over process-wide assets and immutable DBC catalogs.
    #[must_use]
    pub fn new(
        assets: AssetStoreHandle,
        creatures: CreatureCatalog,
        characters: CharacterAppearanceCatalog,
    ) -> Self {
        Self {
            assets,
            creatures,
            characters,
            models: M2ModelCache::new(),
            resident: None,
        }
    }

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
            self.models.collect_unused();
            return Ok(RuntimePlayerPoll::Idle);
        };
        let guid = world.local_player_guid()?;
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
        let path = appearance.body().model_path();
        let scale = appearance.object_scale();
        if self
            .resident
            .as_ref()
            .is_some_and(|resident| resident.path() == path && resident.object_scale == scale)
        {
            return Ok(RuntimePlayerPoll::Current);
        }

        let model = self.models.load(&mut self.assets.borrow_mut(), path)?;
        let camera_height = resolve_model_camera_subject_height(&model, scale)?;
        self.resident = Some(ResidentPlayerModel {
            guid,
            object_scale: scale,
            camera_height,
            model,
        });
        self.models.collect_unused();
        Ok(RuntimePlayerPoll::ModelLoaded)
    }

    /// Returns the controlled player's exact server GUID when resident.
    #[must_use]
    pub fn resident_guid(&self) -> Option<u64> {
        self.resident.as_ref().map(|resident| resident.guid)
    }

    /// Returns the archive-selected player body M2 when resident.
    #[must_use]
    pub fn resident_model(&self) -> Option<&Arc<DecodedM2Model>> {
        self.resident.as_ref().map(|resident| &resident.model)
    }

    /// Returns the authored and stock-clamped camera pivot height.
    #[must_use]
    pub fn camera_subject_height(&self) -> Option<CameraSubjectHeight> {
        self.resident
            .as_ref()
            .map(|resident| resident.camera_height)
    }

    /// Releases local-player residency on world disconnect.
    pub fn disconnect(&mut self) {
        self.resident = None;
        self.models.collect_unused();
    }
}

struct ResidentPlayerModel {
    guid: u64,
    object_scale: f32,
    camera_height: CameraSubjectHeight,
    model: Arc<DecodedM2Model>,
}

impl ResidentPlayerModel {
    fn path(&self) -> &AssetPath {
        self.model.path()
    }
}
