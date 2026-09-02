//! UI-owned character directory values exposed synchronously to Glue Lua.

const CHARACTER_FLAG_GHOST: u32 = 0x0000_2000;
const CHARACTER_FLAG_RENAME: u32 = 0x0000_4000;
const CUSTOMIZE_CHARACTER: u32 = 0x0000_0001;
const CHANGE_FACTION: u32 = 0x0001_0000;
const CHANGE_RACE: u32 = 0x0010_0000;
const CHARACTER_EQUIPMENT_SLOT_COUNT: usize = 23;

/// One display-only equipment row carried by `SMSG_CHAR_ENUM`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UiCharacterEquipment {
    display_id: u32,
    inventory_type_id: u8,
    enchantment_visual_id: u32,
}

impl UiCharacterEquipment {
    /// Captures the three exact character-enumeration equipment fields.
    #[must_use]
    pub const fn new(display_id: u32, inventory_type_id: u8, enchantment_visual_id: u32) -> Self {
        Self {
            display_id,
            inventory_type_id,
            enchantment_visual_id,
        }
    }

    /// Returns the `ItemDisplayInfo.dbc` identifier.
    #[must_use]
    pub const fn display_id(self) -> u32 {
        self.display_id
    }

    /// Returns the build-12340 inventory-type identifier.
    #[must_use]
    pub const fn inventory_type_id(self) -> u8 {
        self.inventory_type_id
    }

    /// Returns the selection-scene item visual override.
    #[must_use]
    pub const fn enchantment_visual_id(self) -> u32 {
        self.enchantment_visual_id
    }
}

/// Pet preview fields carried beside one character-enumeration row.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UiCharacterPetPreview {
    display_id: u32,
    level: u8,
    family_id: u8,
}

impl UiCharacterPetPreview {
    /// Captures the exact character-selection pet fields.
    #[must_use]
    pub const fn new(display_id: u32, level: u8, family_id: u8) -> Self {
        Self {
            display_id,
            level,
            family_id,
        }
    }

    /// Returns the creature display identifier, or zero for no pet.
    #[must_use]
    pub const fn display_id(self) -> u32 {
        self.display_id
    }

    /// Returns the pet level.
    #[must_use]
    pub const fn level(self) -> u8 {
        self.level
    }

    /// Returns the creature-family identifier.
    #[must_use]
    pub const fn family_id(self) -> u8 {
        self.family_id
    }
}

/// Current selected-character model inputs consumed by the pre-world renderer.
#[derive(Clone, Debug, PartialEq)]
pub struct UiCharacterSelectionPreview {
    guid: u64,
    race_id: u8,
    class_id: u8,
    gender_id: u8,
    appearance: [u8; 5],
    equipment: [UiCharacterEquipment; CHARACTER_EQUIPMENT_SLOT_COUNT],
    pet: UiCharacterPetPreview,
    facing_degrees: f64,
}

impl UiCharacterSelectionPreview {
    /// Returns the selected world-object GUID.
    #[must_use]
    pub const fn guid(&self) -> u64 {
        self.guid
    }

    /// Returns the protocol race identifier.
    #[must_use]
    pub const fn race_id(&self) -> u8 {
        self.race_id
    }

    /// Returns the protocol class identifier.
    #[must_use]
    pub const fn class_id(&self) -> u8 {
        self.class_id
    }

    /// Returns the zero-based protocol gender identifier.
    #[must_use]
    pub const fn gender_id(&self) -> u8 {
        self.gender_id
    }

    /// Returns skin, face, hair style, hair color, and facial-hair bytes.
    #[must_use]
    pub const fn appearance(&self) -> [u8; 5] {
        self.appearance
    }

    /// Returns all 23 character-enumeration equipment records in wire order.
    #[must_use]
    pub const fn equipment(&self) -> &[UiCharacterEquipment; CHARACTER_EQUIPMENT_SLOT_COUNT] {
        &self.equipment
    }

    /// Returns the selected character's pet preview.
    #[must_use]
    pub const fn pet(&self) -> UiCharacterPetPreview {
        self.pet
    }

    /// Returns the Glue-controlled character facing in degrees.
    #[must_use]
    pub const fn facing_degrees(&self) -> f64 {
        self.facing_degrees
    }
}

/// One character row projected from the authenticated world response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiCharacterInfo {
    guid: u64,
    name: String,
    race_name: String,
    race_id: u8,
    background_model: String,
    class_name: String,
    class_id: u8,
    level: u8,
    zone_name: Option<String>,
    sex: u8,
    gender_id: u8,
    appearance: [u8; 5],
    equipment: [UiCharacterEquipment; CHARACTER_EQUIPMENT_SLOT_COUNT],
    pet: UiCharacterPetPreview,
    flags: u32,
    customization_flags: u32,
}

