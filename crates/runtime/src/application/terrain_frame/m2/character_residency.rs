//! Transactional character publication with retained component instances.

use super::*;
use solarity_ecs::{PlayerEquipmentSlot, VisibleEquipmentItem};

#[derive(Clone, Copy)]
pub(super) struct M2PlayerItemIdentity {
    slot: PlayerEquipmentSlot,
    visible: VisibleEquipmentItem,
}

/// New resources are prepared without disturbing continuing child instances.
#[derive(Default)]
pub(super) struct M2PreparedCharacter {
    new: Vec<(usize, M2GpuSource, M2GpuPlacement)>,
    retained: Vec<(usize, usize, Option<M2PlayerItemIdentity>)>,
    ready: Vec<(usize, M2GpuPlacement)>,
}

impl M2PreparedCharacter {
    fn push(&mut self, source: M2GpuSource, placement: M2GpuPlacement) {
        self.new
            .push((self.new.len() + self.retained.len(), source, placement));
    }

    fn retain(&mut self, index: usize, identity: Option<M2PlayerItemIdentity>) {
        self.retained
            .push((index, self.new.len() + self.retained.len(), identity));
    }
}

impl M2Frame {
    /// Called only after all changed characters have prepared successfully.
    pub(super) fn retain_character_instances(&mut self, characters: &mut [M2PreparedCharacter]) {
        let mut retained = HashMap::new();
        for (character_index, character) in characters.iter_mut().enumerate() {
            self.retain_unit_effects(character.new.iter_mut().map(|(_, _, placement)| placement));
            for &(index, order, identity) in &character.retained {
                let previous = retained.insert(index, (character_index, order, identity));
                debug_assert!(previous.is_none(), "one owner per retained component");
            }
        }
        if retained.is_empty() {
            return;
        }
        // Detach the retained placements before retiring the old characters.
        // Their source indices stay live and retain the same GPU handles.
        for (index, mut placement) in std::mem::take(&mut self.placements).into_iter().enumerate() {
            if let Some((character, order, identity)) = retained.remove(&index) {
                if let Some(identity) = identity {
                    placement.item_identity = Some(identity);
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
            .extend(character.ready.into_iter().map(|(_, placement)| placement));
    }

    fn same_unit_owner(
        &self,
        input: &ResidentPlayerFrameInput<'_>,
        body_owner: M2GpuPlacementOwner,
    ) -> bool {
        let Some(animation) = input.unit_animation() else {
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
    let same_unit = frame.same_unit_owner(input, body_owner);
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
                ResidentCreatureTexture::Unresolved(kind) => M2ResolvedTexture::Unresolved(*kind),
            })
            .collect::<Vec<_>>();
        let source = prepare_gpu_source(
            renderer,
            mount.model(),
            &resolved,
            None,
            M2LocalLightCount::Zero,
            M2ModelOrientation::Authored,
        )?;
        let owner = match body_owner {
            M2GpuPlacementOwner::PlayerBody { guid } => M2GpuPlacementOwner::PlayerMount { guid },
            M2GpuPlacementOwner::RemotePlayerBody { guid } => {
                M2GpuPlacementOwner::RemotePlayerMount { guid }
            }
            _ => unreachable!("character preparation requires a player body owner"),
        };
        let placement = unit_gpu_placement(
            0,
            world_transform,
            owner,
            mount.model(),
            mount.animation().animation_id(),
            mount.particle_colors().cloned(),
            random,
        )?;
        // Parent-first insertion lets the current mount bone pose determine
        // the rider transform before the body and its equipment are visited.
        prepared.push(source, placement);
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
        M2LocalLightCount::Zero,
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
        )?;
        body.unit_animation = Some(Rc::clone(animation));
        body
    } else {
        unit_gpu_placement(
            0,
            world_transform,
            body_owner,
            input.model(),
            input.animation().animation_id(),
            input.particle_colors().cloned(),
            random,
        )?
    };
    body.unit_presentation = Some(input.generation().clone());
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
            prepared.retain(index, identity);
            if unchanged_visuals {
                for (index, placement) in frame.placements.iter().enumerate() {
                    if let M2GpuPlacementOwner::PlayerItemVisual {
                        guid, item_point, ..
                    } = placement.owner
                        && guid == input.guid()
                        && item_point == attachment.point()
                    {
                        prepared.retain(index, None);
                    }
                }
                continue;
            }
        }
        let orientation = if attachment.is_model_mirrored() {
            M2ModelOrientation::Mirrored
        } else {
            M2ModelOrientation::Authored
        };
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
                M2LocalLightCount::Zero,
                orientation,
            )?;
            let mut placement = unit_gpu_placement(
                0,
                world_transform,
                M2GpuPlacementOwner::PlayerItem {
                    guid: input.guid(),
                    point: attachment.point(),
                },
                attachment.model(),
                0,
                attachment.particle_colors().cloned(),
                random,
            )?;
            placement.orientation = orientation;
            placement.animation_binding = equipment_animation_binding(attachment);
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
                M2LocalLightCount::Zero,
                orientation,
            )?;
            let placement = unit_gpu_placement(
                0,
                world_transform,
                M2GpuPlacementOwner::PlayerItemVisual {
                    guid: input.guid(),
                    item_point: attachment.point(),
                    effect_point: effect.point(),
                },
                effect.model(),
                0,
                None,
                random,
            )?;
            prepared.push(source, placement);
        }
    }
    Ok(prepared)
}
