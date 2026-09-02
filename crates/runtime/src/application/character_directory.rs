//! Projection of protocol character rows through client-authored DBC metadata.

use solarity_asset::{
    AreaTableCatalog, AssetError, AssetStore, CharacterClassCatalog, CharacterRaceCatalog,
};
use solarity_network::{CharacterDirectory, CharacterGender};
use solarity_ui::{
    UiCharacterDirectory, UiCharacterEquipment, UiCharacterInfo, UiCharacterPetPreview,
};
use thiserror::Error;

/// Client-authored labels required by character-selection Glue.
pub(crate) struct RuntimeCharacterMetadata {
    races: CharacterRaceCatalog,
    classes: CharacterClassCatalog,
    areas: AreaTableCatalog,
}

impl RuntimeCharacterMetadata {
    /// Loads selection metadata before the asset stack transfers into Glue.
    pub(crate) fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        Ok(Self {
            races: CharacterRaceCatalog::load(store)?,
            classes: CharacterClassCatalog::load(store)?,
            areas: AreaTableCatalog::load(store)?,
        })
    }

    /// Projects every server row without substituting unknown custom metadata.
    pub(crate) fn project(
        &self,
        directory: &CharacterDirectory,
    ) -> Result<UiCharacterDirectory, CharacterProjectionError> {
        let characters = directory
            .entries()
            .iter()
            .map(|entry| {
                let appearance = entry.appearance();
                let race_id = u32::from(appearance.race().protocol_id());
                let race = self
                    .races
                    .race(race_id)
                    .ok_or(CharacterProjectionError::UnknownRace { id: race_id })?;
                let class_id = appearance.class().protocol_id();
                let class = self
                    .classes
                    .class(u32::from(class_id))
                    .ok_or(CharacterProjectionError::UnknownClass { id: class_id })?;
                let background_model = if class_id == 6 {
                    class.file_string()
                } else {
                    let background_race_id = match race_id {
                        7 => 3,
                        8 => 2,
                        race_id => race_id,
                    };
                    self.races
                        .race(background_race_id)
                        .ok_or(CharacterProjectionError::UnknownRace {
                            id: background_race_id,
                        })?
                        .client_file_string()
                };
                let area_id = entry.location().area_id();
                let zone_name = if area_id == 0 {
                    None
                } else {
                    Some(
                        self.areas
                            .area(area_id)
                            .ok_or(CharacterProjectionError::UnknownArea { id: area_id })?
                            .name()
                            .to_owned(),
                    )
                };
                let equipment = std::array::from_fn(|slot| {
                    let item = entry.equipment()[slot];
                    UiCharacterEquipment::new(
                        item.display_id(),
                        item.inventory_type_id(),
                        item.enchantment(),
                    )
                });
                let pet = entry.pet();
                Ok(UiCharacterInfo::new(
                    entry.guid(),
                    entry.name().to_owned(),
                    race.name().to_owned(),
                    appearance.race().protocol_id(),
                    background_model.to_owned(),
                    class.name().to_owned(),
                    class_id,
                    entry.level(),
                    zone_name,
                    lua_sex(appearance.gender()),
                    appearance.gender().protocol_id(),
                    [
                        appearance.skin(),
                        appearance.face(),
                        appearance.hair_style(),
                        appearance.hair_color(),
                        appearance.facial_hair(),
                    ],
                    equipment,
                    UiCharacterPetPreview::new(pet.display_id(), pet.level(), pet.family_id()),
                    entry.flags(),
                    entry.recustomization_flags(),
                ))
            })
            .collect::<Result<Vec<_>, CharacterProjectionError>>()?;
        let default_background_model = self
            .races
            .race(2)
            .ok_or(CharacterProjectionError::UnknownRace { id: 2 })?
            .client_file_string()
            .to_owned();
        Ok(UiCharacterDirectory::new(
            characters,
            default_background_model,
        ))
    }
}

/// A server character references metadata absent from the mounted client.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum CharacterProjectionError {
    /// The character's race has no client DBC row.
    #[error("character directory references unknown ChrRaces identifier {id}")]
    UnknownRace {
        /// Missing protocol identifier.
        id: u32,
    },
    /// The character's class has no client DBC row.
    #[error("character directory references unknown ChrClasses identifier {id}")]
    UnknownClass {
        /// Missing protocol identifier.
        id: u8,
    },
    /// The character's last zone has no client DBC row.
    #[error("character directory references unknown AreaTable identifier {id}")]
    UnknownArea {
        /// Missing protocol identifier.
        id: u32,
    },
}

/// Converts packet gender to the stock Lua sex constants.
const fn lua_sex(gender: CharacterGender) -> u8 {
    match gender {
        CharacterGender::Male => 2,
        CharacterGender::Female => 3,
        CharacterGender::None => 1,
    }
}
