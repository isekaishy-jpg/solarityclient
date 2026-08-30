//! Owned character-selection state decoded from `SMSG_CHAR_ENUM`.

use thiserror::Error;

/// A build-12340 playable or protocol-defined character race.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CharacterRace {
    /// Human.
    Human,
    /// Orc.
    Orc,
    /// Dwarf.
    Dwarf,
    /// Night elf.
    NightElf,
    /// Undead.
    Undead,
    /// Tauren.
    Tauren,
    /// Gnome.
    Gnome,
    /// Troll.
    Troll,
    /// Goblin protocol value.
    Goblin,
    /// Blood elf.
    BloodElf,
    /// Draenei.
    Draenei,
    /// Fel orc protocol value.
    FelOrc,
    /// Naga protocol value.
    Naga,
    /// Broken protocol value.
    Broken,
    /// Skeleton protocol value.
    Skeleton,
    /// Vrykul protocol value.
    Vrykul,
    /// Tuskarr protocol value.
    Tuskarr,
    /// Forest troll protocol value.
    ForestTroll,
    /// Taunka protocol value.
    Taunka,
    /// Northrend skeleton protocol value.
    NorthrendSkeleton,
    /// Ice troll protocol value.
    IceTroll,
}

impl CharacterRace {
    /// Returns the build-12340 ChrRaces identifier.
    #[must_use]
    pub const fn protocol_id(self) -> u8 {
        match self {
            Self::Human => 1,
            Self::Orc => 2,
            Self::Dwarf => 3,
            Self::NightElf => 4,
            Self::Undead => 5,
            Self::Tauren => 6,
            Self::Gnome => 7,
            Self::Troll => 8,
            Self::Goblin => 9,
            Self::BloodElf => 10,
            Self::Draenei => 11,
            Self::FelOrc => 12,
            Self::Naga => 13,
            Self::Broken => 14,
            Self::Skeleton => 15,
            Self::Vrykul => 16,
            Self::Tuskarr => 17,
            Self::ForestTroll => 18,
            Self::Taunka => 19,
            Self::NorthrendSkeleton => 20,
            Self::IceTroll => 21,
        }
    }
}

/// A build-12340 character class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CharacterClass {
    /// Warrior.
    Warrior,
    /// Paladin.
    Paladin,
    /// Hunter.
    Hunter,
    /// Rogue.
    Rogue,
    /// Priest.
    Priest,
    /// Death knight.
    DeathKnight,
    /// Shaman.
    Shaman,
    /// Mage.
    Mage,
    /// Warlock.
    Warlock,
    /// Druid.
    Druid,
}

impl CharacterClass {
    /// Returns the build-12340 ChrClasses identifier.
    #[must_use]
    pub const fn protocol_id(self) -> u8 {
        match self {
            Self::Warrior => 1,
            Self::Paladin => 2,
            Self::Hunter => 3,
            Self::Rogue => 4,
            Self::Priest => 5,
            Self::DeathKnight => 6,
            Self::Shaman => 7,
            Self::Mage => 8,
            Self::Warlock => 9,
            Self::Druid => 11,
        }
    }
}

/// Character model gender from the character-selection packet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CharacterGender {
    /// Male model.
    Male,
    /// Female model.
    Female,
    /// Genderless protocol value, normally used for pets.
    None,
}

impl CharacterGender {
    /// Returns the build-12340 model-gender identifier.
    #[must_use]
    pub const fn protocol_id(self) -> u8 {
        match self {
            Self::Male => 0,
            Self::Female => 1,
            Self::None => 2,
        }
    }
}

/// Character appearance fields needed to construct the selection model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharacterAppearance {
    race: CharacterRace,
    class: CharacterClass,
    gender: CharacterGender,
    skin: u8,
    face: u8,
    hair_style: u8,
    hair_color: u8,
    facial_hair: u8,
}

impl CharacterAppearance {
    /// Returns the character race.
    #[must_use]
    pub const fn race(self) -> CharacterRace {
        self.race
    }

    /// Returns the character class.
    #[must_use]
    pub const fn class(self) -> CharacterClass {
        self.class
    }

    /// Returns the character model gender.
    #[must_use]
    pub const fn gender(self) -> CharacterGender {
        self.gender
    }

    /// Returns the skin customization index.
    #[must_use]
    pub const fn skin(self) -> u8 {
        self.skin
    }

