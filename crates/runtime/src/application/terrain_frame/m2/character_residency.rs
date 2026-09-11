//! Transactional character publication with retained component instances.

use super::*;
use crate::application::player_coordinator::{ResidentMountFrameInput, ResidentPlayerAttachment};
use solarity_ecs::{PlayerEquipmentSlot, VisibleEquipmentItem};

#[derive(Clone, Copy)]
pub(super) struct M2PlayerItemIdentity {
    slot: PlayerEquipmentSlot,
    visible: VisibleEquipmentItem,
}

/// Inputs committed only after every replacement GPU source has prepared.
enum RetainedCharacterState {
    Item(Option<M2PlayerItemIdentity>),
    Mount {
        key: MountModelKey,
        transform: Mat4,
        ground: Option<UnitGroundPlacement>,
    },
}

/// New resources are prepared without disturbing continuing child instances.
#[derive(Default)]
pub(super) struct M2PreparedCharacter {
    new: Vec<(usize, M2GpuSource, M2GpuPlacement)>,
    retained: Vec<(usize, usize, RetainedCharacterState)>,
    ready: Vec<(usize, M2GpuPlacement)>,
    opacity: Option<Rc<EntityOpacityOwner>>,
}

impl M2PreparedCharacter {
    pub(super) fn push(&mut self, source: M2GpuSource, placement: M2GpuPlacement) {
        if let Some(animation) = &placement.unit_animation {
            self.opacity = Some(Rc::clone(animation.opacity_owner()));
        }
        self.new
            .push((self.new.len() + self.retained.len(), source, placement));
    }

    fn retain(&mut self, index: usize, state: RetainedCharacterState) {
        self.retained
            .push((index, self.new.len() + self.retained.len(), state));
    }
}

impl M2Frame {
    /// Called only after all changed characters have prepared successfully.
    pub(super) fn retain_character_instances(&mut self, characters: &mut [M2PreparedCharacter]) {
        let mut retained = HashMap::new();
        for (character_index, character) in characters.iter_mut().enumerate() {
            self.retain_unit_effects(character.new.iter_mut().map(|(_, _, placement)| placement));
            for (index, order, state) in character.retained.drain(..) {
                let previous = retained.insert(index, (character_index, order, state));
                debug_assert!(previous.is_none(), "one owner per retained component");
            }
        }
        if retained.is_empty() {
            return;
        }
        // Detach the retained placements before retiring the old characters.
        // Their source indices stay live and retain the same GPU handles.
        for (index, mut placement) in std::mem::take(&mut self.placements).into_iter().enumerate() {
            if let Some((character, order, state)) = retained.remove(&index) {
                match state {
                    RetainedCharacterState::Item(Some(identity)) => {
                        placement.item_identity = Some(identity);
                    }
                    RetainedCharacterState::Item(None) => {}
                    RetainedCharacterState::Mount {
                        key,
                        transform,
                        ground,
                    } => {
                        placement.mount_key = Some(key);
                        placement.transform = transform;
                        placement.local_transform = transform;
                        placement.ground_placement = ground;
                    }
                }
                characters[character].ready.push((order, placement));
            } else {
                self.placements.push(placement);
            }
        }
        debug_assert!(
            retained.is_empty(),
            "retention indices came from this scene"
        );
    }

    pub(super) fn publish_character(&mut self, mut character: M2PreparedCharacter) {
        for (order, source, mut placement) in character.new {
            placement.source_index = self.sources.len();
            self.sources.push(Some(source));
            character.ready.push((order, placement));
        }
        character.ready.sort_unstable_by_key(|(order, _)| *order);
        self.placements
            .extend(character.ready.into_iter().map(|(_, mut placement)| {
                placement.entity_opacity = character.opacity.as_ref().map(Rc::clone);
                placement
            }));
    }

    fn same_unit_owner(
        &self,
        animation: Option<&Rc<UnitAnimationBehavior>>,
        body_owner: M2GpuPlacementOwner,
    ) -> bool {
        let Some(animation) = animation else {
            return false;
        };
        self.placements.iter().any(|placement| {
            placement.owner == body_owner
                && placement
                    .unit_animation
                    .as_ref()
                    .is_some_and(|previous| Rc::ptr_eq(previous, animation))
        })
    }

