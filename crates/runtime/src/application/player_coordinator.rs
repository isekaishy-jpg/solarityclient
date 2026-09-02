//! Local-player model residency and authored presentation measurements.

use std::sync::Arc;

use solarity_asset::{
    AnimationDataCatalog, AssetError, AssetPath, AssetStoreHandle, BlpTextureCache,
    BlpTextureSource, CharacterAppearanceCatalog, CharacterCustomization, CharacterRaceCatalog,
    CharacterStartOutfitCatalog, CreatureCatalog, CreatureModelAppearance, DecodedM2Model,
    HelmetGeosetVisibilityCatalog, InventoryType, ItemDefinitionCatalog, ItemDisplayCatalog,
    ItemVisualCatalog, M2ModelCache, M2TextureKind, ParticleColorCatalog,
};
use solarity_ecs::{
    ActiveWorld, PLAYER_EQUIPMENT_SLOT_COUNT, PlayerEquipmentSlot, PlayerViewState,
    UnitAnimationTier, UnitSheathState, VisibleEquipmentItem, WorldStateError, WorldTransform,
};
use solarity_rendering::{
    CharacterAtlasTexture, CharacterAttachmentPlan, CharacterAttachmentPlanError,
    CharacterAttachmentPoint, CharacterEquipmentItem, CharacterGeosetContext, CharacterGeosetPlan,
    CharacterGeosetPlanError, CharacterItemVisualPlan, CharacterTabardMode,
    CharacterTextureComposeError, CharacterTexturePlan, CharacterTexturePlanError,
    CharacterWeaponState, M2ParticleColorReplacement, WorldCamera,
};
use solarity_systems::{
    CameraSubjectHeight, CameraSubjectHeightError, MountCameraGeometry, MountCameraHeightError,
    PlayerCameraHeightState, PlayerCameraPose, PlayerCameraPoseError,
    PlayerEquipmentAppearanceError, UnitLocomotionAnimation, UnitModelAnimation,
    UnitModelAppearanceError, resolve_model_camera_subject_height,
    resolve_mounted_player_camera_pose, resolve_player_equipment,
    resolve_unit_locomotion_animation, resolve_unit_model, resolve_unit_model_animation,
};
use solarity_ui::{UiCharacterCreationPreview, UiCharacterSelectionPreview};
use thiserror::Error;

/// `CreatureDisplayInfoExtra` item-display columns in stock component order.
const NPC_EQUIPMENT_SLOTS: [PlayerEquipmentSlot; 11] = [
    PlayerEquipmentSlot::Head,
    PlayerEquipmentSlot::Shoulders,
    PlayerEquipmentSlot::Shirt,
    PlayerEquipmentSlot::Chest,
    PlayerEquipmentSlot::Waist,
    PlayerEquipmentSlot::Legs,
    PlayerEquipmentSlot::Feet,
    PlayerEquipmentSlot::Wrists,
    PlayerEquipmentSlot::Hands,
    PlayerEquipmentSlot::Tabard,
    PlayerEquipmentSlot::Back,
];

