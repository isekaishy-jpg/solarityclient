//! Local-player model residency and authored presentation measurements.

use std::sync::Arc;

use solarity_asset::{
    AssetError, AssetPath, AssetStoreHandle, BlpTextureCache, BlpTextureSource,
    CharacterAppearanceCatalog, CharacterRaceCatalog, CreatureCatalog, DecodedM2Model,
    HelmetGeosetVisibilityCatalog, ItemDefinitionCatalog, ItemDisplayCatalog, M2ModelCache,
    M2TextureKind, ParticleColorCatalog,
};
use solarity_ecs::{
    ActiveWorld, PlayerEquipmentSlot, VisibleEquipmentItem, WorldStateError, WorldTransform,
};
use solarity_rendering::{
    CharacterAtlasTexture, CharacterAttachmentPlan, CharacterAttachmentPlanError,
    CharacterAttachmentPoint, CharacterEquipmentItem, CharacterGeosetContext, CharacterGeosetPlan,
    CharacterGeosetPlanError, CharacterTabardMode, CharacterTextureComposeError,
    CharacterTexturePlan, CharacterTexturePlanError, CharacterWeaponState,
    M2ParticleColorReplacement, WorldCamera,
};
use solarity_systems::{
    CameraSubjectHeight, CameraSubjectHeightError, PlayerCameraPose, PlayerCameraPoseError,
    PlayerEquipmentAppearanceError, UnitModelAppearanceError, resolve_model_camera_subject_height,
    resolve_player_camera_pose, resolve_player_equipment, resolve_unit_model,
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
    /// Resolved customization could not form stock's body-geoset mask.
    #[error(transparent)]
    CharacterGeosetPlan(#[from] CharacterGeosetPlanError),
    /// Equipped child-model paths could not be formed from client tables.
    #[error(transparent)]
    CharacterAttachmentPlan(#[from] CharacterAttachmentPlanError),
    /// Public visible-item fields could not resolve through client item tables.
    #[error(transparent)]
    Equipment(#[from] PlayerEquipmentAppearanceError),
    /// A hardcoded model texture declaration omitted its required BLP path.
    #[error("player M2 {model} has a hardcoded texture without a filename")]
    MissingHardcodedTexturePath {
        /// Model containing the invalid declaration.
        model: AssetPath,
    },
    /// The local player resolved without the character-only appearance join.
    #[error("local player {guid:#018X} has no character appearance")]
    MissingCharacterAppearance {
        /// Player GUID whose object kind promised character appearance.
        guid: u64,
    },
    /// A resolved player appearance references no corresponding race row.
    #[error("character appearance references missing race {race_id}")]
    MissingCharacterRace {
        /// Absent `ChrRaces.dbc` identifier.
        race_id: u32,
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

/// Immutable client-table dependencies used to resolve player presentation.
pub struct RuntimePlayerCatalogs {
    creatures: CreatureCatalog,
    characters: CharacterAppearanceCatalog,
    races: CharacterRaceCatalog,
    helmet_visibility: HelmetGeosetVisibilityCatalog,
    item_definitions: ItemDefinitionCatalog,
    item_displays: ItemDisplayCatalog,
    particle_colors: ParticleColorCatalog,
}

impl RuntimePlayerCatalogs {
    /// Groups the exact stock tables consumed by player presentation.
    #[must_use]
    pub fn new(
        creatures: CreatureCatalog,
        characters: CharacterAppearanceCatalog,
        races: CharacterRaceCatalog,
        helmet_visibility: HelmetGeosetVisibilityCatalog,
        item_definitions: ItemDefinitionCatalog,
        item_displays: ItemDisplayCatalog,
        particle_colors: ParticleColorCatalog,
    ) -> Self {
        Self {
            creatures,
            characters,
            races,
            helmet_visibility,
            item_definitions,
            item_displays,
            particle_colors,
        }
    }
}

/// Resolves ECS appearance into a shared model without putting assets in ECS.
pub struct RuntimePlayerPresentation {
    assets: AssetStoreHandle,
    creatures: CreatureCatalog,
    characters: CharacterAppearanceCatalog,
    races: CharacterRaceCatalog,
    helmet_visibility: HelmetGeosetVisibilityCatalog,
    item_definitions: ItemDefinitionCatalog,
    item_displays: ItemDisplayCatalog,
    particle_colors: ParticleColorCatalog,
    models: M2ModelCache,
    textures: BlpTextureCache,
    resident: Option<ResidentPlayerModel>,
}

impl RuntimePlayerPresentation {
    /// Creates an empty owner over process-wide assets and immutable DBC catalogs.
    #[must_use]
    pub fn new(assets: AssetStoreHandle, catalogs: RuntimePlayerCatalogs) -> Self {
        Self {
            assets,
            creatures: catalogs.creatures,
            characters: catalogs.characters,
            races: catalogs.races,
            helmet_visibility: catalogs.helmet_visibility,
            item_definitions: catalogs.item_definitions,
            item_displays: catalogs.item_displays,
            particle_colors: catalogs.particle_colors,
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
            .map(|item| CharacterEquipmentItem::new(item.slot(), item.definition(), item.display()))
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
                && resident.path() == path
                && resident.object_scale == scale
                && resident.particle_color_id == particle_color_id
                && resident.base_texture_plan == base_texture_plan
                && resident.base_geosets == base_geosets
                && resident.equipment_key == equipment_key
                && resident.attachment_plan == attachment_plan
        }) {
            let transform = world.local_player_transform()?;
            let view = world.local_player_view()?;
            if let Some(resident) = self.resident.as_mut() {
                resident.world_transform = transform;
                resident.camera_pose =
                    resolve_player_camera_pose(transform, view, resident.camera_height)?;
            }
            return Ok(RuntimePlayerPoll::Current);
        }

        let mut assets = self.assets.borrow_mut();
        let model = self.models.load(&mut assets, path)?;
        let texture_plan =
            CharacterTexturePlan::equipped(character, &assets, equipment_items.iter().copied())?;
        let geosets = CharacterGeosetPlan::equipped(
            character,
            CharacterGeosetContext::new(class_id, CharacterTabardMode::Equipment),
            &self.helmet_visibility,
            equipment_items.iter().copied(),
        )?;
        let atlas = texture_plan.compose(&mut assets, &mut self.textures)?;
        let hair = load_optional_texture(texture_plan.hair(), &mut assets, &mut self.textures)?;
        let extra_skin =
            load_optional_texture(texture_plan.extra_skin(), &mut assets, &mut self.textures)?;
        let cape = load_optional_texture(texture_plan.cape(), &mut assets, &mut self.textures)?;
        let textures = prepare_model_textures(
            &model,
            hair.as_ref(),
            extra_skin.as_ref(),
            cape.as_ref(),
            &mut assets,
            &mut self.textures,
        )?;
        let mut attachments = Vec::with_capacity(attachment_plan.attachments().len());
        for attachment in attachment_plan.attachments() {
            let model = self.models.load(&mut assets, attachment.model())?;
            let uses_replacement = model.textures().iter().any(|texture| {
                matches!(
                    texture.kind(),
                    M2TextureKind::Item
                        | M2TextureKind::WeaponArmorBasic
                        | M2TextureKind::WeaponBlade
                )
            });
            let replacement = if uses_replacement {
                load_optional_texture(attachment.texture(), &mut assets, &mut self.textures)?
            } else {
                None
            };
            let textures = prepare_attachment_textures(
                &model,
                replacement.as_ref(),
                &mut assets,
                &mut self.textures,
            )?;
            attachments.push(ResidentPlayerAttachment {
                point: attachment.point(),
                model,
                textures,
                particle_colors: M2ParticleColorReplacement::resolve(
                    &self.particle_colors,
                    attachment.particle_color_id(),
                ),
            });
        }
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
            world_transform: world.local_player_transform()?,
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

    /// Returns the complete stock texture plan for the resident appearance.
    #[must_use]
    pub fn resident_texture_plan(&self) -> Option<&CharacterTexturePlan> {
        self.resident
            .as_ref()
            .map(|resident| &resident.texture_plan)
    }

    /// Returns the complete player-frame input without exposing mutable residency.
    pub(super) fn resident_frame_input(&self) -> Option<ResidentPlayerFrameInput<'_>> {
        self.resident
            .as_ref()
            .map(ResidentPlayerFrameInput::from_resident)
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
    base_texture_plan: CharacterTexturePlan,
    base_geosets: CharacterGeosetPlan,
    equipment_key: Vec<(PlayerEquipmentSlot, VisibleEquipmentItem)>,
    attachment_plan: CharacterAttachmentPlan,
    texture_plan: CharacterTexturePlan,
    geosets: CharacterGeosetPlan,
    atlas: CharacterAtlasTexture,
    hair: Option<Arc<BlpTextureSource>>,
    extra_skin: Option<Arc<BlpTextureSource>>,
    textures: Vec<ResidentPlayerTexture>,
    attachments: Vec<ResidentPlayerAttachment>,
    world_transform: WorldTransform,
    camera_height: CameraSubjectHeight,
    camera_pose: PlayerCameraPose,
    model: Arc<DecodedM2Model>,
}

/// One player M2 texture declaration after customization resolution.
pub(super) enum ResidentPlayerTexture {
    /// A concrete BLP selected through ordinary archive precedence.
    Authored(Arc<BlpTextureSource>),
    /// The placement-owned body atlas produced by stock composition.
    BodyAtlas,
    /// A replacement category not supplied by the naked body presentation.
    Unresolved(M2TextureKind),
}

/// One independently animated M2 attached to the player body pose.
pub(super) struct ResidentPlayerAttachment {
    point: CharacterAttachmentPoint,
    model: Arc<DecodedM2Model>,
    textures: Vec<ResidentPlayerTexture>,
    particle_colors: Option<M2ParticleColorReplacement>,
}

impl ResidentPlayerAttachment {
    pub(super) const fn point(&self) -> CharacterAttachmentPoint {
        self.point
    }

    pub(super) const fn model(&self) -> &Arc<DecodedM2Model> {
        &self.model
    }

    pub(super) fn textures(&self) -> &[ResidentPlayerTexture] {
        &self.textures
    }

    pub(super) const fn particle_colors(&self) -> Option<&M2ParticleColorReplacement> {
        self.particle_colors.as_ref()
    }
}

/// Borrowed immutable inputs required to publish the resident player M2.
pub(super) struct ResidentPlayerFrameInput<'a> {
    guid: u64,
    model: &'a Arc<DecodedM2Model>,
    textures: &'a [ResidentPlayerTexture],
    atlas: &'a CharacterAtlasTexture,
    geosets: &'a CharacterGeosetPlan,
    world_transform: WorldTransform,
    object_scale: f32,
    particle_colors: Option<&'a M2ParticleColorReplacement>,
    attachments: &'a [ResidentPlayerAttachment],
}

impl<'a> ResidentPlayerFrameInput<'a> {
    fn from_resident(resident: &'a ResidentPlayerModel) -> Self {
        Self {
            guid: resident.guid,
            model: &resident.model,
            textures: &resident.textures,
            atlas: &resident.atlas,
            geosets: &resident.geosets,
            world_transform: resident.world_transform,
            object_scale: resident.object_scale,
            particle_colors: resident.particle_colors.as_ref(),
            attachments: &resident.attachments,
        }
    }

    pub(super) const fn guid(&self) -> u64 {
        self.guid
    }

    pub(super) const fn model(&self) -> &Arc<DecodedM2Model> {
        self.model
    }

    pub(super) const fn textures(&self) -> &[ResidentPlayerTexture] {
        self.textures
    }

    pub(super) const fn atlas(&self) -> &CharacterAtlasTexture {
        self.atlas
    }

    pub(super) const fn geosets(&self) -> &CharacterGeosetPlan {
        self.geosets
    }

    pub(super) const fn world_transform(&self) -> WorldTransform {
        self.world_transform
    }

    pub(super) const fn object_scale(&self) -> f32 {
        self.object_scale
    }

    pub(super) const fn particle_colors(&self) -> Option<&M2ParticleColorReplacement> {
        self.particle_colors
    }

    pub(super) const fn attachments(&self) -> &[ResidentPlayerAttachment] {
        self.attachments
    }
}

/// Loads one optional character replacement without inventing a substitute.
fn load_optional_texture(
    path: Option<&AssetPath>,
    assets: &mut solarity_asset::AssetStore,
    textures: &mut BlpTextureCache,
) -> Result<Option<Arc<BlpTextureSource>>, AssetError> {
    path.map(|path| textures.load(assets, path)).transpose()
}

/// Resolves every body-model texture slot without inventing equipment inputs.
fn prepare_model_textures(
    model: &DecodedM2Model,
    hair: Option<&Arc<BlpTextureSource>>,
    extra_skin: Option<&Arc<BlpTextureSource>>,
    cape: Option<&Arc<BlpTextureSource>>,
    assets: &mut solarity_asset::AssetStore,
    textures: &mut BlpTextureCache,
) -> Result<Vec<ResidentPlayerTexture>, RuntimePlayerError> {
    model
        .textures()
        .iter()
        .map(|texture| match texture.kind() {
            M2TextureKind::Hardcoded => {
                let path = texture.filename().ok_or_else(|| {
                    RuntimePlayerError::MissingHardcodedTexturePath {
                        model: model.path().clone(),
                    }
                })?;
                Ok(ResidentPlayerTexture::Authored(
                    textures.load(assets, path)?,
                ))
            }
            M2TextureKind::Body => Ok(ResidentPlayerTexture::BodyAtlas),
            // Build-12340 CCharacterComponent binds its resolved hair image to
            // special texture slot 6. Every stock playable body M2 authors
            // that slot as Environment; the generic type-7 Hair category is
            // not a substitute.
            M2TextureKind::Environment => Ok(hair.map_or(
                ResidentPlayerTexture::Unresolved(M2TextureKind::Environment),
                |source| ResidentPlayerTexture::Authored(Arc::clone(source)),
            )),
            M2TextureKind::SkinExtra => Ok(extra_skin.map_or(
                ResidentPlayerTexture::Unresolved(M2TextureKind::SkinExtra),
                |source| ResidentPlayerTexture::Authored(Arc::clone(source)),
            )),
            M2TextureKind::Item => Ok(cape.map_or(
                ResidentPlayerTexture::Unresolved(M2TextureKind::Item),
                |source| ResidentPlayerTexture::Authored(Arc::clone(source)),
            )),
            kind => Ok(ResidentPlayerTexture::Unresolved(kind)),
        })
        .collect()
}

/// Resolves one attached item M2's hardcoded and display replacement slots.
fn prepare_attachment_textures(
    model: &DecodedM2Model,
    replacement: Option<&Arc<BlpTextureSource>>,
    assets: &mut solarity_asset::AssetStore,
    textures: &mut BlpTextureCache,
) -> Result<Vec<ResidentPlayerTexture>, RuntimePlayerError> {
    model
        .textures()
        .iter()
        .map(|texture| match texture.kind() {
            M2TextureKind::Hardcoded => {
                let path = texture.filename().ok_or_else(|| {
                    RuntimePlayerError::MissingHardcodedTexturePath {
                        model: model.path().clone(),
                    }
                })?;
                Ok(ResidentPlayerTexture::Authored(
                    textures.load(assets, path)?,
                ))
            }
            M2TextureKind::Item | M2TextureKind::WeaponArmorBasic | M2TextureKind::WeaponBlade => {
                Ok(replacement.map_or(
                    ResidentPlayerTexture::Unresolved(texture.kind()),
                    |source| ResidentPlayerTexture::Authored(Arc::clone(source)),
                ))
            }
            kind => Ok(ResidentPlayerTexture::Unresolved(kind)),
        })
        .collect()
}

impl ResidentPlayerModel {
    fn path(&self) -> &AssetPath {
        self.model.path()
    }
}