    fn matching_item_model(
        &self,
        guid: u64,
        attachment: &ResidentPlayerAttachment,
    ) -> Option<usize> {
        self.placements.iter().position(|placement| {
            placement.owner
                == (M2GpuPlacementOwner::PlayerItem {
                    guid,
                    point: attachment.point(),
                })
                && self
                    .sources
                    .get(placement.source_index)
                    .and_then(Option::as_ref)
                    .is_some_and(|source| source.model.path() == attachment.model().path())
        })
    }
}

/// Shared Unit_C mount inputs; the body animation identifies the unit lifetime.
pub(super) struct UnitMountGpuInput<'a> {
    pub(super) mount: ResidentMountFrameInput<'a>,
    pub(super) body_owner: M2GpuPlacementOwner,
    pub(super) world_transform: WorldTransform,
    pub(super) animation: Option<&'a Rc<UnitAnimationBehavior>>,
}

/// Prepares the independent mount before its attached player or creature body.
/// 717910 preserves its instance while the same unit and mount model remain.
pub(super) fn prepare_mount_gpu(
    frame: &M2Frame,
    renderer: &mut VulkanRenderer,
    prepared: &mut M2PreparedCharacter,
    input: UnitMountGpuInput<'_>,
    scene_time_ms: f32,
    random: &mut CrtRand,
) -> Result<(), RuntimeTerrainFrameError> {
    let mount = input.mount;
    let world_transform = unit_placement_transform(input.world_transform, mount.object_scale())?;
    let same_unit = frame.same_unit_owner(input.animation, input.body_owner);
    let owner = match input.body_owner {
        M2GpuPlacementOwner::PlayerBody { guid } => M2GpuPlacementOwner::PlayerMount { guid },
        M2GpuPlacementOwner::RemotePlayerBody { guid } => {
            M2GpuPlacementOwner::RemotePlayerMount { guid }
        }
        M2GpuPlacementOwner::CreatureBody { guid } => M2GpuPlacementOwner::CreatureMount { guid },
        _ => unreachable!("mount preparation requires a unit body owner"),
    };
    let ground = input.animation.map(|animation| UnitGroundPlacement {
        position: input.world_transform.position(),
        scale: mount.object_scale(),
        owner: Rc::clone(animation),
    });
    // 717910 leaves an unchanged mount instance intact. A character atlas
    // or equipment change does not replace that independent CM2Model.
    let retained = same_unit
        .then(|| {
            frame.placements.iter().position(|placement| {
                placement.owner == owner
                    && placement
                        .mount_key
                        .as_ref()
                        .is_some_and(|key| key.is_same_model_as(mount.key()))
            })
        })
        .flatten();
    if let Some(index) = retained {
        prepared.retain(
            index,
            RetainedCharacterState::Mount {
                key: mount.key().clone(),
                transform: world_transform,
                ground,
            },
        );
    } else {
        if mount.model().attachment(0).is_none() {
            return Err(RuntimeTerrainFrameError::MissingMountM2Attachment {
                model: mount.model().path().clone(),
                attachment_id: 0,
            });
        }
        let resolved = mount
            .textures()
            .iter()
            .map(|texture| match texture {
                ResidentCreatureTexture::Authored(source) => {
                    M2ResolvedTexture::Authored(source.as_ref())
                }
                ResidentCreatureTexture::StockWhite => M2ResolvedTexture::StockWhite,
                ResidentCreatureTexture::StockFailure => M2ResolvedTexture::StockFailure,
            })
            .collect::<Vec<_>>();
        let source = prepare_gpu_source(
            renderer,
            mount.model(),
            &resolved,
            None,
            M2LocalLightCount::Four,
            M2ModelOrientation::Authored,
        )?;
        // 73D5D0 first constructs the mount through 81F8F0. Its default
        // sequence owns native timers and weighted/cycle rolls before movement.
        let mut placement = default_gpu_placement(
            scene_time_ms,
            world_transform,
            owner,
            mount.model(),
            &frame.animations,
            mount.particle_colors().cloned(),
            random,
        )?;
        if let Some(playback) = &mut placement.playback {
            select_mount_animation(
                input.animation.map(Rc::as_ref),
                &mut playback.borrow_mut(),
                mount.model(),
                mount.animation().animation_id(),
                scene_time_ms,
                random,
            )?;
        }
        placement.ground_placement = ground;
        placement.mount_key = Some(mount.key().clone());
        // Parent-first insertion lets the current mount bone pose determine
        // the rider transform before the body and its equipment are visited.
        prepared.push(source, placement);
    }
    Ok(())
}

