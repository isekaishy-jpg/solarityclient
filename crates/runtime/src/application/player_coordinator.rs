//! Local-player model residency and authored presentation measurements.

use std::sync::Arc;

use solarity_asset::{
    AnimationDataCatalog, AssetError, AssetPath, AssetStoreHandle, BlpTextureCache,
    BlpTextureSource, CharacterAppearanceCatalog, CharacterRaceCatalog, CreatureCatalog,
    CreatureModelAppearance, DecodedM2Model, HelmetGeosetVisibilityCatalog, ItemDefinitionCatalog,
    ItemDisplayCatalog, ItemVisualCatalog, M2ModelCache, M2TextureKind, ParticleColorCatalog,
};
use solarity_ecs::{
    ActiveWorld, PlayerEquipmentSlot, VisibleEquipmentItem, WorldStateError, WorldTransform,
};
use solarity_rendering::{
    CharacterAtlasTexture, CharacterAttachmentPlan, CharacterAttachmentPlanError,
    CharacterAttachmentPoint, CharacterEquipmentItem, CharacterGeosetContext, CharacterGeosetPlan,
    CharacterGeosetPlanError, CharacterItemVisualPlan, CharacterTabardMode,
    CharacterTextureComposeError, CharacterTexturePlan, CharacterTexturePlanError,
    CharacterWeaponState, M2ParticleColorReplacement, WorldCamera,
};
use solarity_systems::{
    CameraSubjectHeight, CameraSubjectHeightError, PlayerCameraPose, PlayerCameraPoseError,
    PlayerEquipmentAppearanceError, UnitLocomotionAnimation, UnitModelAnimation,
    UnitModelAppearanceError, resolve_model_camera_subject_height, resolve_player_camera_pose,
    resolve_player_equipment, resolve_unit_locomotion_animation, resolve_unit_model,
    resolve_unit_model_animation,
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
    /// DBC fallback traversal found no animation sequence present in the M2.
    #[error(
        "player M2 {model} has no stock fallback for animation {animation_id} at tier {animation_tier}"
    )]
    MissingModelAnimation {
        /// Model whose sequences exhausted the stock fallback path.
        model: AssetPath,
        /// Base AnimationData identifier selected from gameplay state.
        animation_id: u16,
        /// Numeric unit animation tier supplied by `UNIT_FIELD_BYTES_1`.
        animation_tier: u8,
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

/// Observable result of one visible-creature residency synchronization pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeCreaturePoll {
    /// No active world exists and no creature model is resident.
    Idle,
    /// The visible creature set or one of its appearances changed.
    ModelsChanged,
    /// Existing creature models received only transform/animation updates.
    Current,
}

/// Observable result of one remote-player residency synchronization pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeRemotePlayerPoll {
    /// No active world exists and no remote character is resident.
    Idle,
    /// The remote player set or one complete appearance changed.
    ModelsChanged,
    /// Existing remote characters received only transform/animation updates.
    Current,
}

/// Immutable client-table dependencies used to resolve player presentation.
pub struct RuntimePlayerCatalogs {
    animations: AnimationDataCatalog,
    creatures: CreatureCatalog,
    characters: CharacterAppearanceCatalog,
    races: CharacterRaceCatalog,
    helmet_visibility: HelmetGeosetVisibilityCatalog,
    items: RuntimePlayerItemCatalogs,
    particle_colors: ParticleColorCatalog,
}

/// Item-table group consumed together by player equipment presentation.
pub struct RuntimePlayerItemCatalogs {
    definitions: ItemDefinitionCatalog,
    displays: ItemDisplayCatalog,
    visuals: ItemVisualCatalog,
}

impl RuntimePlayerItemCatalogs {
    /// Groups exact item definition, display, and visual joins.
    #[must_use]
    pub const fn new(
        definitions: ItemDefinitionCatalog,
        displays: ItemDisplayCatalog,
        visuals: ItemVisualCatalog,
    ) -> Self {
        Self {
            definitions,
            displays,
            visuals,
        }
    }
}

impl RuntimePlayerCatalogs {
    /// Groups the exact stock tables consumed by player presentation.
    #[must_use]
    pub fn new(
        animations: AnimationDataCatalog,
        creatures: CreatureCatalog,
        characters: CharacterAppearanceCatalog,
        races: CharacterRaceCatalog,
        helmet_visibility: HelmetGeosetVisibilityCatalog,
        items: RuntimePlayerItemCatalogs,
        particle_colors: ParticleColorCatalog,
    ) -> Self {
        Self {
            animations,
            creatures,
            characters,
            races,
            helmet_visibility,
            items,
            particle_colors,
        }
    }
}

