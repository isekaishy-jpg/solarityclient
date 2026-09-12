//! Retained NPC appearance and model admission.

use super::population_worker::{PopulationLoading, PopulationRequest};

use super::{
    ActiveWorld, AnimationDataCatalog, CharacterAttachmentPlan, CharacterCustomization,
    CharacterEquipmentItem, CharacterGeosetContext, CharacterGeosetPlan, CharacterTabardMode,
    CharacterTexturePlan, CreatureAppearanceInputs, CreatureModelKey, DesiredCreatureModel,
    M2ParticleColorReplacement, PlayerEquipmentSlot, ResidentCreatureGeosets,
    ResidentCreatureModel, RuntimeCreaturePoll, RuntimePlayerError, RuntimePlayerPresentation,
    UnitAnimationTier, UnitLocomotionAnimation, UnitModelAppearanceError,
    UnitPresentationGeneration, VisibleEquipmentItem, WorldObjectIdentity, WorldTransform,
    load_mount_model, load_player_attachments, mount_model_key, prepare_creature_textures,
    prepare_npc_character_textures, resolve_npc_equipment, resolve_resident_animation,
    resolve_unit_locomotion_animation, resolve_unit_model,
};

impl RuntimePlayerPresentation {
    /// Synchronizes every visible non-player unit into shared M2 residency.
    ///
    /// Player objects require character atlas and equipment composition and
    /// remain on the dedicated player path. This pass admits creature objects
    /// only after their complete display, transform, and tier state exists.
    /// `template_for` supplies family and flags bound to each exact lifetime;
    /// its arrival and later level/pet changes update the authored body scale.
    pub fn synchronize_creatures(
        &mut self,
        world: Option<&ActiveWorld>,
        template_for: impl Fn(WorldObjectIdentity) -> Option<(u32, u32)>,
    ) -> Result<RuntimeCreaturePoll, RuntimePlayerError> {
        self.synchronize_creatures_inner(world, template_for, PopulationLoading::Synchronous)
    }

    /// Admits finite NPC asset jobs without borrowing the live ECS on workers.
    pub(in crate::application) fn synchronize_creatures_async(
        &mut self,
        world: Option<&ActiveWorld>,
        template_for: impl Fn(WorldObjectIdentity) -> Option<(u32, u32)>,
        cpu: &solarity_cpu::CpuExecutor,
        renderer: &mut solarity_rendering::VulkanRenderer,
    ) -> Result<RuntimeCreaturePoll, RuntimePlayerError> {
        self.creature_worker.service(world, cpu)?;
        self.synchronize_creatures_inner(
            world,
            template_for,
            PopulationLoading::Asynchronous { cpu, renderer },
        )
    }

