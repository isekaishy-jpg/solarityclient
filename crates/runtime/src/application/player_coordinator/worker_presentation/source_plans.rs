//! Domain attachment selection shared by source admission and derived construction.

use super::super::{
    CharacterAttachmentPlan, CharacterEquipmentItem, CharacterWeaponState, CreatureModelKey,
    PlayerEquipmentSlot, ResidentGlueCharacterKey, RuntimePlayerError, RuntimePlayerSharedCatalogs,
    VisibleEquipmentItem, resolve_creation_equipment, resolve_npc_equipment,
    resolve_selection_equipment, resolve_selection_quiver,
};

impl RuntimePlayerSharedCatalogs {
    pub(in crate::application::player_coordinator) fn creature_attachment_plan(
        &self,
        key: &CreatureModelKey,
        definitions: &[Option<solarity_asset::ItemDefinition>; 3],
        model: &solarity_asset::DecodedM2Model,
    ) -> Result<CharacterAttachmentPlan, RuntimePlayerError> {
        let appearance = self
            .creatures
            .resolve_model(key.display_id)
            .map_err(super::super::UnitModelAppearanceError::from)?;
        let mut plan = if let Some(extra) = appearance.extra() {
            let equipment = resolve_npc_equipment(
                appearance.display().id(),
                extra.npc_item_display_ids(),
                &self.item_displays,
            )?;
            CharacterAttachmentPlan::npc_armor(
                equipment.iter().copied(),
                &self.races,
                extra.race_id(),
                extra.gender_id(),
            )?
        } else {
            CharacterAttachmentPlan::default()
        };
        if let Some(state) = key.weapon_state {
            let equipment = std::array::from_fn(|index| {
                let entry = key.virtual_entries[index];
                if entry == 0 {
                    return None;
                }
                let definition = definitions[index].as_ref()?;
                let display = self.item_displays.display(definition.display_info_id())?;
                Some(CharacterEquipmentItem::new_visible(
                    [
                        PlayerEquipmentSlot::MainHand,
                        PlayerEquipmentSlot::OffHand,
                        PlayerEquipmentSlot::Ranged,
                    ][index],
                    VisibleEquipmentItem::new(entry, 0),
                    definition,
                    display,
                ))
            });
            plan.add_npc_held_items(
                equipment,
                state,
                [definitions[0].as_ref(), definitions[1].as_ref()],
            )?;
        }
        plan.retain_attachments(|attachment| {
            !matches!(
                attachment.slot(),
                Some(
                    PlayerEquipmentSlot::MainHand
                        | PlayerEquipmentSlot::OffHand
                        | PlayerEquipmentSlot::Ranged
                )
            ) || model.attachment(attachment.point().id()).is_some()
        });
        Ok(plan)
    }

    pub(in crate::application::player_coordinator) fn glue_attachment_plan(
        &self,
        key: &ResidentGlueCharacterKey,
    ) -> Result<CharacterAttachmentPlan, RuntimePlayerError> {
        match key {
            ResidentGlueCharacterKey::Creation(preview) => {
                let race = self.races.race(u32::from(preview.race_id())).ok_or(
                    RuntimePlayerError::MissingCharacterRace {
                        race_id: u32::from(preview.race_id()),
                    },
                )?;
                let mut equipment = resolve_creation_equipment(
                    preview,
                    &self.start_outfits,
                    &self.item_definitions,
                    &self.item_displays,
                )?;
                equipment.retain(|item| {
                    item.inventory_type() != Some(solarity_asset::InventoryType::Head)
                });
                Ok(CharacterAttachmentPlan::equipped_items(
                    equipment.iter().copied(),
                    race,
                    u32::from(preview.gender_id()),
                    CharacterWeaponState::new(solarity_ecs::UnitSheathState::Unarmed),
                )?)
            }
            ResidentGlueCharacterKey::Selection(preview) => {
                let race = self.races.race(u32::from(preview.race_id())).ok_or(
                    RuntimePlayerError::MissingCharacterRace {
                        race_id: u32::from(preview.race_id()),
                    },
                )?;
                let mut equipment = resolve_selection_equipment(preview, &self.item_displays)?;
                equipment.retain(|item| {
                    (preview.show_helmet() || item.slot() != PlayerEquipmentSlot::Head)
                        && (preview.show_cloak() || item.slot() != PlayerEquipmentSlot::Back)
                });
                Ok(CharacterAttachmentPlan::character_selection(
                    equipment.iter().copied(),
                    resolve_selection_quiver(preview, &self.item_displays)?,
                    race,
                    u32::from(preview.gender_id()),
                    preview.class_id(),
                )?)
            }
        }
    }
}
