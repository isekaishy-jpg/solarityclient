//! Stock equipment child-model attachment planning for player characters.

use solarity_asset::{AssetPath, CharacterRace, InventoryType, ItemDefinition};
use solarity_ecs::{PlayerEquipmentSlot, UnitSheathState};

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
    /// Left shoulder armor.
    ShoulderLeft = 5,
    /// Right shoulder armor.
    ShoulderRight = 6,
    /// Race- and gender-suffixed helmet model.
    Helmet = 11,
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

/// Runtime placement inputs that are not stored in item display records.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharacterWeaponState {
    sheath_state: UnitSheathState,
}

impl CharacterWeaponState {
    /// Creates explicit held-item placement state without boolean call-site flags.
    #[must_use]
    pub const fn new(sheath_state: UnitSheathState) -> Self {
        Self { sheath_state }
    }

    /// Returns the authoritative unit weapon state.
    #[must_use]
    pub const fn sheath_state(self) -> UnitSheathState {
        self.sheath_state
    }
}

/// One separate item M2 attached to the player model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacterItemAttachment {
    source: CharacterItemAttachmentSource,
    point: CharacterAttachmentPoint,
    model: AssetPath,
    texture: Option<AssetPath>,
    item_visual_id: u32,
    enchantment_word: u32,
    particle_color_id: u32,
    display_flags: u32,
}

impl CharacterItemAttachment {
    /// Returns the public equipment slot that produced this child model, if any.
    #[must_use]
    pub const fn slot(&self) -> Option<PlayerEquipmentSlot> {
        match self.source {
            CharacterItemAttachmentSource::EquipmentSlot(slot) => Some(slot),
            CharacterItemAttachmentSource::CharacterEnumerationBag(_) => None,
        }
    }

    /// Returns the character-enumeration bag index that produced this model.
    #[must_use]
    pub const fn character_enumeration_bag_slot(&self) -> Option<u8> {
        match self.source {
            CharacterItemAttachmentSource::EquipmentSlot(_) => None,
            CharacterItemAttachmentSource::CharacterEnumerationBag(slot) => Some(slot),
        }
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

    /// Returns the BLP bound to item replacement categories 2 through 4.
    #[must_use]
    pub const fn texture(&self) -> Option<&AssetPath> {
        self.texture.as_ref()
    }

    /// Returns the display's attached item-visual identifier.
    #[must_use]
    pub const fn item_visual_id(&self) -> u32 {
        self.item_visual_id
    }

    /// Returns the packed permanent and temporary enchantment identifiers.
    #[must_use]
    pub const fn enchantment_word(&self) -> u32 {
        self.enchantment_word
    }

    /// Returns the display's particle-color identifier.
    #[must_use]
    pub const fn particle_color_id(&self) -> u32 {
        self.particle_color_id
    }

    /// Returns whether this component follows the character's active sequence.
    ///
    /// Build 12340 assigns this behavior to `ItemDisplayInfo.flags` bit `0x40`.
    #[must_use]
    pub const fn inherits_character_animation(&self) -> bool {
        self.display_flags & 0x40 != 0
    }

    /// Returns whether the attachment-six shoulder follows attachment five.
    ///
    /// Build 12340 applies `ItemDisplayInfo.flags` bit `0x80` only to the
    /// component installed at character attachment six.
    #[must_use]
    pub const fn mirrors_opposite_shoulder_animation(&self) -> bool {
        self.point.id() == CharacterAttachmentPoint::ShoulderRight.id()
            && self.display_flags & 0x80 != 0
    }

    /// Returns whether stock reflects this attached model across local X.
    ///
    /// Build 12340 applies `ItemDisplayInfo.flags` bit `0x100` to every
    /// attachment except the right-hand link (identifier one).
    #[must_use]
    pub const fn is_model_mirrored(&self) -> bool {
        self.point.id() != CharacterAttachmentPoint::HandRight.id()
            && self.display_flags & 0x100 != 0
    }
}

/// Domain source of one child attachment in world or Glue presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CharacterItemAttachmentSource {
    /// One of the nineteen public equipment slots.
    EquipmentSlot(PlayerEquipmentSlot),
    /// One of the four trailing `SMSG_CHAR_ENUM` bag records.
    CharacterEnumerationBag(u8),
}