/// Failure while resolving the local player's authored presentation model.
#[derive(Debug, Error)]
pub enum RuntimePlayerError {
    /// The active ECS world lost a required player invariant.
    #[error(transparent)]
    World(#[from] WorldStateError),
    /// Projected unit state or a required DBC join is incomplete.
    #[error(transparent)]
    Appearance(#[from] UnitModelAppearanceError),
    /// Character-creation customization has no exact DBC appearance join.
    #[error(transparent)]
    CharacterAppearance(#[from] solarity_asset::AppearanceError),
    /// The selected M2 or one of its SKIN companions failed strict loading.
    #[error(transparent)]
    Asset(#[from] AssetError),
    /// Authored M2 geometry cannot produce a finite stock camera height.
    #[error(transparent)]
    CameraHeight(#[from] CameraSubjectHeightError),
    /// Authoritative movement and saved view state cannot form a finite orbit.
    #[error(transparent)]
    CameraPose(#[from] PlayerCameraPoseError),
    /// Live mount camera markers or their animation time are invalid.
    #[error(transparent)]
    MountCamera(#[from] MountCameraHeightError),
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
    /// The playable creation key has no stock starter-outfit row.
    #[error(
        "character creation has no starter outfit for race/class/gender {race_id}/{class_id}/{gender_id}"
    )]
    MissingCreationOutfit {
        /// Protocol race identifier.
        race_id: u8,
        /// Protocol class identifier.
        class_id: u8,
        /// Playable gender identifier.
        gender_id: u8,
    },
    /// A starter outfit references an item absent from the client item table.
    #[error("character creation starter outfit references missing item {item_id}")]
    MissingCreationOutfitItem {
        /// Absent `Item.dbc` identifier.
        item_id: u32,
    },
    /// A starter outfit item disagrees with its canonical item display.
    #[error(
        "character creation item {item_id} stores display {outfit_display_id}, but Item.dbc stores {item_display_id}"
    )]
    CreationOutfitDisplayMismatch {
        /// Starter item identifier.
        item_id: u32,
        /// Display identifier retained by `CharStartOutfit.dbc`.
        outfit_display_id: i32,
        /// Display identifier retained by `Item.dbc`.
        item_display_id: u32,
    },
    /// A starter outfit item disagrees with its canonical inventory type.
    #[error(
        "character creation item {item_id} stores inventory type {outfit_inventory_type}, but Item.dbc stores {item_inventory_type}"
    )]
    CreationOutfitInventoryMismatch {
        /// Starter item identifier.
        item_id: u32,
        /// Inventory type retained by `CharStartOutfit.dbc`.
        outfit_inventory_type: i32,
        /// Inventory type retained by `Item.dbc`.
        item_inventory_type: u32,
    },
    /// A wearable starter item references an absent display row.
    #[error("character creation starter item {item_id} references missing display {display_id}")]
    MissingCreationOutfitDisplay {
        /// Starter item identifier.
        item_id: u32,
        /// Absent `ItemDisplayInfo.dbc` identifier.
        display_id: u32,
    },
    /// Character enumeration supplied an inventory category outside the stock domain.
    #[error("character selection slot {slot} has invalid inventory type {inventory_type_id}")]
    InvalidSelectionInventoryType {
        /// Zero-based character-enumeration slot.
        slot: usize,
        /// Rejected raw inventory type.
        inventory_type_id: u8,
    },
    /// Character enumeration references an absent item display.
    #[error("character selection slot {slot} references missing display {display_id}")]
    MissingSelectionItemDisplay {
        /// Zero-based character-enumeration slot.
        slot: usize,
        /// Absent `ItemDisplayInfo.dbc` identifier.
        display_id: u32,
    },
    /// A player-model NPC has no stock baked body texture.
    #[error("creature display {display_id} has no baked character texture")]
    MissingNpcBakedTexture {
        /// `CreatureDisplayInfo.dbc` row whose extended appearance is incomplete.
        display_id: u32,
    },
    /// An extended NPC appearance references an absent item display row.
    #[error("creature display {display_id} references missing NPC item display {item_display_id}")]
    MissingNpcItemDisplay {
        /// Creature display selecting the extended appearance.
        display_id: u32,
        /// Absent `ItemDisplayInfo.dbc` identifier.
        item_display_id: u32,
    },
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
    /// Glue supplied a gender outside the two playable body-display columns.
    #[error("character creation gender {gender_id} has no playable body display")]
    InvalidCreationGender {
        /// Rejected protocol gender identifier.
        gender_id: u8,
    },
    /// Glue supplied a non-finite character-model facing.
    #[error("character creation facing {facing_degrees} degrees is invalid")]
    InvalidCreationFacing {
        /// Rejected model-facing value.
        facing_degrees: f64,
    },
    /// Glue supplied a non-finite selected-character facing.
    #[error("character selection facing {facing_degrees} degrees is invalid")]
    InvalidSelectionFacing {
        /// Rejected model-facing value.
        facing_degrees: f64,
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
    start_outfits: CharacterStartOutfitCatalog,
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
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        animations: AnimationDataCatalog,
        creatures: CreatureCatalog,
        characters: CharacterAppearanceCatalog,
        races: CharacterRaceCatalog,
        helmet_visibility: HelmetGeosetVisibilityCatalog,
        start_outfits: CharacterStartOutfitCatalog,
        items: RuntimePlayerItemCatalogs,
        particle_colors: ParticleColorCatalog,
    ) -> Self {
        Self {
            animations,
            creatures,
            characters,
            races,
            helmet_visibility,
            start_outfits,
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
    start_outfits: CharacterStartOutfitCatalog,
    item_definitions: ItemDefinitionCatalog,
    item_displays: ItemDisplayCatalog,
    item_visuals: ItemVisualCatalog,
    particle_colors: ParticleColorCatalog,
    models: M2ModelCache,
    textures: BlpTextureCache,
    resident: Option<ResidentPlayerModel>,
    creatures_resident: Vec<ResidentCreatureModel>,
    remote_players: Vec<ResidentPlayerModel>,
    glue_character: Option<ResidentGlueCharacterModel>,
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
            start_outfits: catalogs.start_outfits,
            item_definitions: catalogs.items.definitions,
            item_displays: catalogs.items.displays,
            item_visuals: catalogs.items.visuals,
            particle_colors: catalogs.particle_colors,
            models: M2ModelCache::new(),
            textures: BlpTextureCache::new(),
            resident: None,
            creatures_resident: Vec::new(),
            remote_players: Vec::new(),
            glue_character: None,
        }
    }

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
        let Some(preview) = preview else {
            return Ok(self.glue_character.take().is_some());
        };
        if self.glue_character.as_ref().is_some_and(|resident| {
            resident.key == ResidentGlueCharacterKey::Creation(preview.clone())
        }) {
            return Ok(false);
        }
        if !preview.facing_degrees().is_finite() {
            return Err(RuntimePlayerError::InvalidCreationFacing {
                facing_degrees: preview.facing_degrees(),
            });
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
        let equipment_items = resolve_creation_equipment(
            preview,
            &self.start_outfits,
            &self.item_definitions,
            &self.item_displays,
        )?;
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
        let authored_scale = body.display().model_scale() * body.model().model_scale();
        let model_scale = if authored_scale > 0.0 {
            authored_scale
        } else {
            1.0
        };
        let mut assets = self.assets.borrow_mut();
        let model = self.models.load(&mut assets, body.model_path())?;
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
            geosets,
            animation,
            model_scale,
            facing_radians: preview.facing_degrees().to_radians() as f32,
            attachments,
            particle_colors: M2ParticleColorReplacement::resolve(
                &self.particle_colors,
                body.display().particle_color_id(),
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
        let Some(preview) = preview else {
            return Ok(self.glue_character.take().is_some());
        };
        if self.glue_character.as_ref().is_some_and(|resident| {
            resident.key == ResidentGlueCharacterKey::Selection(Box::new(preview.clone()))
        }) {
            return Ok(false);
        }
        if !preview.facing_degrees().is_finite() {
            return Err(RuntimePlayerError::InvalidSelectionFacing {
                facing_degrees: preview.facing_degrees(),
            });
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
        let [skin, face, hair_style, hair_color, facial_hair] = preview.appearance();
        let customization =
            CharacterCustomization::new(skin, face, hair_style, hair_color, facial_hair);
        let appearance = self.characters.resolve_player(
            u32::from(preview.race_id()),
            u32::from(preview.gender_id()),
            customization,
        )?;
        let equipment_items = resolve_selection_equipment(preview, &self.item_displays)?;
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
            race,
            u32::from(preview.gender_id()),
            preview.class_id(),
        )?;
        let authored_scale = body.display().model_scale() * body.model().model_scale();
        let model_scale = if authored_scale > 0.0 {
            authored_scale
        } else {
            1.0
        };
        let mut assets = self.assets.borrow_mut();
        let model = self.models.load(&mut assets, body.model_path())?;
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
            geosets,
            animation,
            model_scale,
            facing_radians: preview.facing_degrees().to_radians() as f32,
            attachments,
            particle_colors: M2ParticleColorReplacement::resolve(
                &self.particle_colors,
                body.display().particle_color_id(),
            ),
        });
        Ok(true)
    }