/// Resolves ECS appearance into a shared model without putting assets in ECS.
pub struct RuntimePlayerPresentation {
    assets: AssetStoreHandle,
    animations: AnimationDataCatalog,
    creatures: CreatureCatalog,
    characters: CharacterAppearanceCatalog,
    races: CharacterRaceCatalog,
    helmet_visibility: HelmetGeosetVisibilityCatalog,
    item_definitions: ItemDefinitionCatalog,
    item_displays: ItemDisplayCatalog,
    item_visuals: ItemVisualCatalog,
    particle_colors: ParticleColorCatalog,
    models: M2ModelCache,
    textures: BlpTextureCache,
    resident: Option<ResidentPlayerModel>,
    creatures_resident: Vec<ResidentCreatureModel>,
    remote_players: Vec<ResidentPlayerModel>,
}

impl RuntimePlayerPresentation {
    /// Creates an empty owner over process-wide assets and immutable DBC catalogs.
    #[must_use]
    pub fn new(assets: AssetStoreHandle, catalogs: RuntimePlayerCatalogs) -> Self {
        Self {
            assets,
            animations: catalogs.animations,
            creatures: catalogs.creatures,
            characters: catalogs.characters,
            races: catalogs.races,
            helmet_visibility: catalogs.helmet_visibility,
            item_definitions: catalogs.items.definitions,
            item_displays: catalogs.items.displays,
            item_visuals: catalogs.items.visuals,
            particle_colors: catalogs.particle_colors,
            models: M2ModelCache::new(),
            textures: BlpTextureCache::new(),
            resident: None,
            creatures_resident: Vec::new(),
            remote_players: Vec::new(),
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
            let requested_animation = world.movement_state(guid).map_or(
                UnitLocomotionAnimation::STAND,
                resolve_unit_locomotion_animation,
            );
            if let Some(resident) = self.resident.as_mut() {
                resident.world_transform = transform;
                resident.animation = resolve_resident_animation(
                    &self.animations,
                    &resident.model,
                    requested_animation,
                    unit_presentation.animation_tier(),
                )?;
                resident.camera_pose = Some(resolve_player_camera_pose(
                    transform,
                    view,
                    resident.camera_height,
                )?);
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
        let attachments = load_player_attachments(
            &attachment_plan,
            &self.item_visuals,
            &self.particle_colors,
            &mut self.models,
            &mut self.textures,
            &mut assets,
        )?;
        drop(assets);
        let camera_height = resolve_model_camera_subject_height(&model, scale)?;
        let camera_pose = resolve_player_camera_pose(
            world.local_player_transform()?,
            world.local_player_view()?,
            camera_height,
        )?;
        let particle_colors =
            M2ParticleColorReplacement::resolve(&self.particle_colors, particle_color_id);
        let requested_animation = world.movement_state(guid).map_or(
            UnitLocomotionAnimation::STAND,
            resolve_unit_locomotion_animation,
        );
        let animation = resolve_resident_animation(
            &self.animations,
            &model,
            requested_animation,
            unit_presentation.animation_tier(),
        )?;
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
            animation,
            camera_height,
            camera_pose: Some(camera_pose),
            model,
        });
        self.models.collect_unused();
        self.textures.collect_unused();
        Ok(RuntimePlayerPoll::ModelLoaded)
    }

