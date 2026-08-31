//! Stock item-visual selection and effect-model attachment planning.

use solarity_asset::{AssetPath, ItemVisualCatalog};
use solarity_ecs::VisibleEquipmentItem;

use super::CharacterItemAttachment;

/// One effect M2 attached to an equipped item model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacterItemVisualEffect {
    attachment_id: u32,
    model: AssetPath,
}

impl CharacterItemVisualEffect {
    /// Returns the item-local M2 attachment identifier in the closed `0..4` range.
    #[must_use]
    pub const fn attachment_id(&self) -> u32 {
        self.attachment_id
    }

    /// Returns the exact effect model path authored by `ItemVisualEffects.dbc`.
    #[must_use]
    pub const fn model(&self) -> &AssetPath {
        &self.model
    }
}

/// The selected five-slot visual stack for one attached item M2.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CharacterItemVisualPlan {
    visual_id: Option<u32>,
    effects: Vec<CharacterItemVisualEffect>,
}

impl CharacterItemVisualPlan {
    /// Resolves stock's single item-visual stack for one attached model.
    ///
    /// A valid visual authored by `ItemDisplayInfo.dbc` wins. Otherwise the
    /// permanent public enchantment is considered before the temporary one.
    /// Missing enchantment, visual, and effect rows contribute no child; this
    /// matches the client's checked DBC lookups and does not substitute data.
    #[must_use]
    pub fn resolve(attachment: &CharacterItemAttachment, catalog: &ItemVisualCatalog) -> Self {
        let visible = VisibleEquipmentItem::new(0, attachment.enchantment_word());
        let visual_id = catalog
            .visual(attachment.item_visual_id())
            .map(|visual| visual.id())
            .or_else(|| enchantment_visual_id(visible.permanent_enchantment_id(), catalog))
            .or_else(|| enchantment_visual_id(visible.temporary_enchantment_id(), catalog));
        let Some(visual) = visual_id.and_then(|id| catalog.visual(id)) else {
            return Self::default();
        };
        let effects = visual
            .effect_ids()
            .into_iter()
            .enumerate()
            .filter_map(|(attachment_id, effect_id)| {
                let effect = catalog.effect(effect_id)?;
                let model = effect.model_path()?.clone();
                Some(CharacterItemVisualEffect {
                    attachment_id: attachment_id as u32,
                    model,
                })
            })
            .collect();
        Self {
            visual_id: Some(visual.id()),
            effects,
        }
    }

    /// Returns the selected `ItemVisuals.dbc` identifier, when one resolved.
    #[must_use]
    pub const fn visual_id(&self) -> Option<u32> {
        self.visual_id
    }

    /// Returns effect models in item-local attachment order.
    #[must_use]
    pub fn effects(&self) -> &[CharacterItemVisualEffect] {
        &self.effects
    }
}

/// Joins one public enchantment to an existing nonzero visual definition.
fn enchantment_visual_id(enchantment_id: u16, catalog: &ItemVisualCatalog) -> Option<u32> {
    let enchantment = catalog.enchantment(u32::from(enchantment_id))?;
    catalog
        .visual(enchantment.item_visual_id())
        .map(|visual| visual.id())
}
