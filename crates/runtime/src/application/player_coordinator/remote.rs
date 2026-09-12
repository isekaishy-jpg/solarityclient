//! Retained remote character appearance and model admission.

use super::population_worker::{PopulationLoading, PopulationRequest};

use super::{
    ActiveWorld, AnimationDataCatalog, AssetPath, CharacterAttachmentPlan, CharacterCustomization,
    CharacterEquipmentItem, CharacterGeosetContext, CharacterGeosetPlan, CharacterTabardMode,
    CharacterTexturePlan, CharacterWeaponState, DesiredRemotePlayerModel, M2ModelCache,
    M2ParticleColorReplacement, OptionalTextureBinding, PlayerCameraHeightState,
    PlayerEquipmentAppearanceError, PlayerViewState, RemoteAppearanceInputs, ResidentPlayerModel,
    RuntimePlayerError, RuntimePlayerPresentation, RuntimeRemotePlayerPoll, UnitAnimationTier,
    UnitLocomotionAnimation, UnitModelAppearanceError, UnitPresentationGeneration, WorldTransform,
    load_mount_model, load_optional_texture, load_player_attachments, mount_model_key,
    prepare_model_textures, resolve_model_camera_subject_height, resolve_player_equipment,
    resolve_resident_animation, resolve_unit_locomotion_animation, resolve_unit_model,
};

impl RuntimePlayerPresentation {
    /// Synchronizes every visible non-local player through character composition.
    pub fn synchronize_remote_players(
        &mut self,
        world: Option<&ActiveWorld>,
    ) -> Result<RuntimeRemotePlayerPoll, RuntimePlayerError> {
        self.synchronize_remote_players_inner(world, PopulationLoading::Synchronous)
    }

    /// Keeps archive decoding and character atlas composition off presentation.
    pub(in crate::application) fn synchronize_remote_players_async(
        &mut self,
        world: Option<&ActiveWorld>,
        cpu: &solarity_cpu::CpuExecutor,
        renderer: &mut solarity_rendering::VulkanRenderer,
    ) -> Result<RuntimeRemotePlayerPoll, RuntimePlayerError> {
        self.remote_worker.service(world, cpu)?;
        self.synchronize_remote_players_inner(
            world,
            PopulationLoading::Asynchronous { cpu, renderer },
        )
    }

