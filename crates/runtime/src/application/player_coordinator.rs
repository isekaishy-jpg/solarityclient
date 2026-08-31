//! Local-player model residency and authored presentation measurements.

use std::sync::Arc;

use solarity_asset::{
    AssetError, AssetPath, AssetStoreHandle, BlpTextureCache, BlpTextureSource,
    CharacterAppearanceCatalog, CreatureCatalog, DecodedM2Model, M2ModelCache,
    ParticleColorCatalog,
};
use solarity_ecs::{ActiveWorld, WorldStateError};
use solarity_rendering::{
    CharacterAtlasTexture, CharacterTextureComposeError, CharacterTexturePlan,
    CharacterTexturePlanError, M2ParticleColorReplacement, WorldCamera,
};
use solarity_systems::{
    CameraSubjectHeight, CameraSubjectHeightError, PlayerCameraPose, PlayerCameraPoseError,
    UnitModelAppearanceError, resolve_model_camera_subject_height, resolve_player_camera_pose,
    resolve_unit_model,
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
    /// Authoritative movement and saved view state cannot form a finite orbit.
    #[error(transparent)]
    CameraPose(#[from] PlayerCameraPoseError),
    /// Resolved customization could not form stock's texture replacement plan.
    #[error(transparent)]
    CharacterTexturePlan(#[from] CharacterTexturePlanError),
    /// Planned character texture layers could not form the complete body atlas.
    #[error(transparent)]
    CharacterTextureCompose(#[from] CharacterTextureComposeError),
    /// The local player resolved without the character-only appearance join.
    #[error("local player {guid:#018X} has no character appearance")]
    MissingCharacterAppearance {
        /// Player GUID whose object kind promised character appearance.
        guid: u64,
    },
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
    particle_colors: ParticleColorCatalog,
    models: M2ModelCache,
    textures: BlpTextureCache,
    resident: Option<ResidentPlayerModel>,
}

impl RuntimePlayerPresentation {
    /// Creates an empty owner over process-wide assets and immutable DBC catalogs.
    #[must_use]
    pub fn new(
        assets: AssetStoreHandle,
        creatures: CreatureCatalog,
        characters: CharacterAppearanceCatalog,
        particle_colors: ParticleColorCatalog,
    ) -> Self {
        Self {
            assets,
            creatures,
            characters,
            particle_colors,
            models: M2ModelCache::new(),
            textures: BlpTextureCache::new(),
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
            self.textures.collect_unused();
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
        let particle_color_id = appearance.body().display().particle_color_id();
        let character = appearance
            .character()
            .ok_or(RuntimePlayerError::MissingCharacterAppearance { guid })?;
        let texture_plan = CharacterTexturePlan::base(character)?;
        if self.resident.as_ref().is_some_and(|resident| {
            resident.guid == guid
                && resident.path() == path
                && resident.object_scale == scale
                && resident.particle_color_id == particle_color_id
                && resident.texture_plan == texture_plan
        }) {
            let transform = world.local_player_transform()?;
            let view = world.local_player_view()?;
            if let Some(resident) = self.resident.as_mut() {
                resident.camera_pose =
                    resolve_player_camera_pose(transform, view, resident.camera_height)?;
            }
            return Ok(RuntimePlayerPoll::Current);
        }

        let mut assets = self.assets.borrow_mut();
        let model = self.models.load(&mut assets, path)?;
        let atlas = texture_plan.compose(&mut assets, &mut self.textures)?;
        let hair = load_optional_texture(texture_plan.hair(), &mut assets, &mut self.textures)?;
        let extra_skin =
            load_optional_texture(texture_plan.extra_skin(), &mut assets, &mut self.textures)?;
        drop(assets);
        let camera_height = resolve_model_camera_subject_height(&model, scale)?;
        let camera_pose = resolve_player_camera_pose(
            world.local_player_transform()?,
            world.local_player_view()?,
            camera_height,
        )?;
        let particle_colors =
            M2ParticleColorReplacement::resolve(&self.particle_colors, particle_color_id);
        self.resident = Some(ResidentPlayerModel {
            guid,
            object_scale: scale,
            particle_color_id,
            particle_colors,
            texture_plan,
            atlas,
            hair,
            extra_skin,
            camera_height,
            camera_pose,
            model,
        });
        self.models.collect_unused();
        self.textures.collect_unused();
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

    /// Returns the body display's exact `ParticleColor.dbc` identifier.
    #[must_use]
    pub fn resident_particle_color_id(&self) -> Option<u32> {
        self.resident
            .as_ref()
            .map(|resident| resident.particle_color_id)
    }

    /// Returns placement-local body-emitter colors when the display selected them.
    #[must_use]
    pub fn resident_particle_colors(&self) -> Option<&M2ParticleColorReplacement> {
        self.resident
            .as_ref()
            .and_then(|resident| resident.particle_colors.as_ref())
    }

    /// Returns the complete placement-owned body atlas mip chain.
    #[must_use]
    pub fn resident_body_atlas(&self) -> Option<&CharacterAtlasTexture> {
        self.resident.as_ref().map(|resident| &resident.atlas)
    }

    /// Returns the authored texture replacing the character hair slot.
    #[must_use]
    pub fn resident_hair_texture(&self) -> Option<&Arc<BlpTextureSource>> {
        self.resident
            .as_ref()
            .and_then(|resident| resident.hair.as_ref())
    }

    /// Returns the authored texture replacing the character extra-skin slot.
    #[must_use]
    pub fn resident_extra_skin_texture(&self) -> Option<&Arc<BlpTextureSource>> {
        self.resident
            .as_ref()
            .and_then(|resident| resident.extra_skin.as_ref())
    }

    /// Returns the authored and stock-clamped camera pivot height.
    #[must_use]
    pub fn camera_subject_height(&self) -> Option<CameraSubjectHeight> {
        self.resident
            .as_ref()
            .map(|resident| resident.camera_height)
    }

    /// Returns the current pre-collision camera orbit for the resident player.
    #[must_use]
    pub fn camera_pose(&self) -> Option<PlayerCameraPose> {
        self.resident.as_ref().map(|resident| resident.camera_pose)
    }

    /// Builds renderer-owned camera state after far-clip policy has resolved.
    ///
    /// This conversion intentionally requires the caller's final far clip;
    /// player presentation does not own map/CVar visibility policy.
    #[must_use]
    pub fn world_camera(&self, far_clip: f32) -> Option<WorldCamera> {
        self.camera_pose().map(|pose| {
            WorldCamera::stock_following(
                pose.eye(),
                pose.target(),
                pose.up(),
                pose.orbit_pivot(),
                pose.subject(),
                far_clip,
            )
        })
    }

    /// Releases local-player residency on world disconnect.
    pub fn disconnect(&mut self) {
        self.resident = None;
        self.models.collect_unused();
        self.textures.collect_unused();
    }
}

struct ResidentPlayerModel {
    guid: u64,
    object_scale: f32,
    particle_color_id: u32,
    particle_colors: Option<M2ParticleColorReplacement>,
    texture_plan: CharacterTexturePlan,
    atlas: CharacterAtlasTexture,
    hair: Option<Arc<BlpTextureSource>>,
    extra_skin: Option<Arc<BlpTextureSource>>,
    camera_height: CameraSubjectHeight,
    camera_pose: PlayerCameraPose,
    model: Arc<DecodedM2Model>,
}

/// Loads one optional character replacement without inventing a substitute.
fn load_optional_texture(
    path: Option<&AssetPath>,
    assets: &mut solarity_asset::AssetStore,
    textures: &mut BlpTextureCache,
) -> Result<Option<Arc<BlpTextureSource>>, AssetError> {
    path.map(|path| textures.load(assets, path)).transpose()
}

impl ResidentPlayerModel {
    fn path(&self) -> &AssetPath {
        self.model.path()
    }
}