impl UiCharacterInfo {
    /// Captures one complete row required by stock character-selection globals.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        guid: u64,
        name: String,
        race_name: String,
        race_id: u8,
        background_model: String,
        class_name: String,
        class_id: u8,
        level: u8,
        zone_name: Option<String>,
        sex: u8,
        gender_id: u8,
        appearance: [u8; 5],
        equipment: [UiCharacterEquipment; CHARACTER_EQUIPMENT_SLOT_COUNT],
        pet: UiCharacterPetPreview,
        flags: u32,
        customization_flags: u32,
    ) -> Self {
        Self {
            guid,
            name,
            race_name,
            race_id,
            background_model,
            class_name,
            class_id,
            level,
            zone_name,
            sex,
            gender_id,
            appearance,
            equipment,
            pet,
            flags,
            customization_flags,
        }
    }

    /// Returns the character GUID used as Glue's stable character ID.
    #[must_use]
    pub const fn guid(&self) -> u64 {
        self.guid
    }

    /// Returns the server-authored character name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the selected-locale ChrRaces display name.
    #[must_use]
    pub fn race_name(&self) -> &str {
        &self.race_name
    }

    /// Returns stock's class/race-remapped background filename component.
    #[must_use]
    pub fn background_model(&self) -> &str {
        &self.background_model
    }

    /// Returns the selected-locale ChrClasses display name.
    #[must_use]
    pub fn class_name(&self) -> &str {
        &self.class_name
    }

    /// Returns the class protocol identifier.
    #[must_use]
    pub const fn class_id(&self) -> u8 {
        self.class_id
    }

    /// Returns the server-authored level.
    #[must_use]
    pub const fn level(&self) -> u8 {
        self.level
    }

    /// Returns the selected-locale AreaTable name.
    #[must_use]
    pub fn zone_name(&self) -> Option<&str> {
        self.zone_name.as_deref()
    }

    /// Returns stock's Lua sex token: 2 male, 3 female, or 1 neutral.
    #[must_use]
    pub const fn sex(&self) -> u8 {
        self.sex
    }

    /// Reports whether character selection presents this row as a ghost.
    #[must_use]
    pub const fn is_ghost(&self) -> bool {
        self.flags & CHARACTER_FLAG_GHOST != 0
    }

    /// Reports whether the world requires a rename before login.
    #[must_use]
    pub const fn requires_rename(&self) -> bool {
        self.flags & CHARACTER_FLAG_RENAME != 0
    }

    /// Reports the paid-character-customization branch.
    #[must_use]
    pub const fn has_paid_customization(&self) -> bool {
        self.customization_flags & CUSTOMIZE_CHARACTER != 0
    }

    /// Reports the paid-race-change branch.
    #[must_use]
    pub const fn has_paid_race_change(&self) -> bool {
        self.customization_flags & CHANGE_RACE != 0
    }

    /// Reports the paid-faction-change branch.
    #[must_use]
    pub const fn has_paid_faction_change(&self) -> bool {
        self.customization_flags & CHANGE_FACTION != 0
    }
}

/// Complete character state queried synchronously by stock Glue Lua.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UiCharacterDirectory {
    characters: Vec<UiCharacterInfo>,
    selected_guid: Option<u64>,
    default_background_model: String,
    facing_radians_bits: u32,
}

impl UiCharacterDirectory {
    /// Captures server order and initially selects the first returned row.
    #[must_use]
    pub fn new(characters: Vec<UiCharacterInfo>, default_background_model: String) -> Self {
        let selected_guid = characters.first().map(UiCharacterInfo::guid);
        Self {
            characters,
            selected_guid,
            default_background_model,
            facing_radians_bits: 0.0_f32.to_bits(),
        }
    }

    /// Returns character rows in server order.
    #[must_use]
    pub fn characters(&self) -> &[UiCharacterInfo] {
        &self.characters
    }

    /// Returns the current selected GUID.
    #[must_use]
    pub const fn selected_guid(&self) -> Option<u64> {
        self.selected_guid
    }

    pub(crate) fn default_background_model(&self) -> &str {
        &self.default_background_model
    }

    pub(crate) fn by_index(&self, one_based_index: u32) -> Option<&UiCharacterInfo> {
        let index = usize::try_from(one_based_index.checked_sub(1)?).ok()?;
        self.characters.get(index)
    }

    pub(crate) fn index_of(&self, guid: u64) -> Option<u32> {
        self.characters
            .iter()
            .position(|character| character.guid == guid)
            .and_then(|index| u32::try_from(index + 1).ok())
    }

    pub(crate) fn select_index(&mut self, one_based_index: u32) -> u32 {
        // Wow.exe 0x004E4580 converts the Lua index to zero-based form and
        // normalizes every out-of-range value to the first row before event 8
        // publishes the corresponding one-based index.
        let normalized_index = self
            .by_index(one_based_index)
            .map_or(1, |_character| one_based_index);
        self.selected_guid = self.by_index(normalized_index).map(UiCharacterInfo::guid);
        normalized_index
    }

    pub(crate) fn facing_degrees(&self) -> f64 {
        f64::from(f32::from_bits(self.facing_radians_bits).to_degrees())
    }

    pub(crate) fn set_facing_degrees(&mut self, facing_degrees: f64) {
        // Wow.exe 0x004E3030 narrows the Lua number to a float, multiplies by
        // the stock degrees-to-radians constant, and stores that native value.
        let radians = (facing_degrees as f32).to_radians();
        self.facing_radians_bits = radians.to_bits();
    }

    pub(crate) fn selection_preview(&self) -> Option<UiCharacterSelectionPreview> {
        let character = self
            .selected_guid
            .and_then(|guid| self.characters.iter().find(|row| row.guid == guid))?;
        Some(UiCharacterSelectionPreview {
            guid: character.guid,
            race_id: character.race_id,
            class_id: character.class_id,
            gender_id: character.gender_id,
            appearance: character.appearance,
            equipment: character.equipment,
            pet: character.pet,
            facing_degrees: self.facing_degrees(),
        })
    }
}