    /// Returns the face customization index.
    #[must_use]
    pub const fn face(self) -> u8 {
        self.face
    }

    /// Returns the hair-style customization index.
    #[must_use]
    pub const fn hair_style(self) -> u8 {
        self.hair_style
    }

    /// Returns the hair-color customization index.
    #[must_use]
    pub const fn hair_color(self) -> u8 {
        self.hair_color
    }

    /// Returns the facial-hair customization index.
    #[must_use]
    pub const fn facial_hair(self) -> u8 {
        self.facial_hair
    }
}

/// Last known world location shown at character selection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterLocation {
    area_id: u32,
    map_id: u32,
    x: f32,
    y: f32,
    z: f32,
}

impl CharacterLocation {
    /// Returns the AreaTable identifier.
    #[must_use]
    pub const fn area_id(self) -> u32 {
        self.area_id
    }

    /// Returns the Map identifier.
    #[must_use]
    pub const fn map_id(self) -> u32 {
        self.map_id
    }

    /// Returns the world X coordinate.
    #[must_use]
    pub const fn x(self) -> f32 {
        self.x
    }

    /// Returns the world Y coordinate.
    #[must_use]
    pub const fn y(self) -> f32 {
        self.y
    }

    /// Returns the world Z coordinate.
    #[must_use]
    pub const fn z(self) -> f32 {
        self.z
    }
}

/// One of the 23 equipment display slots in character-selection order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharacterEquipment {
    display_id: u32,
    inventory_type_id: u8,
    enchantment: u32,
}

impl CharacterEquipment {
    /// Returns the ItemDisplayInfo identifier.
    #[must_use]
    pub const fn display_id(self) -> u32 {
        self.display_id
    }

    /// Returns the raw build-12340 inventory-type identifier.
    #[must_use]
    pub const fn inventory_type_id(self) -> u8 {
        self.inventory_type_id
    }

    /// Returns the visible enchantment identifier.
    #[must_use]
    pub const fn enchantment(self) -> u32 {
        self.enchantment
    }
}

/// Pet preview state attached to a character row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharacterPet {
    display_id: u32,
    level: u8,
    family_id: u8,
}

impl CharacterPet {
    /// Returns the creature display identifier, or zero when no pet is shown.
    #[must_use]
    pub const fn display_id(self) -> u32 {
        self.display_id
    }

    /// Returns the pet level.
    #[must_use]
    pub const fn level(self) -> u8 {
        self.level
    }

    /// Returns the build-12340 creature-family identifier.
    #[must_use]
    pub const fn family_id(self) -> u8 {
        self.family_id
    }
}

/// One owned character-selection row.
#[derive(Clone, Debug, PartialEq)]
pub struct CharacterEntry {
    guid: u64,
    name: String,
    appearance: CharacterAppearance,
    level: u8,
    location: CharacterLocation,
    guild_id: u32,
    flags: u32,
    recustomization_flags: u32,
    first_login: bool,
    pet: CharacterPet,
    equipment: [CharacterEquipment; 23],
}

impl CharacterEntry {
    /// Returns the world object GUID used for `CMSG_PLAYER_LOGIN`.
    #[must_use]
    pub const fn guid(&self) -> u64 {
        self.guid
    }

    /// Returns the character name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns character appearance and class identity.
    #[must_use]
    pub const fn appearance(&self) -> CharacterAppearance {
        self.appearance
    }

    /// Returns the character level.
    #[must_use]
    pub const fn level(&self) -> u8 {
        self.level
    }

    /// Returns the last known world location.
    #[must_use]
    pub const fn location(&self) -> CharacterLocation {
        self.location
    }

    /// Returns the guild identifier, or zero when unguilded.
    #[must_use]
    pub const fn guild_id(&self) -> u32 {
        self.guild_id
    }

    /// Returns the exact character-selection flag bits.
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }

    /// Returns the exact recustomization flag bits.
    #[must_use]
    pub const fn recustomization_flags(&self) -> u32 {
        self.recustomization_flags
    }

    /// Returns whether this character has never entered the world.
    #[must_use]
    pub const fn is_first_login(&self) -> bool {
        self.first_login
    }

    /// Returns pet preview state.
    #[must_use]
    pub const fn pet(&self) -> CharacterPet {
        self.pet
    }

    /// Returns all equipment displays in exact stock slot order.
    #[must_use]
    pub const fn equipment(&self) -> &[CharacterEquipment; 23] {
        &self.equipment
    }
}