    /// Returns renderer inputs for the current character-creation body.
    pub(super) fn creation_frame_input(&self) -> Option<ResidentCreationFrameInput<'_>> {
        self.glue_character
            .as_ref()
            .map(ResidentCreationFrameInput::from_resident)
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
        let path = M2ModelCache::canonical_path(appearance.body().model_path())?;
        let scale = appearance.object_scale();
        let body_model = appearance.body().model();
        let body_display = appearance.body().display();
        let authored_scale = body_display.model_scale() * body_model.model_scale();
        // CGUnit_C multiplies the authored display/model scale by the live
        // OBJECT_FIELD_SCALE_X before publishing its collision dimensions.
        let collision_scale = authored_scale * scale.max(0.001);
        let collision_extent = body_model
            .collision_extent()
            .map(|extent| extent * collision_scale);
        let particle_color_id = appearance.body().display().particle_color_id();
        let mount_key = appearance
            .mount()
            .map(|mount| mount_model_key(mount, appearance.object_scale()));
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
                && resident.path() == &path
                && resident.object_scale == scale
                && resident.particle_color_id == particle_color_id
                && resident.base_texture_plan == base_texture_plan
                && resident.base_geosets == base_geosets
                && resident.equipment_key == equipment_key
                && resident.attachment_plan == attachment_plan
                && resident.mount_key == mount_key
        }) {
            let transform = world.local_player_transform()?;
            let view = world.local_player_view()?;
            let requested_animation = world.movement_state(guid).map_or(
                UnitLocomotionAnimation::STAND,
                resolve_unit_locomotion_animation,
            );
            if let Some(resident) = self.resident.as_mut() {
                resident.world_transform = transform;
                resident.view = view;
                resident.animation = resolve_resident_animation(
                    &self.animations,
                    &resident.model,
                    if resident.mount.is_some() {
                        UnitLocomotionAnimation::MOUNT
                    } else {
                        requested_animation
                    },
                    unit_presentation.animation_tier(),
                )?;
                if let Some(mount) = resident.mount.as_mut() {
                    mount.animation = resolve_resident_animation(
                        &self.animations,
                        &mount.model,
                        requested_animation,
                        unit_presentation.animation_tier(),
                    )?;
                }
                let camera_heights = resident
                    .camera_height_state
                    .sample(resident.camera_time_ms)?;
                resident.camera_height = camera_heights.subject_height();
                resident.camera_pose = Some(resolve_mounted_player_camera_pose(
                    transform,
                    view,
                    camera_heights,
                )?);
            }
            return Ok(RuntimePlayerPoll::Current);
        }

        let requested_animation = world.movement_state(guid).map_or(
            UnitLocomotionAnimation::STAND,
            resolve_unit_locomotion_animation,
        );
        let mut assets = self.assets.borrow_mut();
        let model = self.models.load(&mut assets, &path)?;
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
        let mount = load_mount_model(
            appearance.mount(),
            appearance.object_scale(),
            requested_animation,
            unit_presentation.animation_tier(),
            &self.animations,
            &self.particle_colors,
            &mut self.models,
            &mut self.textures,
            &mut assets,
        )?;
        drop(assets);
        let base_camera_height = resolve_model_camera_subject_height(&model, scale)?;
        let previous_camera = self.resident.as_ref().and_then(|resident| {
            (resident.path() == &path && resident.object_scale == scale).then_some((
                resident.camera_height_state,
                resident.camera_time_ms,
                resident.mount_key.clone(),
            ))
        });
        let (mut camera_height_state, camera_time_ms, previous_mount_key) = previous_camera
            .unwrap_or((PlayerCameraHeightState::new(base_camera_height), 0.0, None));
        if previous_mount_key != mount_key {
            if mount_key.is_some() {
                camera_height_state.begin_mount_generation(camera_time_ms)?;
            } else {
                camera_height_state.set_mounted(false, camera_time_ms)?;
            }
        }
        let camera_heights = camera_height_state.sample(camera_time_ms)?;
        let camera_height = camera_heights.subject_height();
        let world_transform = world.local_player_transform()?;
        let view = world.local_player_view()?;
        let camera_pose =
            resolve_mounted_player_camera_pose(world_transform, view, camera_heights)?;
        let particle_colors =
            M2ParticleColorReplacement::resolve(&self.particle_colors, particle_color_id);
        let animation = resolve_resident_animation(
            &self.animations,
            &model,
            if mount.is_some() {
                UnitLocomotionAnimation::MOUNT
            } else {
                requested_animation
            },
            unit_presentation.animation_tier(),
        )?;
        self.resident = Some(ResidentPlayerModel {
            guid,
            object_scale: scale,
            collision_extent,
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
            world_transform,
            view,
            animation,
            camera_height,
            camera_height_state,
            camera_time_ms,
            camera_pose: Some(camera_pose),
            model,
            mount_key,
            mount,
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
            let (textures, geosets) = if let Some(extra) = appearance.extra() {
                let character = self.characters.resolve_player(
                    extra.race_id(),
                    extra.gender_id(),
                    CharacterCustomization::from_ids(
                        extra.skin_id(),
                        extra.face_id(),
                        extra.hair_style_id(),
                        extra.hair_color_id(),
                        extra.facial_hair_style_id(),
                    ),
                )?;
                let equipment = resolve_npc_equipment(
                    appearance.display().id(),
                    extra.npc_item_display_ids(),
                    &self.item_displays,
                )?;
                let texture_plan =
                    CharacterTexturePlan::equipped(&character, &assets, equipment.iter().copied())?;
                let textures = prepare_npc_character_textures(
                    &model,
                    &appearance,
                    &texture_plan,
                    &mut assets,
                    &mut self.textures,
                )?;
                let geosets = CharacterGeosetPlan::equipped(
                    &character,
                    CharacterGeosetContext::new(0, CharacterTabardMode::Equipment),
                    &self.helmet_visibility,
                    equipment.iter().copied(),
                )?;
                (textures, Some(geosets))
            } else {
                (
                    prepare_creature_textures(
                        &model,
                        &appearance,
                        &mut assets,
                        &mut self.textures,
                    )?,
                    None,
                )
            };
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
                geosets,
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
                    if resident.mount.is_some() {
                        UnitLocomotionAnimation::MOUNT
                    } else {
                        desired.requested_animation
                    },
                    desired.animation_tier,
                )?;
                if let Some(mount) = resident.mount.as_mut() {
                    mount.animation = resolve_resident_animation(
                        &self.animations,
                        &mount.model,
                        desired.requested_animation,
                        desired.animation_tier,
                    )?;
                }
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
        let authored_scale =
            appearance.body().display().model_scale() * appearance.body().model().model_scale();
        let collision_scale = authored_scale * appearance.object_scale().max(0.001);
        let collision_extent = appearance
            .body()
            .model()
            .collision_extent()
            .map(|extent| extent * collision_scale);
        Ok(Some(DesiredRemotePlayerModel {
            guid,
            object_scale: appearance.object_scale(),
            collision_extent,
            particle_color_id: appearance.body().display().particle_color_id(),
            path: M2ModelCache::canonical_path(appearance.body().model_path())?,
            base_texture_plan,
            base_geosets,
            equipment_key,
            attachment_plan,
            world_transform,
            requested_animation,
            animation_tier: unit_presentation.animation_tier(),
            mount_key: appearance
                .mount()
                .map(|mount| mount_model_key(mount, appearance.object_scale())),
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
        let mount = load_mount_model(
            appearance.mount(),
            appearance.object_scale(),
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
        Ok(ResidentPlayerModel {
            guid: desired.guid,
            object_scale: desired.object_scale,
            collision_extent: desired.collision_extent,
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

    /// Returns the archive-selected active mount M2 when the server supplies one.
    #[must_use]
    pub fn resident_mount_model(&self) -> Option<&Arc<DecodedM2Model>> {
        self.resident
            .as_ref()
            .and_then(|resident| resident.mount.as_ref())
            .map(|mount| &mount.model)
    }

    /// Returns the mount's stock DBC and object-field scale product.
    #[must_use]
    pub fn resident_mount_scale(&self) -> Option<f32> {
        self.resident
            .as_ref()
            .and_then(|resident| resident.mount.as_ref())
            .map(|mount| mount.object_scale)
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

    /// Returns the number of visible remote characters with active mount models.
    #[must_use]
    pub fn resident_remote_mount_count(&self) -> usize {
        self.remote_players
            .iter()
            .filter(|player| player.mount.is_some())
            .count()
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

    /// Returns the selected player's scaled stock collision width and height.
    #[must_use]
    pub fn collision_extent(&self) -> Option<[f32; 2]> {
        self.resident
            .as_ref()
            .map(|resident| resident.collision_extent)
    }

    /// Returns the current pre-collision camera orbit for the resident player.
    #[must_use]
    pub fn camera_pose(&self) -> Option<PlayerCameraPose> {
        self.resident
            .as_ref()
            .and_then(|resident| resident.camera_pose)
    }

    /// Applies markers sampled from the current rendered mount bone pose.
    ///
    /// The renderer owns the precise animation clock and therefore publishes
    /// this sample after pose evaluation. The resulting camera state is ready
    /// for the next target-camera pass without turning M2 events into a second
    /// gameplay event stream.
    pub(super) fn apply_mount_camera_sample(
        &mut self,
        geometry: Option<MountCameraGeometry>,
        time_ms: f32,
    ) -> Result<(), RuntimePlayerError> {
        let Some(resident) = self.resident.as_mut() else {
            return Ok(());
        };
        resident.camera_time_ms = time_ms;
        if let Some(geometry) = geometry {
            resident
                .camera_height_state
                .update_mount(geometry, time_ms)?;
        } else {
            resident.camera_height_state.set_mounted(false, time_ms)?;
        }
        let camera_heights = resident.camera_height_state.sample(time_ms)?;
        resident.camera_height = camera_heights.subject_height();
        resident.camera_pose = Some(resolve_mounted_player_camera_pose(
            resident.world_transform,
            resident.view,
            camera_heights,
        )?);
        Ok(())
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
        self.glue_character = None;
        self.creatures_resident.clear();
        self.remote_players.clear();
        self.models.collect_unused();
        self.textures.collect_unused();
    }
}

#[derive(Clone, Debug, PartialEq)]
enum ResidentGlueCharacterKey {
    Creation(UiCharacterCreationPreview),
    Selection(Box<UiCharacterSelectionPreview>),
}

struct ResidentGlueCharacterModel {
    key: ResidentGlueCharacterKey,
    model: Arc<DecodedM2Model>,
    textures: Vec<ResidentPlayerTexture>,
    atlas: CharacterAtlasTexture,
    geosets: CharacterGeosetPlan,
    animation: UnitModelAnimation,
    model_scale: f32,
    facing_radians: f32,
    attachments: Vec<ResidentPlayerAttachment>,
    particle_colors: Option<M2ParticleColorReplacement>,
}

/// Borrowed character-creation body passed into the Glue M2 compositor.
pub(super) struct ResidentCreationFrameInput<'a> {
    model: &'a Arc<DecodedM2Model>,
    textures: &'a [ResidentPlayerTexture],
    atlas: &'a CharacterAtlasTexture,
    geosets: &'a CharacterGeosetPlan,
    animation: UnitModelAnimation,
    model_scale: f32,
    facing_radians: f32,
    attachments: &'a [ResidentPlayerAttachment],
    particle_colors: Option<&'a M2ParticleColorReplacement>,
}

impl<'a> ResidentCreationFrameInput<'a> {
    fn from_resident(resident: &'a ResidentGlueCharacterModel) -> Self {
        Self {
            model: &resident.model,
            textures: &resident.textures,
            atlas: &resident.atlas,
            geosets: &resident.geosets,
            animation: resident.animation,
            model_scale: resident.model_scale,
            facing_radians: resident.facing_radians,
            attachments: &resident.attachments,
            particle_colors: resident.particle_colors.as_ref(),
        }
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

    pub(super) const fn animation(&self) -> UnitModelAnimation {
        self.animation
    }

    pub(super) const fn model_scale(&self) -> f32 {
        self.model_scale
    }

    pub(super) const fn facing_radians(&self) -> f32 {
        self.facing_radians
    }

    pub(super) const fn particle_colors(&self) -> Option<&M2ParticleColorReplacement> {
        self.particle_colors
    }

    pub(super) const fn attachments(&self) -> &[ResidentPlayerAttachment] {
        self.attachments
    }
}

struct ResidentPlayerModel {
    guid: u64,
    object_scale: f32,
    collision_extent: [f32; 2],
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
    view: PlayerViewState,
    animation: UnitModelAnimation,
    camera_height: CameraSubjectHeight,
    camera_height_state: PlayerCameraHeightState,
    camera_time_ms: f32,
    camera_pose: Option<PlayerCameraPose>,
    model: Arc<DecodedM2Model>,
    mount_key: Option<MountModelKey>,
    mount: Option<ResidentMountModel>,
}

struct DesiredRemotePlayerModel {
    guid: u64,
    object_scale: f32,
    collision_extent: [f32; 2],
    particle_color_id: u32,
    path: AssetPath,
    base_texture_plan: CharacterTexturePlan,
    base_geosets: CharacterGeosetPlan,
    equipment_key: Vec<(PlayerEquipmentSlot, VisibleEquipmentItem)>,
    attachment_plan: CharacterAttachmentPlan,
    world_transform: WorldTransform,
    requested_animation: UnitLocomotionAnimation,
    animation_tier: solarity_ecs::UnitAnimationTier,
    mount_key: Option<MountModelKey>,
}

/// Stable DBC/model identity which invalidates one resident mount generation.
#[derive(Clone, PartialEq)]
struct MountModelKey {
    display_id: u32,
    path: AssetPath,
    object_scale: f32,
    particle_color_id: u32,
    mount_height: f32,
}

/// Archive-selected mount model and its independently animated presentation.
struct ResidentMountModel {
    model: Arc<DecodedM2Model>,
    textures: Vec<ResidentCreatureTexture>,
    object_scale: f32,
    particle_colors: Option<M2ParticleColorReplacement>,
    animation: UnitModelAnimation,
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
    geosets: Option<CharacterGeosetPlan>,
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
    mount: Option<ResidentMountFrameInput<'a>>,
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
            mount: resident
                .mount
                .as_ref()
                .map(ResidentMountFrameInput::from_resident),
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

    pub(super) const fn mount(&self) -> Option<ResidentMountFrameInput<'a>> {
        self.mount
    }
}

/// Borrowed mount resources composed beneath one resident rider.
#[derive(Clone, Copy)]
pub(super) struct ResidentMountFrameInput<'a> {
    model: &'a Arc<DecodedM2Model>,
    textures: &'a [ResidentCreatureTexture],
    object_scale: f32,
    animation: UnitModelAnimation,
    particle_colors: Option<&'a M2ParticleColorReplacement>,
}

impl<'a> ResidentMountFrameInput<'a> {
    fn from_resident(resident: &'a ResidentMountModel) -> Self {
        Self {
            model: &resident.model,
            textures: &resident.textures,
            object_scale: resident.object_scale,
            animation: resident.animation,
            particle_colors: resident.particle_colors.as_ref(),
        }
    }

    pub(super) const fn model(self) -> &'a Arc<DecodedM2Model> {
        self.model
    }

    pub(super) const fn textures(self) -> &'a [ResidentCreatureTexture] {
        self.textures
    }

    pub(super) const fn object_scale(self) -> f32 {
        self.object_scale
    }

    pub(super) const fn animation(self) -> UnitModelAnimation {
        self.animation
    }

    pub(super) const fn particle_colors(self) -> Option<&'a M2ParticleColorReplacement> {
        self.particle_colors
    }
}

/// Borrowed immutable inputs required to publish one visible creature M2.
pub(super) struct ResidentCreatureFrameInput<'a> {
    guid: u64,
    model: &'a Arc<DecodedM2Model>,
    textures: &'a [ResidentCreatureTexture],
    geosets: Option<&'a CharacterGeosetPlan>,
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
            geosets: resident.geosets.as_ref(),
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

    pub(super) const fn geosets(&self) -> Option<&CharacterGeosetPlan> {
        self.geosets
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

/// Resolves stock starter-outfit rows through the same item tables as live gear.
fn resolve_creation_equipment<'catalog>(
    preview: &UiCharacterCreationPreview,
    outfits: &CharacterStartOutfitCatalog,
    definitions: &'catalog ItemDefinitionCatalog,
    displays: &'catalog ItemDisplayCatalog,
) -> Result<Vec<CharacterEquipmentItem<'catalog>>, RuntimePlayerError> {
    let outfit = outfits
        .outfit(preview.race_id(), preview.class_id(), preview.gender_id())
        .ok_or(RuntimePlayerError::MissingCreationOutfit {
            race_id: preview.race_id(),
            class_id: preview.class_id(),
            gender_id: preview.gender_id(),
        })?;
    let mut occupied = [false; PLAYER_EQUIPMENT_SLOT_COUNT];
    let mut equipment = Vec::new();
    for outfit_item in outfit.items() {
        let Ok(item_id) = u32::try_from(outfit_item.item_id()) else {
            continue;
        };
        if item_id == 0 {
            continue;
        }
        let definition = definitions
            .item(item_id)
            .ok_or(RuntimePlayerError::MissingCreationOutfitItem { item_id })?;
        if outfit_item.display_info_id() != definition.display_info_id() as i32 {
            return Err(RuntimePlayerError::CreationOutfitDisplayMismatch {
                item_id,
                outfit_display_id: outfit_item.display_info_id(),
                item_display_id: definition.display_info_id(),
            });
        }
        if outfit_item.inventory_type() != definition.inventory_type() as i32 {
            return Err(RuntimePlayerError::CreationOutfitInventoryMismatch {
                item_id,
                outfit_inventory_type: outfit_item.inventory_type(),
                item_inventory_type: definition.inventory_type() as u32,
            });
        }
        let Some(slot) = creation_equipment_slot(definition.inventory_type(), &occupied) else {
            continue;
        };
        let display_id = definition.display_info_id();
        let display = displays.display(display_id).ok_or(
            RuntimePlayerError::MissingCreationOutfitDisplay {
                item_id,
                display_id,
            },
        )?;
        occupied[slot.index()] = true;
        equipment.push(CharacterEquipmentItem::new_visible(
            slot,
            VisibleEquipmentItem::new(item_id, 0),
            definition,
            display,
        ));
    }
    Ok(equipment)
}

/// Resolves the first nineteen display-only character-enumeration slots.
fn resolve_selection_equipment<'catalog>(
    preview: &UiCharacterSelectionPreview,
    displays: &'catalog ItemDisplayCatalog,
) -> Result<Vec<CharacterEquipmentItem<'catalog>>, RuntimePlayerError> {
    PlayerEquipmentSlot::ALL
        .into_iter()
        .zip(preview.equipment())
        .enumerate()
        .filter(|(_, (_, item))| item.display_id() != 0)
        .map(|(slot, (equipment_slot, item))| {
            let inventory_type = InventoryType::try_from(u32::from(item.inventory_type_id()))
                .map_err(
                    |_source| RuntimePlayerError::InvalidSelectionInventoryType {
                        slot,
                        inventory_type_id: item.inventory_type_id(),
                    },
                )?;
            let display = displays.display(item.display_id()).ok_or(
                RuntimePlayerError::MissingSelectionItemDisplay {
                    slot,
                    display_id: item.display_id(),
                },
            )?;
            Ok(CharacterEquipmentItem::new_selection(
                equipment_slot,
                display,
                inventory_type,
                item.enchantment_visual_id(),
            ))
        })
        .collect()
}