    /// Synchronizes every visible non-player unit into shared M2 residency.
    ///
    /// Player objects require character atlas and equipment composition and
    /// remain on the dedicated player path. This pass admits creature objects
    /// only after their complete display, transform, and tier state exists.
    pub fn synchronize_creatures(
        &mut self,
        world: Option<&ActiveWorld>,
    ) -> Result<RuntimeCreaturePoll, RuntimePlayerError> {
        let Some(world) = world else {
            self.creatures_resident.clear();
            self.models.collect_unused();
            self.textures.collect_unused();
            return Ok(RuntimeCreaturePoll::Idle);
        };

        let mut desired = Vec::new();
        for guid in world.visible_unit_guids() {
            if world.object_kind(guid) != Some(solarity_ecs::ObjectKind::Unit) {
                continue;
            }
            let Some(transform) = world.object_transform(guid) else {
                continue;
            };
            let Some(presentation) = world.unit_presentation(guid) else {
                continue;
            };
            let appearance =
                match resolve_unit_model(world, guid, &self.creatures, &self.characters) {
                    Ok(appearance) => appearance,
                    Err(
                        UnitModelAppearanceError::MissingObjectPresentation { .. }
                        | UnitModelAppearanceError::MissingUnitPresentation { .. },
                    ) => continue,
                    Err(error) => return Err(error.into()),
                };
            let requested_animation = world.movement_state(guid).map_or(
                UnitLocomotionAnimation::STAND,
                resolve_unit_locomotion_animation,
            );
            desired.push(DesiredCreatureModel {
                key: CreatureModelKey {
                    guid,
                    display_id: appearance.body().display().id(),
                    path: appearance.body().model_path().clone(),
                    object_scale: appearance.object_scale(),
                    particle_color_id: appearance.body().display().particle_color_id(),
                },
                transform,
                requested_animation,
                animation_tier: presentation.animation_tier(),
            });
        }

        let unchanged = desired.len() == self.creatures_resident.len()
            && desired
                .iter()
                .zip(&self.creatures_resident)
                .all(|(desired, resident)| desired.key == resident.key);
        if unchanged {
            for (desired, resident) in desired.iter().zip(&mut self.creatures_resident) {
                resident.world_transform = desired.transform;
                resident.animation = resolve_resident_animation(
                    &self.animations,
                    &resident.model,
                    desired.requested_animation,
                    desired.animation_tier,
                )?;
            }
            return Ok(RuntimeCreaturePoll::Current);
        }

        let mut assets = self.assets.borrow_mut();
        let mut residents = Vec::with_capacity(desired.len());
        for desired in desired {
            let appearance = self
                .creatures
                .resolve_model(desired.key.display_id)
                .map_err(UnitModelAppearanceError::from)?;
            let model = self.models.load(&mut assets, appearance.model_path())?;
            let textures =
                prepare_creature_textures(&model, &appearance, &mut assets, &mut self.textures)?;
            let animation = resolve_resident_animation(
                &self.animations,
                &model,
                desired.requested_animation,
                desired.animation_tier,
            )?;
            residents.push(ResidentCreatureModel {
                key: desired.key,
                model,
                textures,
                particle_colors: M2ParticleColorReplacement::resolve(
                    &self.particle_colors,
                    appearance.display().particle_color_id(),
                ),
                world_transform: desired.transform,
                animation,
            });
        }
        drop(assets);
        self.creatures_resident = residents;
        self.models.collect_unused();
        self.textures.collect_unused();
        Ok(RuntimeCreaturePoll::ModelsChanged)
    }

    /// Synchronizes every visible non-local player through character composition.
    pub fn synchronize_remote_players(
        &mut self,
        world: Option<&ActiveWorld>,
    ) -> Result<RuntimeRemotePlayerPoll, RuntimePlayerError> {
        let Some(world) = world else {
            self.remote_players.clear();
            self.models.collect_unused();
            self.textures.collect_unused();
            return Ok(RuntimeRemotePlayerPoll::Idle);
        };
        let local_guid = world.local_player_guid()?;
        let mut desired = Vec::new();
        for guid in world.visible_unit_guids() {
            if guid == local_guid
                || world.object_kind(guid) != Some(solarity_ecs::ObjectKind::Player)
            {
                continue;
            }
            if let Some(player) = self.resolve_desired_remote_player(world, guid)? {
                desired.push(player);
            }
        }

        let unchanged = desired.len() == self.remote_players.len()
            && desired
                .iter()
                .zip(&self.remote_players)
                .all(|(desired, resident)| resident.matches_remote(desired));
        if unchanged {
            for (desired, resident) in desired.iter().zip(&mut self.remote_players) {
                resident.world_transform = desired.world_transform;
                resident.animation = resolve_resident_animation(
                    &self.animations,
                    &resident.model,
                    desired.requested_animation,
                    desired.animation_tier,
                )?;
            }
            return Ok(RuntimeRemotePlayerPoll::Current);
        }

        let mut residents = Vec::with_capacity(desired.len());
        for desired in desired {
            residents.push(self.load_remote_player(world, desired)?);
        }
        self.remote_players = residents;
        self.models.collect_unused();
        self.textures.collect_unused();
        Ok(RuntimeRemotePlayerPoll::ModelsChanged)
    }