/// A malformed `SMSG_CHAR_ENUM` body.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("malformed character directory at byte {offset}: {message}")]
pub struct CharacterDirectoryError {
    offset: usize,
    message: &'static str,
}

impl CharacterDirectoryError {
    /// Returns the byte offset at which decoding failed.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }

    /// Returns a stable description of the rejected field.
    #[must_use]
    pub const fn message(&self) -> &'static str {
        self.message
    }
}

/// One immutable character-enumeration response in server order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CharacterDirectory {
    entries: Vec<CharacterEntry>,
}

impl CharacterDirectory {
    pub(crate) fn decode(payload: &[u8]) -> Result<Self, CharacterDirectoryError> {
        let mut cursor = BodyCursor::new(payload);
        let count = usize::from(cursor.read_u8("missing character count")?);
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            entries.push(decode_character(&mut cursor)?);
        }
        if !cursor.is_empty() {
            return Err(cursor.error("character directory has trailing bytes"));
        }
        Ok(Self { entries })
    }

    /// Returns character rows in exact server order.
    #[must_use]
    pub fn entries(&self) -> &[CharacterEntry] {
        &self.entries
    }

    /// Finds a character by world object GUID.
    #[must_use]
    pub fn by_guid(&self, guid: u64) -> Option<&CharacterEntry> {
        self.entries.iter().find(|entry| entry.guid == guid)
    }

    /// Finds the first character with an exact display name.
    #[must_use]
    pub fn by_name(&self, name: &str) -> Option<&CharacterEntry> {
        self.entries.iter().find(|entry| entry.name == name)
    }
}

fn decode_character(
    cursor: &mut BodyCursor<'_>,
) -> Result<CharacterEntry, CharacterDirectoryError> {
    let guid = cursor.read_u64("missing character GUID")?;
    let name = cursor.read_cstring("invalid character name")?;
    let race = decode_race(cursor.read_u8("missing character race")?)
        .ok_or_else(|| cursor.error("unknown character race"))?;
    let class = decode_class(cursor.read_u8("missing character class")?)
        .ok_or_else(|| cursor.error("unknown character class"))?;
    let gender = decode_gender(cursor.read_u8("missing character gender")?)
        .ok_or_else(|| cursor.error("unknown character gender"))?;
    let appearance = CharacterAppearance {
        race,
        class,
        gender,
        skin: cursor.read_u8("missing skin index")?,
        face: cursor.read_u8("missing face index")?,
        hair_style: cursor.read_u8("missing hair-style index")?,
        hair_color: cursor.read_u8("missing hair-color index")?,
        facial_hair: cursor.read_u8("missing facial-hair index")?,
    };
    let level = cursor.read_u8("missing character level")?;
    let location = CharacterLocation {
        area_id: cursor.read_u32("missing area identifier")?,
        map_id: cursor.read_u32("missing map identifier")?,
        x: cursor.read_f32("missing position X")?,
        y: cursor.read_f32("missing position Y")?,
        z: cursor.read_f32("missing position Z")?,
    };
    let guild_id = cursor.read_u32("missing guild identifier")?;
    let flags = cursor.read_u32("missing character flags")?;
    let recustomization_flags = cursor.read_u32("missing recustomization flags")?;
    let first_login = cursor.read_u8("missing first-login flag")? != 0;
    let pet_display_id = cursor.read_u32("missing pet display identifier")?;
    let pet_level = narrow_u32(cursor, "missing pet level", "pet level exceeds one byte")?;
    let pet_family = narrow_u32(cursor, "missing pet family", "pet family exceeds one byte")?;
    const EMPTY_GEAR: CharacterEquipment = CharacterEquipment {
        display_id: 0,
        inventory_type_id: 0,
        enchantment: 0,
    };
    let mut equipment = [EMPTY_GEAR; 23];
    for slot in &mut equipment {
        *slot = CharacterEquipment {
            display_id: cursor.read_u32("missing equipment display identifier")?,
            inventory_type_id: cursor.read_u8("missing equipment inventory type")?,
            enchantment: cursor.read_u32("missing equipment enchantment")?,
        };
    }
    Ok(CharacterEntry {
        guid,
        name,
        appearance,
        level,
        location,
        guild_id,
        flags,
        recustomization_flags,
        first_login,
        pet: CharacterPet {
            display_id: pet_display_id,
            level: pet_level,
            family_id: pet_family,
        },
        equipment,
    })
}

