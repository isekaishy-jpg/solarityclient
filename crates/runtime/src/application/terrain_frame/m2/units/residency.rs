//! Transactional unit source replacement preserves living component state.

use super::super::character_residency::{
    M2PreparedCharacter, M2UnitItemIdentity, UnitEquipmentGpuInput, UnitMountGpuInput,
    prepare_character_gpu, prepare_mount_gpu, prepare_unit_equipment_gpu,
};
use super::super::playback::M2PlaybackStorage;
use super::super::source::prepare_gpu_source;
use super::super::unit_registration::UnitSceneRegistration;
use super::super::{M2Frame, M2GpuPlacementOwner, RuntimeTerrainFrameError};
use super::super::{
    M2GeosetSelection, M2GpuPlacement, M2ResolvedTexture, UnitGroundPlacement, m2_gpu_placement,
    unit_gpu_placement, unit_placement_transform,
};
use crate::application::player_coordinator::{
    ResidentCreatureFrameInput, ResidentCreatureTexture, ResidentPlayerFrameInput,
};
use crate::random::CrtRand;
use solarity_rendering::{M2LocalLightCount, M2ModelOrientation, VulkanRenderer};
use std::rc::Rc;

impl M2Frame {
    /// Replaces the one player-owned source and placement transactionally.
    pub(in super::super::super) fn replace_player(
        &mut self,
        renderer: &mut VulkanRenderer,
        input: Option<ResidentPlayerFrameInput<'_>>,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.retire_removed_models();
        let Some(input) = input else {
            self.remove_player();
            return Ok(());
        };
        let mut prepared = [prepare_character_gpu(
            self,
            renderer,
            &input,
            M2GpuPlacementOwner::PlayerBody { guid: input.guid() },
            self.animation_time_ms(),
            random,
        )?];
        self.retain_character_instances(&mut prepared);
        self.remove_player();
        let topology_retained = self.placements.len();
        for character in prepared {
            self.publish_character(character);
        }
        solarity_profiling::profile_event_value!(
            "m2.topology.local.added",
            self.placements.len() - topology_retained
        );
        Ok(())
    }

    /// Replaces all visible creature sources and placements transactionally.
    pub(in super::super::super) fn replace_creatures(
        &mut self,
        renderer: &mut VulkanRenderer,
        inputs: &[ResidentCreatureFrameInput<'_>],
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.retire_removed_models();
        let mut prepared = Vec::with_capacity(inputs.len());
        let mut retained = Vec::with_capacity(inputs.len());
        for input in inputs {
            if self
                .placements
                .owner_indices(M2GpuPlacementOwner::CreatureBody { guid: input.guid() })
                .any(|index| {
                    self.placements[index]
                        .unit_presentation
                        .as_ref()
                        .is_some_and(|generation| generation.matches(input.generation()))
                })
            {
                retained.push(input.guid());
                continue;
            }
            let mut character = M2PreparedCharacter::default();
            let mount = input.mount();
            if let Some(mount) = mount {
                prepare_mount_gpu(
                    self,
                    renderer,
                    &mut character,
                    UnitMountGpuInput {
                        mount,
                        body_owner: M2GpuPlacementOwner::CreatureBody { guid: input.guid() },
                        world_transform: input.world_transform(),
                        animation: input.unit_animation(),
                    },
                    self.animation_time_ms(),
                    random,
                )?;
            }
            let resolved = input
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
                input.model(),
                &resolved,
                input.geosets().map(M2GeosetSelection::from),
                M2LocalLightCount::Four,
                M2ModelOrientation::Authored,
            )?;
            let transform = unit_placement_transform(
                input.world_transform(),
                mount.map_or(input.object_scale(), |mount| mount.object_scale()),
            )?;
            let mut placement = if let Some(animation) = input.unit_animation() {
                animation.synchronize(self.animation_time_ms() as u32, random)?;
                let mut placement = m2_gpu_placement(
                    0,
                    transform,
                    M2GpuPlacementOwner::CreatureBody { guid: input.guid() },
                    input.model(),
                    Some(M2PlaybackStorage::Shared(animation.playback())),
                    input.particle_colors().cloned(),
                    self.animation_time_ms() as u32,
                )?;
                placement.unit_animation = Some(Rc::clone(animation));
                placement
            } else {
                unit_gpu_placement(
                    self.animation_time_ms(),
                    transform,
                    M2GpuPlacementOwner::CreatureBody { guid: input.guid() },
                    input.model(),
                    input.animation().animation_id(),
                    input.particle_colors().cloned(),
                    random,
                )?
            };
            placement.scene_registration = Some(UnitSceneRegistration::new(
                mount.map_or(input.model().as_ref(), |mount| mount.model().as_ref()),
                transform,
            )?);
            placement.unit_presentation = Some(input.generation().clone());
            placement.rider_scale = mount.map_or(1.0, |mount| mount.rider_scale());
            placement.ground_placement =
                input
                    .unit_animation()
                    .filter(|_| mount.is_none())
                    .map(|animation| UnitGroundPlacement {
                        position: input.world_transform().position(),
                        scale: input.object_scale(),
                        owner: Rc::clone(animation),
                    });
            character.push(source, placement);
            prepare_unit_equipment_gpu(
                self,
                renderer,
                &mut character,
                UnitEquipmentGpuInput {
                    guid: input.guid(),
                    model: input.model(),
                    attachments: input.attachments(),
                    animation: input.unit_animation(),
                    body_owner: M2GpuPlacementOwner::CreatureBody { guid: input.guid() },
                    world_transform: transform,
                    atlas: None,
                },
                |slot| {
                    input
                        .armor_display_id(slot)
                        .map(|display_id| M2UnitItemIdentity::NpcArmor { slot, display_id })
                        .or_else(|| {
                            input
                                .virtual_item_entry(slot)
                                .map(|entry_id| M2UnitItemIdentity::NpcVirtual { slot, entry_id })
                        })
                },
                self.animation_time_ms(),
                random,
            )?;
            prepared.push(character);
        }

        self.retain_character_instances(&mut prepared);
        self.remove_creatures(&retained);
        let topology_retained = self.placements.len();
        for character in prepared {
            self.publish_character(character);
        }
        solarity_profiling::profile_event_value!(
            "m2.topology.creatures.added",
            self.placements.len() - topology_retained
        );
        Ok(())
    }