    /// The live path resolves at most one changed NPC's admission inputs per service.
    fn synchronize_creatures_inner(
        &mut self,
        world: Option<&ActiveWorld>,
        template_for: impl Fn(WorldObjectIdentity) -> Option<(u32, u32)>,
        mut loading: PopulationLoading<'_>,
    ) -> Result<RuntimeCreaturePoll, RuntimePlayerError> {
        let Some(world) = world else {
            self.creatures_resident.clear();
            self.unit_animations.clear();
            self.models.collect_unused();
            self.textures.collect_unused();
            return Ok(RuntimeCreaturePoll::Idle);
        };

        let mut desired = Vec::new();
        let mut admission = None;
        let mut retained = Vec::with_capacity(self.creatures_resident.len());
        for &guid in world.visible_unit_guids() {
            if world.object_kind(guid) != Some(solarity_ecs::ObjectKind::Unit) {
                continue;
            }
            let Some(identity) = world.object_identity(guid) else {
                continue;
            };
            let Some(transform) = world.object_transform(guid) else {
                continue;
            };
            let Some(presentation) = world.unit_presentation(guid) else {
                continue;
            };
            let requested_animation = world.movement_state(guid).map_or(
                UnitLocomotionAnimation::STAND,
                resolve_unit_locomotion_animation,
            );
            let template = template_for(identity);
            let previous_index = self
                .creatures_resident
                .binary_search_by_key(&guid, |resident| resident.key.guid)
                .ok()
                .filter(|&index| self.creatures_resident[index].key.identity == identity);
            let previous = previous_index.map(|index| &self.creatures_resident[index]);
            let body_definition = self
                .unit_animations
                .get(guid)
                .filter(|animation| animation.identity() == identity)
                .map_or_else(
                    || {
                        self.animations
                            .definition(u32::from(requested_animation.animation_id()))
                    },
                    |animation| animation.current_body_definition(),
                );
            let inputs = CreatureAppearanceInputs::read(
                world,
                guid,
                template,
                body_definition.map(|definition| definition.id()),
                previous.map(|resident| (resident.weapon_state, resident.stand_state)),
            );
            if let Some(index) = previous_index
                && inputs.is_some()
                && self.creatures_resident[index].inputs == inputs
            {
                self.creature_worker.discard_ready(identity)?;
                self.creatures_resident[index].update_motion(
                    transform,
                    requested_animation,
                    presentation.animation_tier(),
                    &self.animations,
                )?;
                retained.push(guid);
                continue;
            }
            if loading.is_asynchronous() {
                if !self.creature_worker.accepts(identity)
                    || admission.is_some_and(|selected| selected != identity)
                {
                    if let Some(index) = previous_index {
                        self.creatures_resident[index].update_motion(
                            transform,
                            requested_animation,
                            presentation.animation_tier(),
                            &self.animations,
                        )?;
                        retained.push(guid);
                    }
                    continue;
                }
                admission = Some(identity);
            }
            let appearance =
                match resolve_unit_model(world, guid, &self.creatures, &self.characters) {
                    Ok(appearance) => appearance,
                    Err(
                        UnitModelAppearanceError::MissingObjectPresentation { .. }
                        | UnitModelAppearanceError::MissingUnitPresentation { .. },
                    ) => continue,
                    Err(error) => return Err(error.into()),
                };
            let family = template.and_then(|(id, _flags)| self.creature_families.family(id));
            let Some(body_scale) = solarity_systems::resolve_unit_body_scale(
                world,
                guid,
                &self.creatures,
                &self.races,
                family,
            ) else {
                continue;
            };
            let virtual_entries = world.unit_virtual_items(guid).unwrap_or_default().entries();
            let virtual_definitions = previous
                .filter(|resident| resident.key.virtual_entries == virtual_entries)
                .map_or_else(
                    || {
                        virtual_entries.map(|entry| {
                            (entry != 0)
                                .then(|| self.item_definitions.item(entry))
                                .flatten()
                                .filter(|definition| definition.display_info_id() != 0)
                                .copied()
                        })
                    },
                    |resident| resident.virtual_definitions,
                );
            let weapon_state = solarity_rendering::NpcWeaponState::new(
                presentation.sheath_state(),
                world.unit_flags(guid).unwrap_or_default(),
                appearance.body().model().flags(),
                body_definition
                    .and_then(|definition| u16::try_from(definition.behavior_id()).ok())
                    .unwrap_or(506),
            )
            .reconcile(
                previous.map_or(presentation.sheath_state(), |resident| {
                    resident.weapon_state.sheath_state()
                }),
                solarity_rendering::NpcWeaponAnimationInput {
                    animation_id: body_definition.map(solarity_asset::AnimationDataDefinition::id),
                    weapon_flags: body_definition
                        .map_or(0, solarity_asset::AnimationDataDefinition::weapon_flags),
                    has_attack_target: world.unit_attack_target(guid) != 0,
                    template_flags: template.map_or(0, |(_family, flags)| flags),
                    changed_stand_state: previous
                        .filter(|resident| resident.stand_state != presentation.stand_state())
                        .map(|_| presentation.stand_state()),
                },
                [
                    virtual_definitions[0].as_ref(),
                    virtual_definitions[1].as_ref(),
                ],
            );
            desired.push(DesiredCreatureModel {
                inputs,
                key: CreatureModelKey {
                    identity,
                    guid,
                    display_id: appearance.body().display().id(),
                    path: appearance.body().model_path().clone(),
                    object_scale: body_scale * appearance.object_scale(),
                    particle_color_id: appearance.body().display().particle_color_id(),
                    virtual_entries,
                    weapon_state: virtual_entries
                        .iter()
                        .any(|entry| *entry != 0)
                        .then_some(weapon_state),
                    mount_key: appearance
                        .mount()
                        .map(|mount| mount_model_key(mount, body_scale, appearance.object_scale())),
                },
                transform,
                requested_animation,
                animation_tier: presentation.animation_tier(),
                weapon_state,
                stand_state: presentation.stand_state(),
                virtual_definitions,
            });
        }

        let mut residents = Vec::with_capacity(desired.len());
        for desired in desired {
            if let Ok(index) = self
                .creatures_resident
                .binary_search_by_key(&desired.key.guid, |resident| resident.key.guid)
                && self.creatures_resident[index].key == desired.key
            {
                self.creature_worker.discard_ready(desired.key.identity)?;
                let resident = &mut self.creatures_resident[index];
                resident.inputs = desired.inputs;
                resident.world_transform = desired.transform;
                resident.weapon_state = desired.weapon_state;
                resident.stand_state = desired.stand_state;
                if let Some(mount) = resident.mount.as_mut() {
                    mount.animation = resolve_resident_animation(
                        &self.animations,
                        &mount.model,
                        desired.requested_animation,
                        desired.animation_tier,
                    )?;
                }
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
                retained.push(desired.key.guid);
                continue;
            }
            if let PopulationLoading::Asynchronous { cpu, renderer } = &mut loading {
                if let Some(mut resident) = self.creature_worker.take_ready(
                    &desired.key,
                    self.component_texture_level,
                    renderer,
                )? {
                    resident.inputs = desired.inputs;
                    resident.weapon_state = desired.weapon_state;
                    resident.stand_state = desired.stand_state;
                    resident.update_motion(
                        desired.transform,
                        desired.requested_animation,
                        desired.animation_tier,
                        &self.animations,
                    )?;
                    residents.push(resident);
                } else {
                    // The active generation owns reconciliation state until its
                    // complete replacement commits. Movement still advances.
                    if let Ok(index) = self
                        .creatures_resident
                        .binary_search_by_key(&desired.key.guid, |resident| resident.key.guid)
                        && self.creatures_resident[index].key.identity == desired.key.identity
                    {
                        self.creatures_resident[index].update_motion(
                            desired.transform,
                            desired.requested_animation,
                            desired.animation_tier,
                            &self.animations,
                        )?;
                        retained.push(desired.key.guid);
                    }
                    let catalog = self
                        .glue_worker_catalog
                        .as_ref()
                        .ok_or(RuntimePlayerError::MissingPopulationWorkerCatalog)?
                        .clone();
                    let catalogs = self.shared_catalogs();
                    self.creature_worker.submit(
                        cpu,
                        PopulationRequest {
                            identity: desired.key.identity,
                            key: desired.key.clone(),
                            level: self.component_texture_level,
                        },
                        catalog,
                        catalogs,
                        move |presentation| presentation.load_creature(desired),
                    )?;
                }
            } else {
                residents.push(self.load_creature(desired)?);
            }
        }
        let changed = !residents.is_empty() || retained.len() != self.creatures_resident.len();
        if changed {
            retained.sort_unstable();
            if loading.is_asynchronous() {
                self.creature_worker
                    .retire(self.creatures_resident.extract_if(.., |resident| {
                        retained.binary_search(&resident.key.guid).is_err()
                    }));
            } else {
                self.creatures_resident
                    .retain(|resident| retained.binary_search(&resident.key.guid).is_ok());
            }
            self.creatures_resident.extend(residents);
            self.creatures_resident
                .sort_unstable_by_key(|resident| resident.key.guid);
        }
        self.synchronize_replicated_animations(world, solarity_ecs::ObjectKind::Unit);
        if changed {
            self.models.collect_unused();
            self.textures.collect_unused();
            Ok(RuntimeCreaturePoll::ModelsChanged)
        } else {
            Ok(RuntimeCreaturePoll::Current)
        }
    }
    /// Completes one immutable NPC resource generation on its asset owner.
    pub(super) fn load_creature(
        &mut self,
        desired: DesiredCreatureModel,
    ) -> Result<ResidentCreatureModel, RuntimePlayerError> {
        let mut assets = self.assets.borrow_mut();
        let appearance = self
            .creatures
            .resolve_model(desired.key.display_id)
            .map_err(UnitModelAppearanceError::from)?;
        let model = self.models.load(&mut assets, appearance.model_path())?;
        let (textures, geosets, mut attachment_plan) = if let Some(extra) = appearance.extra() {
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
            // 730100 walks all eleven CreatureDisplayInfoExtra components
            // through 4F2830/4F2640, including separate head/shoulder M2s.
            let attachment_plan = CharacterAttachmentPlan::npc_armor(
                equipment.iter().copied(),
                &self.races,
                character.race_id(),
                character.gender_id(),
            )?;
            (
                textures,
                Some(ResidentCreatureGeosets::Character(geosets)),
                attachment_plan,
            )
        } else {
            (
                prepare_creature_textures(&model, &appearance, &mut assets, &mut self.textures)?,
                ResidentCreatureGeosets::from_packed_selector(appearance.display().geoset_data()),
                CharacterAttachmentPlan::default(),
            )
        };
        if let Some(state) = desired.key.weapon_state {
            let entries = desired.key.virtual_entries;
            let equipment = std::array::from_fn(|index| {
                if entries[index] == 0 {
                    return None;
                }
                let definition = desired.virtual_definitions[index].as_ref()?;
                let display = self.item_displays.display(definition.display_info_id())?;
                Some(CharacterEquipmentItem::new_visible(
                    [
                        PlayerEquipmentSlot::MainHand,
                        PlayerEquipmentSlot::OffHand,
                        PlayerEquipmentSlot::Ranged,
                    ][index],
                    VisibleEquipmentItem::new(entries[index], 0),
                    definition,
                    display,
                ))
            });
            attachment_plan.add_npc_held_items(
                equipment,
                state,
                [
                    desired.virtual_definitions[0].as_ref(),
                    desired.virtual_definitions[1].as_ref(),
                ],
            )?;
        }
        attachment_plan.retain_attachments(|attachment| {
            !matches!(
                attachment.slot(),
                Some(
                    PlayerEquipmentSlot::MainHand
                        | PlayerEquipmentSlot::OffHand
                        | PlayerEquipmentSlot::Ranged
                )
            ) || model.attachment(attachment.point().id()).is_some()
        });
        let attachments = load_player_attachments(
            &attachment_plan,
            &self.item_visuals,
            &self.particle_colors,
            &mut self.models,
            &mut self.textures,
            &mut assets,
        )?;
        // 73D5D0 and 717910 give every Unit_C its own mount model;
        // NPC residency preserves the same independent child as players.
        let mount_appearance = desired
            .key
            .mount_key
            .as_ref()
            .map(|key| {
                self.creatures
                    .resolve_model(key.display_id)
                    .map_err(UnitModelAppearanceError::from)
            })
            .transpose()?;
        let mount = load_mount_model(
            mount_appearance.as_ref(),
            desired.key.mount_key.as_ref(),
            desired.requested_animation,
            desired.animation_tier,
            &self.animations,
            &self.particle_colors,
            &mut self.models,
            &mut self.textures,
            &mut assets,
        )?;
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
        Ok(ResidentCreatureModel {
            inputs: desired.inputs,
            generation,
            key: desired.key,
            model,
            textures,
            geosets,
            attachments,
            armor_display_ids: appearance
                .extra()
                .map_or([0; 11], |extra| extra.npc_item_display_ids()),
            weapon_state: desired.weapon_state,
            stand_state: desired.stand_state,
            virtual_definitions: desired.virtual_definitions,
            particle_colors: M2ParticleColorReplacement::resolve(
                &self.particle_colors,
                appearance.display().particle_color_id(),
            ),
            world_transform: desired.transform,
            animation,
            mount,
        })
    }
}

/// Motion consumes current replication independently of expensive appearance inputs.
impl ResidentCreatureModel {
    /// Keeps mount and rider animation selection live while appearance is retained.
    fn update_motion(
        &mut self,
        transform: WorldTransform,
        requested: UnitLocomotionAnimation,
        tier: UnitAnimationTier,
        animations: &AnimationDataCatalog,
    ) -> Result<(), RuntimePlayerError> {
        self.world_transform = transform;
        if let Some(mount) = self.mount.as_mut() {
            mount.animation =
                resolve_resident_animation(animations, &mount.model, requested, tier)?;
        }
        self.animation = resolve_resident_animation(
            animations,
            &self.model,
            if self.mount.is_some() {
                UnitLocomotionAnimation::MOUNT
            } else {
                requested
            },
            tier,
        )?;
        Ok(())
    }
}
