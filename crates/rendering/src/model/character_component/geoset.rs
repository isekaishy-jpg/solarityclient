//! Exact build-12340 character body-geoset selection.

use solarity_asset::{CharacterModelAppearance, HelmetGeosetVisibilityCatalog, ItemDisplayInfo};
use solarity_ecs::PlayerEquipmentSlot;

use super::{CharacterEquipmentItem, CharacterGeosetPlanError};

const DEATH_KNIGHT_CLASS_ID: u8 = 6;
const FACE_FLAG_GLOWING_EYES: u32 = 0x04;
const CHARACTER_GEOSET_SLOT_COUNT: usize = 19;
const EQUIPMENT_GEOSET_SLOT_COUNT: usize = 11;
const BASE_GEOSETS: [u32; CHARACTER_GEOSET_SLOT_COUNT] = [
    1, 101, 201, 301, 401, 501, 601, 702, 801, 901, 1001, 1101, 1201, 1301, 1401, 1501, 1601, 1701,
    1801,
];

/// Selects whether stock's separately composed guild-tabard geometry is active.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CharacterTabardMode {
    /// Use only the ordinary equipped `ItemDisplayInfo` tabard selection.
    #[default]
    Equipment,
    /// Show the stock custom guild-tabard torso and lower-torso geometry.
    CustomGuild,
}

/// Non-DBC character state that participates in stock geoset selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharacterGeosetContext {
    class_id: u8,
    tabard_mode: CharacterTabardMode,
}

impl CharacterGeosetContext {
    /// Creates context from authoritative unit class and tabard state.
    #[must_use]
    pub const fn new(class_id: u8, tabard_mode: CharacterTabardMode) -> Self {
        Self {
            class_id,
            tabard_mode,
        }
    }

    /// Returns the unit class byte used for death-knight eye geometry.
    #[must_use]
    pub const fn class_id(self) -> u8 {
        self.class_id
    }

    /// Returns the selected stock tabard path.
    #[must_use]
    pub const fn tabard_mode(self) -> CharacterTabardMode {
        self.tabard_mode
    }
}

/// Final enabled M2 submesh identifiers after stock character preparation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacterGeosetPlan {
    visible_geosets: Vec<u32>,
}

impl CharacterGeosetPlan {
    /// Selects customization, helmet masks, and equipped-item body geosets.
    ///
    /// The implementation follows build 12340's `CCharacterComponent` order.
    /// Order matters because robe, tabard, boot, and leg branches hide ranges
    /// selected by earlier defaults before enabling their authored variants.
    ///
    /// # Errors
    ///
    /// Returns [`CharacterGeosetPlanError`] when a model with a helmet mask has
    /// a gender outside stock's two `ItemDisplayInfo` visibility columns.
    pub fn equipped<'catalog, I>(
        appearance: &CharacterModelAppearance<'_>,
        context: CharacterGeosetContext,
        helmet_visibility: &HelmetGeosetVisibilityCatalog,
        equipment: I,
    ) -> Result<Self, CharacterGeosetPlanError>
    where
        I: IntoIterator<Item = CharacterEquipmentItem<'catalog>>,
    {
        let equipment = collect_equipment(equipment);
        let mut visible_geosets = base_geosets(appearance);
        apply_helmet_visibility(
            &mut visible_geosets,
            appearance,
            helmet_visibility,
            display(&equipment, 0),
        )?;
        apply_eye_glow(&mut visible_geosets, appearance, context.class_id());
        apply_equipment_geosets(&mut visible_geosets, context, &equipment);
        Ok(Self { visible_geosets })
    }

    /// Returns the enabled M2 submesh identifiers in stock selection order.
    #[must_use]
    pub fn visible_geosets(&self) -> &[u32] {
        &self.visible_geosets
    }

    /// Tests whether one M2 submesh identifier survives character preparation.
    #[must_use]
    pub fn is_visible(&self, geoset_id: u16) -> bool {
        self.visible_geosets.contains(&u32::from(geoset_id))
    }
}

/// Builds the nineteen persistent character component slots plus body geoset zero.
fn base_geosets(appearance: &CharacterModelAppearance<'_>) -> Vec<u32> {
    let mut slots = BASE_GEOSETS;
    let customization = appearance.geosets();
    slots[0] = customization.hair();
    if let Some(facial_hair) = customization.facial_hair() {
        slots[1] = facial_hair[0];
        slots[2] = facial_hair[1];
        slots[3] = facial_hair[2];
        slots[16] = facial_hair[3];
        slots[17] = facial_hair[4];
    }

    let mut visible = Vec::with_capacity(32);
    visible.push(0);
    for geoset in slots {
        show(&mut visible, geoset);
    }
    visible
}

