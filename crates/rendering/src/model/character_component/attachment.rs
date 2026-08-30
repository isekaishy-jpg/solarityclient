//! Stock held-item model attachment planning for player characters.

use solarity_asset::{AssetPath, InventoryType};
use solarity_ecs::PlayerEquipmentSlot;

use super::{CharacterAttachmentPlanError, CharacterEquipmentItem};

/// One attachment identifier authored into build-12340 character M2s.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum CharacterAttachmentPoint {
    /// Shield at the left forearm.
    Shield = 0,
    /// Item held in the right hand.
    HandRight = 1,
    /// Item held in the left hand.
    HandLeft = 2,
    /// Ordinary main-hand back sheath.
    SheathMainHand = 26,
    /// Ordinary off-hand back sheath.
    SheathOffHand = 27,
    /// Shield back sheath.
    SheathShield = 28,
    /// Large main-hand weapon back sheath.
    LargeWeaponLeft = 30,
    /// Large off-hand weapon back sheath.
    LargeWeaponRight = 31,
    /// Main-hand hip sheath.
    HipWeaponLeft = 32,
    /// Off-hand hip sheath.
    HipWeaponRight = 33,
}

impl CharacterAttachmentPoint {
    /// Returns the exact M2 attachment identifier.
    #[must_use]
    pub const fn id(self) -> u32 {
        self as u32
    }
}

/// Whether held equipment belongs in the hands or on its sheath attachment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CharacterWeaponPose {
    /// Combat-ready hand attachment.
    Ready,
    /// Sheath selected by `Item.dbc`.
    Sheathed,
}

/// Hand selected for the ranged public equipment slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CharacterRangedHand {
    /// Stock's ordinary ranged-item hand in recovered glue behavior.
    Left,
    /// Alternate right-hand path selected by the stock call-site flag.
    Right,
}

/// Runtime placement inputs that are not stored in item display records.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharacterWeaponState {
    pose: CharacterWeaponPose,
    ranged_hand: CharacterRangedHand,
}

impl CharacterWeaponState {
    /// Creates explicit held-item placement state without boolean call-site flags.
    #[must_use]
    pub const fn new(pose: CharacterWeaponPose, ranged_hand: CharacterRangedHand) -> Self {
        Self { pose, ranged_hand }
    }

    /// Returns whether models are ready or sheathed.
    #[must_use]
    pub const fn pose(self) -> CharacterWeaponPose {
        self.pose
    }

    /// Returns the ranged public slot's active hand.
    #[must_use]
    pub const fn ranged_hand(self) -> CharacterRangedHand {
        self.ranged_hand
    }
}

/// One separate item M2 attached to the player model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacterItemAttachment {
    slot: PlayerEquipmentSlot,
    point: CharacterAttachmentPoint,
    model: AssetPath,
    texture: AssetPath,
    item_visual_id: u32,
    particle_color_id: u32,
}

impl CharacterItemAttachment {
    /// Returns the public equipment slot that produced this child model.
    #[must_use]
    pub const fn slot(&self) -> PlayerEquipmentSlot {
        self.slot
    }

    /// Returns the parent M2 attachment point.
    #[must_use]
    pub const fn point(&self) -> CharacterAttachmentPoint {
        self.point
    }

    /// Returns the legacy model reference passed through stock M2 conversion.
    #[must_use]
    pub const fn model(&self) -> &AssetPath {
        &self.model
    }

    /// Returns the BLP bound to replacement texture slot two on the child M2.
    #[must_use]
    pub const fn texture(&self) -> &AssetPath {
        &self.texture
    }

    /// Returns the display's attached item-visual identifier.
    #[must_use]
    pub const fn item_visual_id(&self) -> u32 {
        self.item_visual_id
    }

    /// Returns the display's particle-color identifier.
    #[must_use]
    pub const fn particle_color_id(&self) -> u32 {
        self.particle_color_id
    }
}

/// Separate held-item M2s prepared in public equipment-slot order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CharacterAttachmentPlan {
    attachments: Vec<CharacterItemAttachment>,
}

impl CharacterAttachmentPlan {
    /// Plans stock main-hand, off-hand, and ranged child models.
    ///
    /// Stock reads only model/texture channel zero for this path. A display with
    /// no model name produces no child. Armor attachment support is deliberately
    /// excluded here because its head/shoulder race naming rules are separate;
    /// stock does not attach arbitrary armor-slot models.
    ///
    /// # Errors
    ///
    /// Returns [`CharacterAttachmentPlanError`] when a nonempty DBC name cannot
    /// form a valid archive-relative path.
    pub fn held_items<'catalog, I>(
        equipment: I,
        state: CharacterWeaponState,
    ) -> Result<Self, CharacterAttachmentPlanError>
    where
        I: IntoIterator<Item = CharacterEquipmentItem<'catalog>>,
    {
        let mut attachments = Vec::with_capacity(3);
        for item in equipment {
            let Some(point) = attachment_point(item, state) else {
                continue;
            };
            let display = item.display();
            let [model_name, _] = display.model_names();
            if model_name.is_empty() {
                continue;
            }

            let [texture_name, _] = display.model_textures();
            let folder = if item.definition().inventory_type() == InventoryType::Shield {
                "Item\\ObjectComponents\\Shield"
            } else {
                "Item\\ObjectComponents\\Weapon"
            };
            attachments.push(CharacterItemAttachment {
                slot: item.slot(),
                point,
                model: AssetPath::new(format!("{folder}\\{model_name}"))?,
                texture: AssetPath::new(format!("{folder}\\{texture_name}.blp"))?,
                item_visual_id: display.item_visual_id(),
                particle_color_id: display.particle_color_id(),
            });
        }
        Ok(Self { attachments })
    }

    /// Returns planned child models in public equipment-slot order.
    #[must_use]
    pub fn attachments(&self) -> &[CharacterItemAttachment] {
        &self.attachments
    }
}

/// Selects stock's hand or sheath link and rejects non-held public slots.
fn attachment_point(
    item: CharacterEquipmentItem<'_>,
    state: CharacterWeaponState,
) -> Option<CharacterAttachmentPoint> {
    let right_hand = match item.slot() {
        PlayerEquipmentSlot::MainHand => true,
        PlayerEquipmentSlot::OffHand => false,
        PlayerEquipmentSlot::Ranged => state.ranged_hand == CharacterRangedHand::Right,
        _ => return None,
    };

    if state.pose == CharacterWeaponPose::Sheathed {
        return sheath_point(item.definition().sheathe_type(), right_hand);
    }
    if item.definition().inventory_type() == InventoryType::Shield {
        return Some(CharacterAttachmentPoint::Shield);
    }
    Some(if right_hand {
        CharacterAttachmentPoint::HandRight
    } else {
        CharacterAttachmentPoint::HandLeft
    })
}

/// Reproduces `GetSheatheLink` without inventing a link for category zero.
fn sheath_point(sheathe_type: u32, right_hand: bool) -> Option<CharacterAttachmentPoint> {
    match (sheathe_type, right_hand) {
        (1, true) => Some(CharacterAttachmentPoint::SheathMainHand),
        (1, false) => Some(CharacterAttachmentPoint::SheathOffHand),
        (2, true) => Some(CharacterAttachmentPoint::LargeWeaponLeft),
        (2, false) => Some(CharacterAttachmentPoint::LargeWeaponRight),
        (3, true) => Some(CharacterAttachmentPoint::HipWeaponLeft),
        (3, false) => Some(CharacterAttachmentPoint::HipWeaponRight),
        (4, _) => Some(CharacterAttachmentPoint::SheathShield),
        _ => None,
    }
}