/// One modeled trailing bag record selected for stock's Glue quiver component.
#[derive(Clone, Copy, Debug)]
pub struct CharacterSelectionQuiver<'catalog> {
    bag_slot: u8,
    display: &'catalog solarity_asset::ItemDisplayInfo,
}

impl<'catalog> CharacterSelectionQuiver<'catalog> {
    /// Creates a quiver input from an exact character-enumeration bag index.
    ///
    /// # Errors
    ///
    /// Returns [`CharacterAttachmentPlanError`] unless `bag_slot` is in the
    /// stock trailing range `19..=22`.
    pub fn new(
        bag_slot: u8,
        display: &'catalog solarity_asset::ItemDisplayInfo,
    ) -> Result<Self, CharacterAttachmentPlanError> {
        if !(19..=22).contains(&bag_slot) {
            return Err(
                CharacterAttachmentPlanError::InvalidCharacterEnumerationBagSlot { bag_slot },
            );
        }
        Ok(Self { bag_slot, display })
    }
}

/// Separate held-item M2s prepared in public equipment-slot order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CharacterAttachmentPlan {
    attachments: Vec<CharacterItemAttachment>,
}

impl CharacterAttachmentPlan {
    /// Plans every stock equipment child model for one player character.
    ///
    /// Head and shoulder armor are the only armor slots with child M2s in the
    /// unmodified client. Helmet filenames are derived from `ChrRaces.dbc`;
    /// shoulder and held-item names remain exactly as authored by their display.
    ///
    /// # Errors
    ///
    /// Returns [`CharacterAttachmentPlanError`] when a nonempty DBC name cannot
    /// form a valid archive-relative path or helmet gender is not male/female.
    pub fn equipped_items<'catalog, I>(
        equipment: I,
        race: &CharacterRace,
        gender_id: u32,
        weapon_state: CharacterWeaponState,
    ) -> Result<Self, CharacterAttachmentPlanError>
    where
        I: IntoIterator<Item = CharacterEquipmentItem<'catalog>>,
    {
        let mut attachments = Vec::with_capacity(6);
        for item in equipment {
            match item.slot() {
                PlayerEquipmentSlot::Head => {
                    push_helmet(&mut attachments, item, race, gender_id)?;
                }
                PlayerEquipmentSlot::Shoulders => {
                    push_shoulders(&mut attachments, item)?;
                }
                PlayerEquipmentSlot::MainHand
                | PlayerEquipmentSlot::OffHand
                | PlayerEquipmentSlot::Ranged => {
                    push_held_item(&mut attachments, item, weapon_state)?;
                }
                PlayerEquipmentSlot::Neck
                | PlayerEquipmentSlot::Shirt
                | PlayerEquipmentSlot::Chest
                | PlayerEquipmentSlot::Waist
                | PlayerEquipmentSlot::Legs
                | PlayerEquipmentSlot::Feet
                | PlayerEquipmentSlot::Wrists
                | PlayerEquipmentSlot::Hands
                | PlayerEquipmentSlot::FingerOne
                | PlayerEquipmentSlot::FingerTwo
                | PlayerEquipmentSlot::TrinketOne
                | PlayerEquipmentSlot::TrinketTwo
                | PlayerEquipmentSlot::Back
                | PlayerEquipmentSlot::Tabard => {}
            }
        }
        Ok(Self { attachments })
    }

    /// Plans the held character-enumeration presentation used by Glue.
    ///
    /// Unlike world equipment, `SMSG_CHAR_ENUM` carries display and inventory
    /// identifiers but no item entry. Build 12340 presents those weapons in
    /// the hands, selects a hunter's ranged slot, and treats inventory type 14
    /// as a shield attachment.
    pub fn character_selection<'catalog, I>(
        equipment: I,
        quiver: Option<CharacterSelectionQuiver<'catalog>>,
        race: &CharacterRace,
        gender_id: u32,
        class_id: u8,
    ) -> Result<Self, CharacterAttachmentPlanError>
    where
        I: IntoIterator<Item = CharacterEquipmentItem<'catalog>>,
    {
        let equipment = equipment.into_iter().collect::<Vec<_>>();
        let mut attachments = Vec::with_capacity(6);
        for item in &equipment {
            match item.slot() {
                PlayerEquipmentSlot::Head => {
                    push_helmet(&mut attachments, *item, race, gender_id)?;
                }
                PlayerEquipmentSlot::Shoulders => {
                    push_shoulders(&mut attachments, *item)?;
                }
                _ => {}
            }
        }
        if let Some(quiver) = quiver {
            push_selection_quiver(&mut attachments, quiver)?;
        }

        let at_slot = |slot| equipment.iter().copied().find(|item| item.slot() == slot);
        let visible = |item: Option<CharacterEquipmentItem<'catalog>>| {
            item.filter(|item| item.inventory_type() != Some(InventoryType::RangedRight))
        };
        let main_hand = at_slot(PlayerEquipmentSlot::MainHand);
        let off_hand = at_slot(PlayerEquipmentSlot::OffHand);
        let ranged = at_slot(PlayerEquipmentSlot::Ranged);
        if class_id == 3 {
            if let Some(item) = ranged {
                let point = if item.inventory_type() == Some(InventoryType::Ranged) {
                    CharacterAttachmentPoint::HandLeft
                } else {
                    CharacterAttachmentPoint::HandRight
                };
                push_selection_held_item(&mut attachments, item, point)?;
                return Ok(Self { attachments });
            }
        } else if let Some(item) = visible(main_hand).or_else(|| visible(ranged)) {
            push_selection_held_item(&mut attachments, item, CharacterAttachmentPoint::HandRight)?;
        }
        if let Some(item) = visible(off_hand) {
            let point = if item.inventory_type() == Some(InventoryType::Shield) {
                CharacterAttachmentPoint::Shield
            } else {
                CharacterAttachmentPoint::HandLeft
            };
            push_selection_held_item(&mut attachments, item, point)?;
        }
        Ok(Self { attachments })
    }

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
            push_held_item(&mut attachments, item, state)?;
        }
        Ok(Self { attachments })
    }

    /// Returns planned child models in public equipment-slot order.
    #[must_use]
    pub fn attachments(&self) -> &[CharacterItemAttachment] {
        &self.attachments
    }
}