/// Applies the gender-selected helmet row to hair, face, ears, and eye slots.
fn apply_helmet_visibility(
    visible: &mut Vec<u32>,
    appearance: &CharacterModelAppearance<'_>,
    catalog: &HelmetGeosetVisibilityCatalog,
    head: Option<&ItemDisplayInfo>,
) -> Result<(), CharacterGeosetPlanError> {
    let Some(head) = head else {
        return Ok(());
    };
    let visibility_ids = head.helmet_geoset_visibility_ids();
    let gender_index = usize::try_from(appearance.gender_id()).map_err(|_error| {
        CharacterGeosetPlanError::UnsupportedHelmetGender {
            gender_id: appearance.gender_id(),
        }
    })?;
    let visibility_id = *visibility_ids.get(gender_index).ok_or(
        CharacterGeosetPlanError::UnsupportedHelmetGender {
            gender_id: appearance.gender_id(),
        },
    )?;
    let Some(mask) = catalog.visibility(visibility_id) else {
        // Stock performs a checked DBC lookup and leaves every slot unchanged
        // when the authored visibility identifier is zero or absent.
        return Ok(());
    };
    // The original x86 shift masks its count to five bits.
    let race_bit = 1_u32 << (appearance.race_id() & 0x1f);
    let hidden_slots = [
        (mask.hair_flags(), 0_u32, 1_u32),
        (mask.facial_hair_flags()[0], 1, 101),
        (mask.facial_hair_flags()[1], 2, 201),
        (mask.facial_hair_flags()[2], 3, 301),
        (mask.ear_flags(), 7, 701),
        (mask.additional_flags()[0], 16, 1601),
        (mask.additional_flags()[1], 17, 1701),
    ];
    for (flags, group, replacement) in hidden_slots {
        if flags & race_bit != 0 {
            hide_group(visible, group);
            show(visible, replacement);
        }
    }
    Ok(())
}

/// Applies the final eye slot substitution performed after helmet masking.
fn apply_eye_glow(visible: &mut Vec<u32>, appearance: &CharacterModelAppearance<'_>, class_id: u8) {
    let face_has_glowing_eyes = appearance
        .face()
        .is_some_and(|face| face.flags() & FACE_FLAG_GLOWING_EYES != 0);
    if class_id == DEATH_KNIGHT_CLASS_ID || face_has_glowing_eyes {
        hide_group(visible, 17);
        show(visible, 1703);
    }
}