    /// Synchronous callers explicitly own loading; the live client supplies its executor.
    fn synchronize_remote_players_inner(
        &mut self,
        world: Option<&ActiveWorld>,
        mut loading: PopulationLoading<'_>,
    ) -> Result<RuntimeRemotePlayerPoll, RuntimePlayerError> {
        let Some(world) = world else {
            self.remote_players.clear();
            self.unit_animations.clear();
            self.models.collect_unused();
            self.textures.collect_unused();
            return Ok(RuntimeRemotePlayerPoll::Idle);
        };
        let local_guid = world.local_player_guid()?;
        let mut retained = Vec::with_capacity(self.remote_players.len());
        let mut residents = Vec::new();
        let mut admission = None;
        for &guid in world.visible_unit_guids() {
            if guid == local_guid
                || world.object_kind(guid) != Some(solarity_ecs::ObjectKind::Player)
            {
                continue;
            }
            let inputs = RemoteAppearanceInputs::read(world, guid);
            let previous = self
                .remote_players
                .binary_search_by_key(&guid, |resident| resident.guid)
                .ok();
            if let Some(index) = previous
                && inputs.is_some()
                && self.remote_players[index].remote_inputs == inputs
            {
                self.remote_worker
                    .discard_ready(self.remote_players[index].identity)?;
                let resident = &mut self.remote_players[index];
                let Some(transform) = world.object_transform(guid) else {
                    continue;
                };
                let Some(presentation) = world.unit_presentation(guid) else {
                    continue;
                };
                resident.update_remote_pose(
                    transform,
                    world.movement_state(guid).map_or(
                        UnitLocomotionAnimation::STAND,
                        resolve_unit_locomotion_animation,
                    ),
                    presentation.animation_tier(),
                    &self.animations,
                )?;
                retained.push(guid);
                continue;
            }
            if loading.is_asynchronous() {
                let identity = world.object_identity(guid);
                if identity.is_some_and(|identity| !self.remote_worker.accepts(identity))
                    || admission.is_some_and(|selected| Some(selected) != identity)
                {
                    if self.retain_remote_pose(world, guid, previous)? {
                        retained.push(guid);
                    }
                    continue;
                }
                admission = identity;
            }
            let Some(desired) = self.resolve_desired_remote_player(world, guid)? else {
                continue;
            };
            if let Some(index) = previous
                && self.remote_players[index].matches_remote(&desired)
            {
                self.remote_worker
                    .discard_ready(self.remote_players[index].identity)?;
                let resident = &mut self.remote_players[index];
                resident.update_remote_motion(&desired, &self.animations)?;
                resident.remote_inputs = inputs;
                retained.push(guid);
            } else {
                let inputs =
                    inputs.ok_or(RuntimePlayerError::MissingCharacterAppearance { guid })?;
                if let PopulationLoading::Asynchronous { cpu, renderer } = &mut loading {
                    if let Some(mut resident) = self.remote_worker.take_ready(
                        &inputs,
                        self.component_texture_level,
                        renderer,
                    )? {
                        resident.update_remote_motion(&desired, &self.animations)?;
                        residents.push(resident);
                    } else {
                        // Replacement admission keeps the active generation and
                        // its independent movement clocks until publication.
                        if self.retain_remote_pose(world, guid, previous)? {
                            retained.push(guid);
                        }
                        let catalog = self
                            .glue_worker_catalog
                            .as_ref()
                            .ok_or(RuntimePlayerError::MissingPopulationWorkerCatalog)?
                            .clone();
                        let catalogs = self.shared_catalogs();
                        self.remote_worker.submit(
                            cpu,
                            PopulationRequest {
                                identity: desired.identity,
                                key: inputs,
                                level: self.component_texture_level,
                            },
                            catalog,
                            catalogs,
                            move |presentation| presentation.load_remote_player(desired, inputs),
                        )?;
                    }
                } else {
                    residents.push(self.load_remote_player(desired, inputs)?);
                }
            }
        }
        let changed = !residents.is_empty() || retained.len() != self.remote_players.len();
        if changed {
            if loading.is_asynchronous() {
                self.remote_worker
                    .retire(self.remote_players.extract_if(.., |resident| {
                        retained.binary_search(&resident.guid).is_err()
                    }));
            } else {
                self.remote_players
                    .retain(|resident| retained.binary_search(&resident.guid).is_ok());
            }
            self.remote_players.extend(residents);
            self.remote_players
                .sort_unstable_by_key(|resident| resident.guid);
        }
        self.synchronize_replicated_animations(world, solarity_ecs::ObjectKind::Player);
        if changed {
            self.models.collect_unused();
            self.textures.collect_unused();
            Ok(RuntimeRemotePlayerPoll::ModelsChanged)
        } else {
            Ok(RuntimeRemotePlayerPoll::Current)
        }
    }

    fn retain_remote_pose(
        &mut self,
        world: &ActiveWorld,
        guid: u64,
        index: Option<usize>,
    ) -> Result<bool, RuntimePlayerError> {
        let Some(resident) = index.map(|index| &mut self.remote_players[index]) else {
            return Ok(false);
        };
        if world.object_identity(guid) != Some(resident.identity) {
            return Ok(false);
        }
        let Some(transform) = world.object_transform(guid) else {
            return Ok(false);
        };
        let Some(presentation) = world.unit_presentation(guid) else {
            return Ok(false);
        };
        resident.update_remote_pose(
            transform,
            world.movement_state(guid).map_or(
                UnitLocomotionAnimation::STAND,
                resolve_unit_locomotion_animation,
            ),
            presentation.animation_tier(),
            &self.animations,
        )?;
        Ok(true)
    }

    fn resolve_desired_remote_player(
        &self,
        world: &ActiveWorld,
        guid: u64,
    ) -> Result<Option<DesiredRemotePlayerModel>, RuntimePlayerError> {
        let Some(identity) = world.object_identity(guid) else {
            return Ok(None);
        };
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
        let Some(body_scale) = solarity_systems::resolve_unit_body_scale(
            world,
            guid,
            &self.creatures,
            &self.races,
            None,
        ) else {
            return Ok(None);
        };
        Ok(Some(DesiredRemotePlayerModel {
            identity,
            guid,
            object_scale: body_scale * appearance.object_scale(),
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
                .map(|mount| mount_model_key(mount, body_scale, appearance.object_scale())),
        }))
    }