/// Adds the race/gender-suffixed stock helmet attachment when authored.
fn push_helmet(
    attachments: &mut Vec<CharacterItemAttachment>,
    item: CharacterEquipmentItem<'_>,
    race: &CharacterRace,
    gender_id: u32,
) -> Result<(), CharacterAttachmentPlanError> {
    let display = item.display();
    let [model_name, _] = display.model_names();
    if model_name.is_empty() {
        return Ok(());
    }
    let gender_suffix = match gender_id {
        0 => 'M',
        1 => 'F',
        _ => {
            return Err(CharacterAttachmentPlanError::UnsupportedHelmetGender { gender_id });
        }
    };
    let model_stem = model_name
        .rfind('.')
        .map_or(model_name, |extension| &model_name[..extension]);
    let [texture_name, _] = display.model_textures();
    attachments.push(CharacterItemAttachment {
        source: CharacterItemAttachmentSource::EquipmentSlot(item.slot()),
        point: CharacterAttachmentPoint::Helmet,
        model: AssetPath::new(format!(
            "Item\\ObjectComponents\\Head\\{model_stem}_{}{gender_suffix}.mdx",
            race.client_prefix()
        ))?,
        texture: attachment_texture("Item\\ObjectComponents\\Head", texture_name)?,
        item_visual_id: item
            .item_visual_override()
            .unwrap_or(display.item_visual_id()),
        enchantment_word: item.visible().enchantment_word(),
        particle_color_id: display.particle_color_id(),
        display_flags: display.flags(),
    });
    Ok(())
}

