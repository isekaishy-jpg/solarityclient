//! UI-owned character directory values exposed synchronously to Glue Lua.

const CHARACTER_FLAG_GHOST: u32 = 0x0000_2000;
const CUSTOMIZE_CHARACTER: u32 = 0x0000_0001;
const CHANGE_FACTION: u32 = 0x0001_0000;
const CHANGE_RACE: u32 = 0x0010_0000;

/// One character row projected from the authenticated world response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiCharacterInfo {
    guid: u64,
    name: String,
    race_name: String,
    race_file_string: String,
    class_name: String,
    class_id: u8,
    level: u8,
    zone_name: Option<String>,
    sex: u8,
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
        race_file_string: String,
        class_name: String,
        class_id: u8,
        level: u8,
        zone_name: Option<String>,
        sex: u8,
        flags: u32,
        customization_flags: u32,
    ) -> Self {
        Self {
            guid,
            name,
            race_name,
            race_file_string,
            class_name,
            class_id,
            level,
            zone_name,
            sex,
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

    /// Returns the race's model/background filename component.
    #[must_use]
    pub fn race_file_string(&self) -> &str {
        &self.race_file_string
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

    /// Reports the paid-character-customization branch.
    #[must_use]
    pub const fn has_paid_customization(&self) -> bool {
        self.customization_flags == CUSTOMIZE_CHARACTER
    }

    /// Reports the paid-race-change branch.
    #[must_use]
    pub const fn has_paid_race_change(&self) -> bool {
        self.customization_flags == CHANGE_RACE
    }

    /// Reports the paid-faction-change branch.
    #[must_use]
    pub const fn has_paid_faction_change(&self) -> bool {
        self.customization_flags == CHANGE_FACTION
    }
}

/// Complete character state queried synchronously by stock Glue Lua.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UiCharacterDirectory {
    characters: Vec<UiCharacterInfo>,
    selected_guid: Option<u64>,
}

impl UiCharacterDirectory {
    /// Captures server order and initially selects the first returned row.
    #[must_use]
    pub fn new(characters: Vec<UiCharacterInfo>) -> Self {
        let selected_guid = characters.first().map(UiCharacterInfo::guid);
        Self {
            characters,
            selected_guid,
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

    pub(crate) fn select_index(&mut self, one_based_index: u32) -> Option<u64> {
        let guid = self.by_index(one_based_index).map(UiCharacterInfo::guid)?;
        self.selected_guid = Some(guid);
        Some(guid)
    }
}