/// Maps an inventory type to the first compatible public equipment slot.
fn creation_equipment_slot(
    inventory_type: InventoryType,
    occupied: &[bool; PLAYER_EQUIPMENT_SLOT_COUNT],
) -> Option<PlayerEquipmentSlot> {
    let first_available =
        |slots: &[PlayerEquipmentSlot]| slots.iter().copied().find(|slot| !occupied[slot.index()]);
    match inventory_type {
        InventoryType::Head => first_available(&[PlayerEquipmentSlot::Head]),
        InventoryType::Neck => first_available(&[PlayerEquipmentSlot::Neck]),
        InventoryType::Shoulders => first_available(&[PlayerEquipmentSlot::Shoulders]),
        InventoryType::Body => first_available(&[PlayerEquipmentSlot::Shirt]),
        InventoryType::Chest | InventoryType::Robe => {
            first_available(&[PlayerEquipmentSlot::Chest])
        }
        InventoryType::Waist => first_available(&[PlayerEquipmentSlot::Waist]),
        InventoryType::Legs => first_available(&[PlayerEquipmentSlot::Legs]),
        InventoryType::Feet => first_available(&[PlayerEquipmentSlot::Feet]),
        InventoryType::Wrists => first_available(&[PlayerEquipmentSlot::Wrists]),
        InventoryType::Hands => first_available(&[PlayerEquipmentSlot::Hands]),
        InventoryType::Finger => first_available(&[
            PlayerEquipmentSlot::FingerOne,
            PlayerEquipmentSlot::FingerTwo,
        ]),
        InventoryType::Trinket => first_available(&[
            PlayerEquipmentSlot::TrinketOne,
            PlayerEquipmentSlot::TrinketTwo,
        ]),
        InventoryType::Weapon => {
            first_available(&[PlayerEquipmentSlot::MainHand, PlayerEquipmentSlot::OffHand])
        }
        InventoryType::Shield | InventoryType::OffHandWeapon | InventoryType::Holdable => {
            first_available(&[PlayerEquipmentSlot::OffHand])
        }
        InventoryType::Ranged
        | InventoryType::Thrown
        | InventoryType::RangedRight
        | InventoryType::Relic => first_available(&[PlayerEquipmentSlot::Ranged]),
        InventoryType::Cloak => first_available(&[PlayerEquipmentSlot::Back]),
        InventoryType::TwoHandWeapon | InventoryType::MainHandWeapon => {
            first_available(&[PlayerEquipmentSlot::MainHand])
        }
        InventoryType::Tabard => first_available(&[PlayerEquipmentSlot::Tabard]),
        InventoryType::NonEquip
        | InventoryType::Bag
        | InventoryType::Ammo
        | InventoryType::Quiver => None,
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

/// Resolves the eleven display-only armor slots carried by an NPC appearance.
fn resolve_npc_equipment<'catalog>(
    creature_display_id: u32,
    display_ids: [u32; 11],
    displays: &'catalog ItemDisplayCatalog,
) -> Result<Vec<CharacterEquipmentItem<'catalog>>, RuntimePlayerError> {
    NPC_EQUIPMENT_SLOTS
        .into_iter()
        .zip(display_ids)
        .filter(|(_, display_id)| *display_id != 0)
        .map(|(slot, display_id)| {
            let display =
                displays
                    .display(display_id)
                    .ok_or(RuntimePlayerError::MissingNpcItemDisplay {
                        display_id: creature_display_id,
                        item_display_id: display_id,
                    })?;
            Ok(CharacterEquipmentItem::new_npc(slot, display))
        })
        .collect()
}