    /// Builds resources solely from a validated immutable appearance snapshot.
    pub(super) fn load_remote_player(
        &mut self,
        desired: DesiredRemotePlayerModel,
        inputs: RemoteAppearanceInputs,
    ) -> Result<ResidentPlayerModel, RuntimePlayerError> {
        let appearance = inputs.appearance;
        let character = self.characters.resolve_player(
            u32::from(inputs.unit.race_id()),
            u32::from(inputs.unit.gender_id()),
            CharacterCustomization::new(
                appearance.skin_id(),
                appearance.face_id(),
                appearance.hair_style_id(),
                appearance.hair_color_id(),
                appearance.facial_hair_style_id(),
            ),
        )?;
        let class_id = inputs.unit.class_id();
        let mut equipment_items = Vec::new();
        for slot in solarity_ecs::PlayerEquipmentSlot::ALL {
            let visible = inputs.equipment.item(slot);
            if visible.entry_id() == 0 {
                continue;
            }
            let definition = self.item_definitions.item(visible.entry_id()).ok_or(
                PlayerEquipmentAppearanceError::MissingItem {
                    slot,
                    entry_id: visible.entry_id(),
                },
            )?;
            let display = self
                .item_displays
                .display(definition.display_info_id())
                .ok_or(PlayerEquipmentAppearanceError::MissingDisplay {
                    slot,
                    entry_id: visible.entry_id(),
                    display_id: definition.display_info_id(),
                })?;
            equipment_items.push(CharacterEquipmentItem::new_visible(
                slot, visible, definition, display,
            ));
        }
        let mount_appearance = (inputs.displays[2] != 0)
            .then(|| self.creatures.resolve_model(inputs.displays[2]))
            .transpose()
            .map_err(UnitModelAppearanceError::from)?;
        let mut assets = self.assets.borrow_mut();
        let model = self.models.load(&mut assets, &desired.path)?;
        let texture_plan =
            CharacterTexturePlan::equipped(&character, &assets, equipment_items.iter().copied())?;
        let geosets = CharacterGeosetPlan::equipped(
            &character,
            CharacterGeosetContext::new(class_id, CharacterTabardMode::Equipment),
            &self.helmet_visibility,
            equipment_items.iter().copied(),
        )?;
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
            &desired.attachment_plan,
            &self.item_visuals,
            &self.particle_colors,
            &mut self.models,
            &mut self.textures,
            &mut assets,
        )?;
        let mount = load_mount_model(
            mount_appearance.as_ref(),
            desired.mount_key.as_ref(),
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
        let generation = UnitPresentationGeneration::prepare(&model, &attachments, mount.as_ref())?;
        Ok(ResidentPlayerModel {
            remote_inputs: Some(inputs),
            generation,
            identity: desired.identity,
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
}

impl ResidentPlayerModel {
    fn update_remote_motion(
        &mut self,
        desired: &DesiredRemotePlayerModel,
        animations: &AnimationDataCatalog,
    ) -> Result<(), RuntimePlayerError> {
        self.update_remote_pose(
            desired.world_transform,
            desired.requested_animation,
            desired.animation_tier,
            animations,
        )
    }

    /// Advances motion without rebuilding the unchanged appearance description.
    fn update_remote_pose(
        &mut self,
        world_transform: WorldTransform,
        requested_animation: UnitLocomotionAnimation,
        animation_tier: UnitAnimationTier,
        animations: &AnimationDataCatalog,
    ) -> Result<(), RuntimePlayerError> {
        self.world_transform = world_transform;
        self.animation = resolve_resident_animation(
            animations,
            &self.model,
            if self.mount.is_some() {
                UnitLocomotionAnimation::MOUNT
            } else {
                requested_animation
            },
            animation_tier,
        )?;
        if let Some(mount) = self.mount.as_mut() {
            mount.animation = resolve_resident_animation(
                animations,
                &mount.model,
                requested_animation,
                animation_tier,
            )?;
        }
        Ok(())
    }
    pub(super) fn path(&self) -> &AssetPath {
        self.model.path()
    }

    fn matches_remote(&self, desired: &DesiredRemotePlayerModel) -> bool {
        self.guid == desired.guid
            && self.identity == desired.identity
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