/// Prepares one complete player character, including equipment and visuals.
pub(super) fn prepare_character_gpu(
    frame: &M2Frame,
    renderer: &mut VulkanRenderer,
    input: &ResidentPlayerFrameInput<'_>,
    body_owner: M2GpuPlacementOwner,
    scene_time_ms: f32,
    random: &mut CrtRand,
) -> Result<M2PreparedCharacter, RuntimeTerrainFrameError> {
    let mount = input.mount();
    let world_transform = if let Some(mount) = mount {
        unit_placement_transform(input.world_transform(), mount.object_scale())?
    } else {
        unit_placement_transform(input.world_transform(), input.object_scale())?
    };
    let mut prepared = M2PreparedCharacter::default();
    let same_unit = frame.same_unit_owner(input.unit_animation(), body_owner);
    // 4EF710 admits the shoulder pair only when both live model paths match.
    let same_shoulders = same_unit
        && [
            CharacterAttachmentPoint::ShoulderLeft,
            CharacterAttachmentPoint::ShoulderRight,
        ]
        .into_iter()
        .all(|point| {
            input
                .attachments()
                .iter()
                .find(|attachment| attachment.point() == point)
                .and_then(|attachment| frame.matching_item_model(input.guid(), attachment))
                .is_some()
        });
    if let Some(mount) = mount {
        prepare_mount_gpu(
            frame,
            renderer,
            &mut prepared,
            UnitMountGpuInput {
                mount,
                body_owner,
                world_transform: input.world_transform(),
                animation: input.unit_animation(),
            },
            scene_time_ms,
            random,
        )?;
    }
    let resolved = input
        .textures()
        .iter()
        .map(|texture| match texture {
            ResidentPlayerTexture::Authored(source) => M2ResolvedTexture::Authored(source.as_ref()),
            ResidentPlayerTexture::StockWhite => M2ResolvedTexture::StockWhite,
            ResidentPlayerTexture::StockFailure => M2ResolvedTexture::StockFailure,
            ResidentPlayerTexture::BodyAtlas => M2ResolvedTexture::CharacterAtlas(input.atlas()),
            ResidentPlayerTexture::Unresolved(kind) => M2ResolvedTexture::Unresolved(*kind),
        })
        .collect::<Vec<_>>();
    let source = prepare_gpu_source(
        renderer,
        input.model(),
        &resolved,
        Some(M2GeosetSelection::Character(input.geosets())),
        M2LocalLightCount::Four,
        M2ModelOrientation::Authored,
    )?;
    let mut body = if let Some(animation) = input.unit_animation() {
        animation.synchronize(scene_time_ms as u32, random)?;
        let mut body = m2_gpu_placement(
            0,
            world_transform,
            body_owner,
            input.model(),
            Some(M2PlaybackStorage::Shared(animation.playback())),
            input.particle_colors().cloned(),
            scene_time_ms as u32,
        )?;
        body.unit_animation = Some(Rc::clone(animation));
        body.ground_placement = mount.is_none().then_some(UnitGroundPlacement {
            position: input.world_transform().position(),
            scale: input.object_scale(),
            owner: Rc::clone(animation),
        });
        body
    } else {
        unit_gpu_placement(
            scene_time_ms,
            world_transform,
            body_owner,
            input.model(),
            input.animation().animation_id(),
            input.particle_colors().cloned(),
            random,
        )?
    };
    body.scene_registration = Some(UnitSceneRegistration::new(
        mount.map_or(input.model().as_ref(), |mount| mount.model().as_ref()),
        world_transform,
    )?);
    body.unit_presentation = Some(input.generation().clone());
    body.rider_scale = mount.map_or(1.0, |mount| mount.rider_scale());
    prepared.push(source, body);
    for attachment in input.attachments() {
        if input.model().attachment(attachment.point().id()).is_none() {
            return Err(RuntimeTerrainFrameError::MissingPlayerM2Attachment {
                model: input.model().path().clone(),
                attachment_id: attachment.point().id(),
            });
        }
        let identity = attachment.slot().map(|slot| M2PlayerItemIdentity {
            slot,
            visible: input.visible_item(slot),
        });
        let candidate = same_unit
            .then(|| frame.matching_item_model(input.guid(), attachment))
            .flatten();
        let retained = candidate.filter(|index| {
            let unchanged_entry = frame.placements[*index]
                .item_identity
                .zip(identity)
                .is_some_and(|(previous, current)| {
                    previous.slot == current.slot
                        && previous.visible.entry_id() == current.visible.entry_id()
                });
            match attachment.point() {
                CharacterAttachmentPoint::Helmet => true, // 4EF020 compares the model path.
                CharacterAttachmentPoint::ShoulderLeft
                | CharacterAttachmentPoint::ShoulderRight => unchanged_entry || same_shoulders,
                _ => unchanged_entry,
            }
        });
        if let Some(index) = retained {
            let unchanged_visuals = matches!(
                attachment.point(),
                CharacterAttachmentPoint::Helmet
                    | CharacterAttachmentPoint::ShoulderLeft
                    | CharacterAttachmentPoint::ShoulderRight
            ) || frame.placements[index]
                .item_identity
                .zip(identity)
                .is_some_and(|(previous, current)| {
                    previous.visible.enchantment_word() == current.visible.enchantment_word()
                });
            prepared.retain(index, RetainedCharacterState::Item(identity));
            if unchanged_visuals {
                for (index, placement) in frame.placements.iter().enumerate() {
                    if let M2GpuPlacementOwner::PlayerItemVisual {
                        guid, item_point, ..
                    } = placement.owner
                        && guid == input.guid()
                        && item_point == attachment.point()
                    {
                        prepared.retain(index, RetainedCharacterState::Item(None));
                    }
                }
                continue;
            }
        }
        let orientation = M2ModelOrientation::Authored;
        if retained.is_none() {
            let resolved = attachment
                .textures()
                .iter()
                .map(|texture| match texture {
                    ResidentPlayerTexture::Authored(source) => {
                        M2ResolvedTexture::Authored(source.as_ref())
                    }
                    ResidentPlayerTexture::StockWhite => M2ResolvedTexture::StockWhite,
                    ResidentPlayerTexture::StockFailure => M2ResolvedTexture::StockFailure,
                    ResidentPlayerTexture::BodyAtlas => {
                        M2ResolvedTexture::CharacterAtlas(input.atlas())
                    }
                    ResidentPlayerTexture::Unresolved(kind) => M2ResolvedTexture::Unresolved(*kind),
                })
                .collect::<Vec<_>>();
            let source = prepare_gpu_source(
                renderer,
                attachment.model(),
                &resolved,
                None,
                M2LocalLightCount::Four,
                orientation,
            )?;
            let mut placement = default_gpu_placement(
                scene_time_ms,
                world_transform,
                M2GpuPlacementOwner::PlayerItem {
                    guid: input.guid(),
                    point: attachment.point(),
                },
                attachment.model(),
                &frame.animations,
                attachment.particle_colors().cloned(),
                random,
            )?;
            placement.orientation = orientation;
            placement.item_identity = identity;
            prepared.push(source, placement);
        }
        for effect in attachment.visual_effects() {
            let resolved = effect
                .textures()
                .iter()
                .map(|texture| match texture {
                    ResidentPlayerTexture::Authored(source) => {
                        M2ResolvedTexture::Authored(source.as_ref())
                    }
                    ResidentPlayerTexture::StockWhite => M2ResolvedTexture::StockWhite,
                    ResidentPlayerTexture::StockFailure => M2ResolvedTexture::StockFailure,
                    ResidentPlayerTexture::BodyAtlas => {
                        M2ResolvedTexture::CharacterAtlas(input.atlas())
                    }
                    ResidentPlayerTexture::Unresolved(kind) => M2ResolvedTexture::Unresolved(*kind),
                })
                .collect::<Vec<_>>();
            let source = prepare_gpu_source(
                renderer,
                effect.model(),
                &resolved,
                None,
                M2LocalLightCount::Four,
                orientation,
            )?;
            let placement = default_gpu_placement(
                scene_time_ms,
                world_transform,
                M2GpuPlacementOwner::PlayerItemVisual {
                    guid: input.guid(),
                    item_point: attachment.point(),
                    effect_point: effect.point(),
                },
                effect.model(),
                &frame.animations,
                None,
                random,
            )?;
            prepared.push(source, placement);
        }
    }
    Ok(prepared)
}