    /// Replaces every visible remote character and equipped child placement.
    pub(in super::super::super) fn replace_remote_players(
        &mut self,
        renderer: &mut VulkanRenderer,
        inputs: &[ResidentPlayerFrameInput<'_>],
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.retire_removed_models();
        let mut prepared = Vec::with_capacity(inputs.len());
        let mut retained = Vec::with_capacity(inputs.len());
        for input in inputs {
            if self
                .placements
                .owner_indices(M2GpuPlacementOwner::RemotePlayerBody { guid: input.guid() })
                .any(|index| {
                    self.placements[index]
                        .unit_presentation
                        .as_ref()
                        .is_some_and(|generation| generation.matches(input.generation()))
                })
            {
                retained.push(input.guid());
                continue;
            }
            prepared.push(prepare_character_gpu(
                self,
                renderer,
                input,
                M2GpuPlacementOwner::RemotePlayerBody { guid: input.guid() },
                self.animation_time_ms(),
                random,
            )?);
        }
        self.retain_character_instances(&mut prepared);
        self.remove_remote_players(&retained);
        let topology_retained = self.placements.len();
        for character in prepared {
            self.publish_character(character);
        }
        solarity_profiling::profile_event_value!(
            "m2.topology.remote_players.added",
            self.placements.len() - topology_retained
        );
        Ok(())
    }

    /// A material rebuild changes GPU resources, not the living model's
    /// emitter histories. Transfer them only after every replacement has
    /// prepared successfully, and only within the same unit/model lifetime.
    pub(in super::super) fn retain_unit_effects<'a>(
        &mut self,
        prepared: impl IntoIterator<Item = &'a mut M2GpuPlacement>,
    ) {
        for replacement in prepared {
            let Some(animation) = &replacement.unit_animation else {
                continue;
            };
            let Some(index) = self
                .placements
                .owner_indices(replacement.owner)
                .find(|&index| {
                    self.placements[index]
                        .unit_animation
                        .as_ref()
                        .is_some_and(|previous| Rc::ptr_eq(previous, animation))
                })
            else {
                continue;
            };
            let previous = &mut self.placements[index];
            std::mem::swap(&mut previous.particles, &mut replacement.particles);
            std::mem::swap(&mut previous.ribbons, &mut replacement.ribbons);
            replacement.last_effect_time_ms = previous.last_effect_time_ms;
        }
    }
}