/// Replays build 12340's item-geoset branches in their executable order.
fn apply_equipment_geosets(
    visible: &mut Vec<u32>,
    context: CharacterGeosetContext,
    equipment: &[Option<CharacterEquipmentItem<'_>>; EQUIPMENT_GEOSET_SLOT_COUNT],
) {
    let shirt = display(equipment, 2);
    let chest = display(equipment, 3);
    let waist = display(equipment, 4);
    let legs = display(equipment, 5);
    let feet = display(equipment, 6);
    let hands = display(equipment, 8);
    let tabard = display(equipment, 9);
    let cape = display(equipment, 10);

    // Gloves replace the hand group and take precedence over chest sleeves.
    if let Some(selector) = geoset(hands, 0) {
        hide_range(visible, 401, 499);
        show(visible, 401 + selector);
    } else if let Some(selector) = geoset(chest, 0) {
        show(visible, 801 + selector);
    }

    // Shirt sleeves are shown only while no outer item occupies an arm-lower
    // texture priority. This is the exact meaning of component mask +0x244.
    if !has_outer_arm_lower_texture(equipment)
        && let Some(selector) = geoset(shirt, 0)
    {
        show(visible, 801 + selector);
    }

    let chest_robe = geoset(chest, 2);
    let legs_robe = chest_robe.is_none().then(|| geoset(legs, 2)).flatten();
    if let Some(selector) = chest_robe.or(legs_robe) {
        apply_robe(visible, selector);
    } else {
        apply_non_robe_lower_body(visible, feet, legs);
    }

    let mut has_item_tabard = false;
    if chest_robe.is_none()
        && legs_robe.is_none()
        && let Some(selector) = geoset(tabard, 0)
    {
        show(visible, 1201 + selector);
        has_item_tabard = true;
    }

    if context.tabard_mode() == CharacterTabardMode::CustomGuild {
        show(visible, 1201);
        if chest_robe.is_none() && legs_robe.is_none() {
            show(visible, 1202);
        }
    }

    // A chest robe jumps directly to cape and waist preparation in stock.
    if chest_robe.is_none() {
        if !has_item_tabard && let Some(selector) = geoset(shirt, 1) {
            show(visible, 1001 + selector);
        }
        if let Some(selector) = geoset(legs, 0)
            && (selector >= 3 || !has_item_tabard)
        {
            if selector >= 3 {
                hide_range(visible, 1300, 1399);
            }
            show(visible, 1101 + selector);
        }
    }

    if let Some(selector) = geoset(cape, 0) {
        hide_range(visible, 1500, 1599);
        show(visible, 1501 + selector);
    }
    if let Some(selector) = geoset(waist, 0) {
        hide_range(visible, 1800, 1899);
        show(visible, 1801 + selector);
    }
}

/// Applies the mutually exclusive boot and trouser geometry branch.
fn apply_non_robe_lower_body(
    visible: &mut Vec<u32>,
    feet: Option<&ItemDisplayInfo>,
    legs: Option<&ItemDisplayInfo>,
) {
    if let Some(selector) = geoset(feet, 0) {
        hide_range(visible, 501, 599);
        show(visible, 901);
        show(visible, 501 + selector);
    }
    show(visible, 901 + geoset(legs, 1).unwrap_or(0));
}

/// Hides the stock lower-body groups covered by a robe and enables its variant.
fn apply_robe(visible: &mut Vec<u32>, selector: u32) {
    hide_range(visible, 501, 599);
    hide_range(visible, 902, 999);
    hide_range(visible, 1100, 1199);
    hide_range(visible, 1300, 1399);
    show(visible, 1301 + selector);
}

/// Collects last-write-wins public slots into CCharacterComponent geoset rows.
fn collect_equipment<'catalog, I>(
    equipment: I,
) -> [Option<CharacterEquipmentItem<'catalog>>; EQUIPMENT_GEOSET_SLOT_COUNT]
where
    I: IntoIterator<Item = CharacterEquipmentItem<'catalog>>,
{
    let mut slots = [None; EQUIPMENT_GEOSET_SLOT_COUNT];
    for item in equipment {
        if let Some(slot) = geoset_slot(item.slot()) {
            slots[slot] = Some(item);
        }
    }
    slots
}

/// Maps public visible equipment to the eleven body-geoset display fields.
const fn geoset_slot(slot: PlayerEquipmentSlot) -> Option<usize> {
    match slot {
        PlayerEquipmentSlot::Head => Some(0),
        PlayerEquipmentSlot::Shoulders => Some(1),
        PlayerEquipmentSlot::Shirt => Some(2),
        PlayerEquipmentSlot::Chest => Some(3),
        PlayerEquipmentSlot::Waist => Some(4),
        PlayerEquipmentSlot::Legs => Some(5),
        PlayerEquipmentSlot::Feet => Some(6),
        PlayerEquipmentSlot::Wrists => Some(7),
        PlayerEquipmentSlot::Hands => Some(8),
        PlayerEquipmentSlot::Tabard => Some(9),
        PlayerEquipmentSlot::Back => Some(10),
        PlayerEquipmentSlot::Neck
        | PlayerEquipmentSlot::FingerOne
        | PlayerEquipmentSlot::FingerTwo
        | PlayerEquipmentSlot::TrinketOne
        | PlayerEquipmentSlot::TrinketTwo
        | PlayerEquipmentSlot::MainHand
        | PlayerEquipmentSlot::OffHand
        | PlayerEquipmentSlot::Ranged => None,
    }
}

/// Returns one collected display without exposing the internal slot array.
fn display<'catalog>(
    equipment: &[Option<CharacterEquipmentItem<'catalog>>; EQUIPMENT_GEOSET_SLOT_COUNT],
    slot: usize,
) -> Option<&'catalog ItemDisplayInfo> {
    equipment[slot].map(CharacterEquipmentItem::display)
}

/// Returns one nonzero authored selector; zero means no geoset operation.
fn geoset(display: Option<&ItemDisplayInfo>, index: usize) -> Option<u32> {
    display
        .map(ItemDisplayInfo::geoset_groups)
        .map(|groups| groups[index])
        .filter(|selector| *selector != 0)
}

/// Tests the exact arm-lower priority mask range used to suppress shirt sleeves.
fn has_outer_arm_lower_texture(
    equipment: &[Option<CharacterEquipmentItem<'_>>; EQUIPMENT_GEOSET_SLOT_COUNT],
) -> bool {
    [3_usize, 7, 8].into_iter().any(|slot| {
        equipment[slot].is_some_and(|item| !item.display().component_textures()[1].is_empty())
    })
}

/// Replaces every currently visible member of one hundred-ID character group.
fn hide_group(visible: &mut Vec<u32>, group: u32) {
    let first = if group == 0 { 1 } else { group * 100 };
    hide_range(visible, first, group * 100 + 99);
}

/// Applies stock's inclusive geoset-range disable operation.
fn hide_range(visible: &mut Vec<u32>, first: u32, last: u32) {
    visible.retain(|geoset| *geoset < first || *geoset > last);
}

/// Enables one geoset while keeping the final plan compact and deterministic.
fn show(visible: &mut Vec<u32>, geoset: u32) {
    if !visible.contains(&geoset) {
        visible.push(geoset);
    }
}