/// Adds both independently authored shoulder channels at their stock links.
fn push_shoulders(
    attachments: &mut Vec<CharacterItemAttachment>,
    item: CharacterEquipmentItem<'_>,
) -> Result<(), CharacterAttachmentPlanError> {
    let display = item.display();
    let models = display.model_names();
    let textures = display.model_textures();
    let points = [
        CharacterAttachmentPoint::ShoulderRight,
        CharacterAttachmentPoint::ShoulderLeft,
    ];
    for channel in 0..2 {
        if models[channel].is_empty() {
            continue;
        }
        attachments.push(CharacterItemAttachment {
            source: CharacterItemAttachmentSource::EquipmentSlot(item.slot()),
            point: points[channel],
            model: AssetPath::new(format!(
                "Item\\ObjectComponents\\Shoulder\\{}",
                models[channel]
            ))?,
            texture: attachment_texture("Item\\ObjectComponents\\Shoulder", textures[channel])?,
            item_visual_id: item
                .item_visual_override()
                .unwrap_or(display.item_visual_id()),
            enchantment_word: item.visible().enchantment_word(),
            particle_color_id: display.particle_color_id(),
            display_flags: display.flags(),
        });
    }
    Ok(())
}

/// Adds one stock weapon/shield model from display channel zero.
fn push_held_item(
    attachments: &mut Vec<CharacterItemAttachment>,
    item: CharacterEquipmentItem<'_>,
    state: CharacterWeaponState,
) -> Result<(), CharacterAttachmentPlanError> {
    let display = item.display();
    let [model_name, _] = display.model_names();
    if model_name.is_empty() {
        return Ok(());
    }
    let Some(point) = attachment_point(item, state)? else {
        return Ok(());
    };

    let [texture_name, _] = display.model_textures();
    let definition = required_definition(item)?;
    let folder = if definition.inventory_type() == InventoryType::Shield {
        "Item\\ObjectComponents\\Shield"
    } else {
        "Item\\ObjectComponents\\Weapon"
    };
    attachments.push(CharacterItemAttachment {
        source: CharacterItemAttachmentSource::EquipmentSlot(item.slot()),
        point,
        model: AssetPath::new(format!("{folder}\\{model_name}"))?,
        texture: attachment_texture(folder, texture_name)?,
        item_visual_id: item
            .item_visual_override()
            .unwrap_or(display.item_visual_id()),
        enchantment_word: item.visible().enchantment_word(),
        particle_color_id: display.particle_color_id(),
        display_flags: display.flags(),
    });
    Ok(())
}

/// Adds one hand-held display from character-enumeration metadata alone.
fn push_selection_held_item(
    attachments: &mut Vec<CharacterItemAttachment>,
    item: CharacterEquipmentItem<'_>,
    point: CharacterAttachmentPoint,
) -> Result<(), CharacterAttachmentPlanError> {
    let display = item.display();
    let [model_name, _] = display.model_names();
    if model_name.is_empty() {
        return Ok(());
    }
    let [texture_name, _] = display.model_textures();
    let folder = if item.inventory_type() == Some(InventoryType::Shield) {
        "Item\\ObjectComponents\\Shield"
    } else {
        "Item\\ObjectComponents\\Weapon"
    };
    attachments.push(CharacterItemAttachment {
        source: CharacterItemAttachmentSource::EquipmentSlot(item.slot()),
        point,
        model: AssetPath::new(format!("{folder}\\{model_name}"))?,
        texture: attachment_texture(folder, texture_name)?,
        item_visual_id: item
            .item_visual_override()
            .unwrap_or(display.item_visual_id()),
        enchantment_word: 0,
        particle_color_id: display.particle_color_id(),
        display_flags: display.flags(),
    });
    Ok(())
}