/// Binds a player-model NPC's baked atlas and remaining special textures.
fn prepare_npc_character_textures(
    model: &DecodedM2Model,
    appearance: &CreatureModelAppearance<'_>,
    plan: &CharacterTexturePlan,
    assets: &mut solarity_asset::AssetStore,
    textures: &mut BlpTextureCache,
) -> Result<Vec<ResidentCreatureTexture>, RuntimePlayerError> {
    let baked_path =
        appearance
            .baked_texture()
            .ok_or(RuntimePlayerError::MissingNpcBakedTexture {
                display_id: appearance.display().id(),
            })?;
    let baked = textures.load(assets, baked_path)?;
    let hair = load_optional_texture(plan.hair(), assets, textures)?;
    let extra_skin = load_optional_texture(plan.extra_skin(), assets, textures)?;
    let cape = load_optional_texture(plan.cape(), assets, textures)?;

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
            M2TextureKind::Body => Ok(ResidentCreatureTexture::Authored(Arc::clone(&baked))),
            M2TextureKind::Environment => Ok(hair.as_ref().map_or(
                ResidentCreatureTexture::Unresolved(M2TextureKind::Environment),
                |source| ResidentCreatureTexture::Authored(Arc::clone(source)),
            )),
            M2TextureKind::SkinExtra => Ok(extra_skin.as_ref().map_or(
                ResidentCreatureTexture::Unresolved(M2TextureKind::SkinExtra),
                |source| ResidentCreatureTexture::Authored(Arc::clone(source)),
            )),
            M2TextureKind::Item => Ok(cape.as_ref().map_or(
                ResidentCreatureTexture::Unresolved(M2TextureKind::Item),
                |source| ResidentCreatureTexture::Authored(Arc::clone(source)),
            )),
            kind => Ok(ResidentCreatureTexture::Unresolved(kind)),
        })
        .collect()
}