fn narrow_u32(
    cursor: &mut BodyCursor<'_>,
    missing: &'static str,
    overflow: &'static str,
) -> Result<u8, CharacterDirectoryError> {
    let value = cursor.read_u32(missing)?;
    u8::try_from(value).map_err(|_| cursor.error(overflow))
}

const fn decode_race(value: u8) -> Option<CharacterRace> {
    Some(match value {
        1 => CharacterRace::Human,
        2 => CharacterRace::Orc,
        3 => CharacterRace::Dwarf,
        4 => CharacterRace::NightElf,
        5 => CharacterRace::Undead,
        6 => CharacterRace::Tauren,
        7 => CharacterRace::Gnome,
        8 => CharacterRace::Troll,
        9 => CharacterRace::Goblin,
        10 => CharacterRace::BloodElf,
        11 => CharacterRace::Draenei,
        12 => CharacterRace::FelOrc,
        13 => CharacterRace::Naga,
        14 => CharacterRace::Broken,
        15 => CharacterRace::Skeleton,
        16 => CharacterRace::Vrykul,
        17 => CharacterRace::Tuskarr,
        18 => CharacterRace::ForestTroll,
        19 => CharacterRace::Taunka,
        20 => CharacterRace::NorthrendSkeleton,
        21 => CharacterRace::IceTroll,
        _ => return None,
    })
}

const fn decode_class(value: u8) -> Option<CharacterClass> {
    Some(match value {
        1 => CharacterClass::Warrior,
        2 => CharacterClass::Paladin,
        3 => CharacterClass::Hunter,
        4 => CharacterClass::Rogue,
        5 => CharacterClass::Priest,
        6 => CharacterClass::DeathKnight,
        7 => CharacterClass::Shaman,
        8 => CharacterClass::Mage,
        9 => CharacterClass::Warlock,
        11 => CharacterClass::Druid,
        _ => return None,
    })
}

const fn decode_gender(value: u8) -> Option<CharacterGender> {
    Some(match value {
        0 => CharacterGender::Male,
        1 => CharacterGender::Female,
        2 => CharacterGender::None,
        _ => return None,
    })
}

struct BodyCursor<'a> {
    payload: &'a [u8],
    offset: usize,
}

impl<'a> BodyCursor<'a> {
    const fn new(payload: &'a [u8]) -> Self {
        Self { payload, offset: 0 }
    }

    fn is_empty(&self) -> bool {
        self.offset == self.payload.len()
    }

    const fn error(&self, message: &'static str) -> CharacterDirectoryError {
        CharacterDirectoryError {
            offset: self.offset,
            message,
        }
    }

    fn take(
        &mut self,
        count: usize,
        message: &'static str,
    ) -> Result<&'a [u8], CharacterDirectoryError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or_else(|| self.error(message))?;
        let bytes = self
            .payload
            .get(self.offset..end)
            .ok_or_else(|| self.error(message))?;
        self.offset = end;
        Ok(bytes)
    }

    fn read_u8(&mut self, message: &'static str) -> Result<u8, CharacterDirectoryError> {
        Ok(self.take(1, message)?[0])
    }

    fn read_u32(&mut self, message: &'static str) -> Result<u32, CharacterDirectoryError> {
        let bytes: [u8; 4] = self
            .take(4, message)?
            .try_into()
            .map_err(|_| self.error(message))?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn read_u64(&mut self, message: &'static str) -> Result<u64, CharacterDirectoryError> {
        let bytes: [u8; 8] = self
            .take(8, message)?
            .try_into()
            .map_err(|_| self.error(message))?;
        Ok(u64::from_le_bytes(bytes))
    }

    fn read_f32(&mut self, message: &'static str) -> Result<f32, CharacterDirectoryError> {
        Ok(f32::from_bits(self.read_u32(message)?))
    }

    fn read_cstring(&mut self, message: &'static str) -> Result<String, CharacterDirectoryError> {
        let remaining = &self.payload[self.offset..];
        let length = remaining
            .iter()
            .position(|byte| *byte == 0)
            .ok_or_else(|| self.error(message))?;
        let bytes = self.take(length, message)?;
        let value = std::str::from_utf8(bytes)
            .map_err(|_| self.error(message))?
            .to_owned();
        self.offset += 1;
        Ok(value)
    }
}