/// Adds the last modeled enumeration bag through stock's quiver component.
fn push_selection_quiver(
    attachments: &mut Vec<CharacterItemAttachment>,
    quiver: CharacterSelectionQuiver<'_>,
) -> Result<(), CharacterAttachmentPlanError> {
    let [model_name, _] = quiver.display.model_names();
    if model_name.is_empty() {
        return Ok(());
    }
    let [texture_name, _] = quiver.display.model_textures();
    attachments.push(CharacterItemAttachment {
        source: CharacterItemAttachmentSource::CharacterEnumerationBag(quiver.bag_slot),
        point: CharacterAttachmentPoint::SheathMainHand,
        model: AssetPath::new(format!("Item\\ObjectComponents\\Quiver\\{model_name}"))?,
        texture: attachment_texture("Item\\ObjectComponents\\Quiver", texture_name)?,
        item_visual_id: 0,
        enchantment_word: 0,
        particle_color_id: quiver.display.particle_color_id(),
        display_flags: quiver.display.flags(),
    });
    Ok(())
}

/// Selects stock's hand or sheath link and rejects non-held public slots.
fn attachment_point(
    item: CharacterEquipmentItem<'_>,
    state: CharacterWeaponState,
) -> Result<Option<CharacterAttachmentPoint>, CharacterAttachmentPlanError> {
    let right_hand = match item.slot() {
        PlayerEquipmentSlot::MainHand => true,
        PlayerEquipmentSlot::OffHand => false,
        PlayerEquipmentSlot::Ranged => ranged_item_uses_right_hand(item)?,
        _ => return Ok(None),
    };

    let ready = match state.sheath_state {
        UnitSheathState::Unarmed => false,
        UnitSheathState::Melee => matches!(
            item.slot(),
            PlayerEquipmentSlot::MainHand | PlayerEquipmentSlot::OffHand
        ),
        UnitSheathState::Ranged => item.slot() == PlayerEquipmentSlot::Ranged,
    };
    if !ready {
        let definition = required_definition(item)?;
        return Ok(sheath_point(definition.sheathe_type(), right_hand));
    }
    let definition = required_definition(item)?;
    if definition.inventory_type() == InventoryType::Shield {
        return Ok(Some(CharacterAttachmentPoint::Shield));
    }
    Ok(Some(if right_hand {
        CharacterAttachmentPoint::HandRight
    } else {
        CharacterAttachmentPoint::HandLeft
    }))
}

/// Selects the stock ranged grip from the closed weapon-subclass domain.
fn ranged_item_uses_right_hand(
    item: CharacterEquipmentItem<'_>,
) -> Result<bool, CharacterAttachmentPlanError> {
    let definition = required_definition(item)?;
    if definition.class_id() != 2 {
        return Err(CharacterAttachmentPlanError::UnsupportedRangedItem {
            class_id: definition.class_id(),
            subclass_id: definition.subclass_id(),
        });
    }
    match definition.subclass_id() {
        2 => Ok(false),
        3 | 16 | 18 | 19 => Ok(true),
        subclass_id => Err(CharacterAttachmentPlanError::UnsupportedRangedItem {
            class_id: definition.class_id(),
            subclass_id,
        }),
    }
}

/// Requires Item.dbc metadata only for the three held-equipment paths.
fn required_definition(
    item: CharacterEquipmentItem<'_>,
) -> Result<&ItemDefinition, CharacterAttachmentPlanError> {
    item.definition()
        .ok_or(CharacterAttachmentPlanError::MissingItemDefinition { slot: item.slot() })
}

/// Builds one optional replacement path without fabricating an empty filename.
fn attachment_texture(
    folder: &str,
    texture_name: &str,
) -> Result<Option<AssetPath>, CharacterAttachmentPlanError> {
    if texture_name.is_empty() {
        return Ok(None);
    }
    Ok(Some(AssetPath::new(format!(
        "{folder}\\{texture_name}.blp"
    ))?))
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