/// Captures the complete display/model identity and stock base-scale product.
fn mount_model_key(appearance: &CreatureModelAppearance<'_>, object_scale: f32) -> MountModelKey {
    let authored_scale = appearance.display().model_scale() * appearance.model().model_scale();
    // `CGUnit_C::GetModelScale` replaces a non-positive authored product with
    // one. The authoritative OBJECT_FIELD_SCALE_X remains an independent
    // instance multiplier applied after that DBC result.
    let model_scale = if authored_scale > 0.0 {
        authored_scale
    } else {
        1.0
    };
    MountModelKey {
        display_id: appearance.display().id(),
        path: appearance.model_path().clone(),
        object_scale: object_scale * model_scale,
        particle_color_id: appearance.display().particle_color_id(),
        mount_height: appearance.model().mount_height(),
    }
}

/// Loads one active mount without substituting body textures or animation.
#[allow(clippy::too_many_arguments)]
fn load_mount_model(
    appearance: Option<&CreatureModelAppearance<'_>>,
    object_scale: f32,
    requested_animation: UnitLocomotionAnimation,
    animation_tier: solarity_ecs::UnitAnimationTier,
    animations: &AnimationDataCatalog,
    particle_colors: &ParticleColorCatalog,
    models: &mut M2ModelCache,
    textures: &mut BlpTextureCache,
    assets: &mut solarity_asset::AssetStore,
) -> Result<Option<ResidentMountModel>, RuntimePlayerError> {
    let Some(appearance) = appearance else {
        return Ok(None);
    };
    let key = mount_model_key(appearance, object_scale);
    let model = models.load(assets, appearance.model_path())?;
    let textures = prepare_creature_textures(&model, appearance, assets, textures)?;
    let animation =
        resolve_resident_animation(animations, &model, requested_animation, animation_tier)?;
    Ok(Some(ResidentMountModel {
        model,
        textures,
        object_scale: key.object_scale,
        particle_colors: M2ParticleColorReplacement::resolve(
            particle_colors,
            key.particle_color_id,
        ),
        animation,
    }))
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
    let Some(path) = path else {
        return Ok(None);
    };
    if !assets.contains(path)? {
        return Ok(None);
    }
    textures.load(assets, path).map(Some)
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
            && self.mount_key == desired.mount_key
    }
}