    fn resolve_desired_remote_player(
        &self,
        world: &ActiveWorld,
        guid: u64,
    ) -> Result<Option<DesiredRemotePlayerModel>, RuntimePlayerError> {
        let appearance = match resolve_unit_model(world, guid, &self.creatures, &self.characters) {
            Ok(appearance) => appearance,
            Err(
                UnitModelAppearanceError::MissingObjectPresentation { .. }
                | UnitModelAppearanceError::MissingUnitPresentation { .. }
                | UnitModelAppearanceError::MissingUnitIdentity { .. }
                | UnitModelAppearanceError::MissingPlayerAppearance { .. },
            ) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let Some(world_transform) = world.object_transform(guid) else {
            return Ok(None);
        };
        let Some(unit_presentation) = world.unit_presentation(guid) else {
            return Ok(None);
        };
        let character = appearance
            .character()
            .ok_or(RuntimePlayerError::MissingCharacterAppearance { guid })?;
        let class_id = appearance
            .player_class_id()
            .ok_or(RuntimePlayerError::MissingCharacterAppearance { guid })?;
        let equipment = match resolve_player_equipment(
            world,
            guid,
            &self.item_definitions,
            &self.item_displays,
        ) {
            Ok(equipment) => equipment,
            Err(PlayerEquipmentAppearanceError::MissingEquipment { .. }) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
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
            .collect();
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
        Ok(Some(DesiredRemotePlayerModel {
            guid,
            object_scale: appearance.object_scale(),
            particle_color_id: appearance.body().display().particle_color_id(),
            path: appearance.body().model_path().clone(),
            base_texture_plan,
            base_geosets,
            equipment_key,
            attachment_plan,
            world_transform,
            requested_animation,
            animation_tier: unit_presentation.animation_tier(),
        }))
    }

    fn load_remote_player(
        &mut self,
        world: &ActiveWorld,
        desired: DesiredRemotePlayerModel,
    ) -> Result<ResidentPlayerModel, RuntimePlayerError> {
        let appearance =
            resolve_unit_model(world, desired.guid, &self.creatures, &self.characters)?;
        let character = appearance
            .character()
            .ok_or(RuntimePlayerError::MissingCharacterAppearance { guid: desired.guid })?;
        let class_id = appearance
            .player_class_id()
            .ok_or(RuntimePlayerError::MissingCharacterAppearance { guid: desired.guid })?;
        let equipment = resolve_player_equipment(
            world,
            desired.guid,
            &self.item_definitions,
            &self.item_displays,
        )?;
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
        let mut assets = self.assets.borrow_mut();
        let model = self.models.load(&mut assets, &desired.path)?;
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
        let attachments = load_player_attachments(
            &desired.attachment_plan,
            &self.item_visuals,
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
            desired.requested_animation,
            desired.animation_tier,
        )?;
        Ok(ResidentPlayerModel {
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
            animation,
            camera_height,
            camera_pose: None,
            model,
        })
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

    /// Returns the number of visible non-player unit models currently resident.
    #[must_use]
    pub fn resident_creature_count(&self) -> usize {
        self.creatures_resident.len()
    }

    /// Returns the number of fully composed visible remote player models.
    #[must_use]
    pub fn resident_remote_player_count(&self) -> usize {
        self.remote_players.len()
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

    /// Returns all visible creature inputs in deterministic GUID order.
    pub(super) fn resident_creature_frame_inputs(&self) -> Vec<ResidentCreatureFrameInput<'_>> {
        self.creatures_resident
            .iter()
            .map(ResidentCreatureFrameInput::from_resident)
            .collect()
    }

    /// Returns remote character inputs in deterministic GUID order.
    pub(super) fn resident_remote_player_frame_inputs(&self) -> Vec<ResidentPlayerFrameInput<'_>> {
        self.remote_players
            .iter()
            .map(ResidentPlayerFrameInput::from_resident)
            .collect()
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
        self.resident
            .as_ref()
            .and_then(|resident| resident.camera_pose)
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
        self.creatures_resident.clear();
        self.remote_players.clear();
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
    animation: UnitModelAnimation,
    camera_height: CameraSubjectHeight,
    camera_pose: Option<PlayerCameraPose>,
    model: Arc<DecodedM2Model>,
}

struct DesiredRemotePlayerModel {
    guid: u64,
    object_scale: f32,
    particle_color_id: u32,
    path: AssetPath,
    base_texture_plan: CharacterTexturePlan,
    base_geosets: CharacterGeosetPlan,
    equipment_key: Vec<(PlayerEquipmentSlot, VisibleEquipmentItem)>,
    attachment_plan: CharacterAttachmentPlan,
    world_transform: WorldTransform,
    requested_animation: UnitLocomotionAnimation,
    animation_tier: solarity_ecs::UnitAnimationTier,
}

#[derive(PartialEq)]
struct CreatureModelKey {
    guid: u64,
    display_id: u32,
    path: AssetPath,
    object_scale: f32,
    particle_color_id: u32,
}

struct DesiredCreatureModel {
    key: CreatureModelKey,
    transform: WorldTransform,
    requested_animation: UnitLocomotionAnimation,
    animation_tier: solarity_ecs::UnitAnimationTier,
}

struct ResidentCreatureModel {
    key: CreatureModelKey,
    model: Arc<DecodedM2Model>,
    textures: Vec<ResidentCreatureTexture>,
    particle_colors: Option<M2ParticleColorReplacement>,
    world_transform: WorldTransform,
    animation: UnitModelAnimation,
}

/// One creature texture after display replacement resolution.
pub(super) enum ResidentCreatureTexture {
    /// A concrete hardcoded or monster-skin BLP.
    Authored(Arc<BlpTextureSource>),
    /// An unsupported replacement category remains explicitly unresolved.
    Unresolved(M2TextureKind),
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
    visual_effects: Vec<ResidentPlayerItemVisualEffect>,
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

    pub(super) fn visual_effects(&self) -> &[ResidentPlayerItemVisualEffect] {
        &self.visual_effects
    }

    pub(super) const fn particle_colors(&self) -> Option<&M2ParticleColorReplacement> {
        self.particle_colors.as_ref()
    }
}

/// One effect model driven by an equipped item model's animated pose.
pub(super) struct ResidentPlayerItemVisualEffect {
    point: u32,
    model: Arc<DecodedM2Model>,
    textures: Vec<ResidentPlayerTexture>,
}

impl ResidentPlayerItemVisualEffect {
    pub(super) const fn point(&self) -> u32 {
        self.point
    }

    pub(super) const fn model(&self) -> &Arc<DecodedM2Model> {
        &self.model
    }

    pub(super) fn textures(&self) -> &[ResidentPlayerTexture] {
        &self.textures
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
    animation: UnitModelAnimation,
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
            animation: resident.animation,
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

    pub(super) const fn animation(&self) -> UnitModelAnimation {
        self.animation
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

/// Borrowed immutable inputs required to publish one visible creature M2.
pub(super) struct ResidentCreatureFrameInput<'a> {
    guid: u64,
    model: &'a Arc<DecodedM2Model>,
    textures: &'a [ResidentCreatureTexture],
    world_transform: WorldTransform,
    object_scale: f32,
    animation: UnitModelAnimation,
    particle_colors: Option<&'a M2ParticleColorReplacement>,
}

impl<'a> ResidentCreatureFrameInput<'a> {
    fn from_resident(resident: &'a ResidentCreatureModel) -> Self {
        Self {
            guid: resident.key.guid,
            model: &resident.model,
            textures: &resident.textures,
            world_transform: resident.world_transform,
            object_scale: resident.key.object_scale,
            animation: resident.animation,
            particle_colors: resident.particle_colors.as_ref(),
        }
    }

    pub(super) const fn guid(&self) -> u64 {
        self.guid
    }

    pub(super) const fn model(&self) -> &Arc<DecodedM2Model> {
        self.model
    }

    pub(super) const fn textures(&self) -> &[ResidentCreatureTexture] {
        self.textures
    }

    pub(super) const fn world_transform(&self) -> WorldTransform {
        self.world_transform
    }

    pub(super) const fn object_scale(&self) -> f32 {
        self.object_scale
    }

    pub(super) const fn animation(&self) -> UnitModelAnimation {
        self.animation
    }

    pub(super) const fn particle_colors(&self) -> Option<&M2ParticleColorReplacement> {
        self.particle_colors
    }
}

/// Applies stock AnimationData tier and fallback traversal to the resident M2.
fn resolve_resident_animation(
    catalog: &AnimationDataCatalog,
    model: &DecodedM2Model,
    requested: UnitLocomotionAnimation,
    tier: solarity_ecs::UnitAnimationTier,
) -> Result<UnitModelAnimation, RuntimePlayerError> {
    if model.animations().sequences().is_empty() {
        // The stock resolver returns its requested ID when no sequence table is
        // active; renderer playback likewise treats an empty table as static.
        return Ok(UnitModelAnimation::static_request(requested));
    }
    let selected = resolve_unit_model_animation(catalog, requested, tier, |animation_id| {
        model
            .animations()
            .available_variation_count(animation_id)
            .is_some()
    });
    selected.ok_or_else(|| RuntimePlayerError::MissingModelAnimation {
        model: model.path().clone(),
        animation_id: requested.animation_id(),
        animation_tier: tier as u8,
    })
}

/// Resolves hardcoded and display-selected monster texture categories.
fn prepare_creature_textures(
    model: &DecodedM2Model,
    appearance: &CreatureModelAppearance<'_>,
    assets: &mut solarity_asset::AssetStore,
    textures: &mut BlpTextureCache,
) -> Result<Vec<ResidentCreatureTexture>, RuntimePlayerError> {
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
                Ok(ResidentCreatureTexture::Authored(
                    textures.load(assets, path)?,
                ))
            }
            kind
            @ (M2TextureKind::Monster1 | M2TextureKind::Monster2 | M2TextureKind::Monster3) => {
                appearance.texture_for(kind).map_or_else(
                    || Ok(ResidentCreatureTexture::Unresolved(kind)),
                    |path| {
                        textures
                            .load(assets, path)
                            .map(ResidentCreatureTexture::Authored)
                            .map_err(RuntimePlayerError::from)
                    },
                )
            }
            kind => Ok(ResidentCreatureTexture::Unresolved(kind)),
        })
        .collect()
}

/// Loads equipped component models and their attached item-visual effects.
fn load_player_attachments(
    attachment_plan: &CharacterAttachmentPlan,
    item_visuals: &ItemVisualCatalog,
    particle_colors: &ParticleColorCatalog,
    models: &mut M2ModelCache,
    textures: &mut BlpTextureCache,
    assets: &mut solarity_asset::AssetStore,
) -> Result<Vec<ResidentPlayerAttachment>, RuntimePlayerError> {
    let mut attachments = Vec::with_capacity(attachment_plan.attachments().len());
    for attachment in attachment_plan.attachments() {
        let model = models.load(assets, attachment.model())?;
        let uses_replacement = model.textures().iter().any(|texture| {
            matches!(
                texture.kind(),
                M2TextureKind::Item | M2TextureKind::WeaponArmorBasic | M2TextureKind::WeaponBlade
            )
        });
        let replacement = if uses_replacement {
            load_optional_texture(attachment.texture(), assets, textures)?
        } else {
            None
        };
        let resolved_textures =
            prepare_attachment_textures(&model, replacement.as_ref(), assets, textures)?;
        let visual_plan = CharacterItemVisualPlan::resolve(attachment, item_visuals);
        let mut visual_effects = Vec::with_capacity(visual_plan.effects().len());
        for effect in visual_plan.effects() {
            // Stock omits an effect whose equipped component lacks the
            // authored attachment link; it does not select another point.
            if model.attachment(effect.attachment_id()).is_none() {
                continue;
            }
            let effect_model = models.load(assets, effect.model())?;
            let effect_textures =
                prepare_attachment_textures(&effect_model, None, assets, textures)?;
            visual_effects.push(ResidentPlayerItemVisualEffect {
                point: effect.attachment_id(),
                model: effect_model,
                textures: effect_textures,
            });
        }
        attachments.push(ResidentPlayerAttachment {
            point: attachment.point(),
            model,
            textures: resolved_textures,
            visual_effects,
            particle_colors: M2ParticleColorReplacement::resolve(
                particle_colors,
                attachment.particle_color_id(),
            ),
        });
    }
    Ok(attachments)
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

    fn matches_remote(&self, desired: &DesiredRemotePlayerModel) -> bool {
        self.guid == desired.guid
            && self.path() == &desired.path
            && self.object_scale == desired.object_scale
            && self.particle_color_id == desired.particle_color_id
            && self.base_texture_plan == desired.base_texture_plan
            && self.base_geosets == desired.base_geosets
            && self.equipment_key == desired.equipment_key
            && self.attachment_plan == desired.attachment_plan
    }
}
