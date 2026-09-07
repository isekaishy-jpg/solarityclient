//! Projection of protocol character rows through client-authored DBC metadata.

use solarity_asset::{
    AreaTableCatalog, AssetError, AssetStore, CharacterClassCatalog, CharacterFactionCatalog,
    CharacterRaceCatalog,
};
use solarity_ecs::{ActiveWorld, UnitIdentity};
use solarity_network::{CharacterDirectory, CharacterGender};
use solarity_ui::{
    UiCharacterDirectory, UiCharacterEquipment, UiCharacterInfo, UiCharacterPetPreview,
    UiFactionGroup, UiPlayerClassState, UiPlayerFactionState, UiPlayerIdentityState,
    UiPlayerLanguage, UiPlayerProgressionState, UiPlayerRaceState, UiPlayerState,
    UiPlayerStatsState, UiPlayerVitalsState, UiUnitPowerType, UiWorldState, UiZoneState,
};
use thiserror::Error;

/// Client-authored labels required by character-selection Glue.
pub(crate) struct RuntimeCharacterMetadata {
    races: CharacterRaceCatalog,
    classes: CharacterClassCatalog,
    factions: CharacterFactionCatalog,
    areas: AreaTableCatalog,
    world_model_areas: solarity_asset::WorldModelAreaCatalog,
}

impl RuntimeCharacterMetadata {
    /// Borrows the same area catalog used by zone text for per-field audio inheritance.
    pub(crate) fn world_location(
        &self,
        terrain_area_id: Option<u32>,
        world_model: Option<super::terrain_coordinator::UnitWorldModelLocation>,
    ) -> Result<RuntimeWorldLocation, CharacterProjectionError> {
        let group = world_model.and_then(|location| self.world_model_areas.area(location.key));
        let root = world_model.and_then(|location| {
            self.world_model_areas
                .area(solarity_asset::WorldModelAreaKey {
                    group_id: -1,
                    ..location.key
                })
        });
        let world_model_only = world_model.is_some_and(|location| location.world_model_only);
        // 782560 takes only the group's nonzero AreaTable relation on static roots.
        let area_id = group
            .filter(|_| world_model.is_some_and(|location| location.area_override))
            .map(|row| row.area_id())
            .filter(|id| *id != 0)
            .or(terrain_area_id);
        let area = area_id
            .map(|id| {
                self.areas
                    .area(id)
                    .ok_or(CharacterProjectionError::UnknownArea { id })
            })
            .transpose()?;
        let parent = if let Some(area) = area.filter(|area| area.parent_area_id() != 0) {
            Some(self.areas.area(area.parent_area_id()).ok_or(
                CharacterProjectionError::UnknownArea {
                    id: area.parent_area_id(),
                },
            )?)
        } else {
            None
        };
        Ok(RuntimeWorldLocation {
            chunk_key: None,
            area_id,
            sound_location_ids: solarity_media::ZoneSoundLocationIds {
                areas: [
                    parent.or(area).map_or(0, |row| row.id()),
                    area.filter(|_| parent.is_some()).map_or(0, |row| row.id()),
                ],
                world_model_areas: [
                    root.map_or(0, |row| row.id()),
                    group.map_or(0, |row| row.id()),
                ],
                world_model_only,
            },
            sounds: solarity_media::resolve_zone_sound_references(
                parent.map(|area| area.sounds()),
                area.map(|area| area.sounds()),
                root.map(|row| row.sounds()),
                group.map(|row| row.sounds()),
                world_model_only,
            ),
            world_model_only,
        })
    }

    /// Loads selection metadata before the asset stack transfers into Glue.
    pub(crate) fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        Ok(Self {
            races: CharacterRaceCatalog::load(store)?,
            classes: CharacterClassCatalog::load(store)?,
            factions: CharacterFactionCatalog::load(store)?,
            areas: AreaTableCatalog::load(store)?,
            world_model_areas: solarity_asset::WorldModelAreaCatalog::load(store)?,
        })
    }

    /// Reports whether all synchronous player facts queried by FrameXML OnLoad
    /// handlers have arrived in the initial object-update stream.
    pub(crate) fn active_player_is_ready(active: &ActiveWorld) -> bool {
        active.local_player_unit_identity().is_some()
            && active.local_player_identity().is_some()
            && active.local_player_money().is_some()
            && active.local_player_progression().is_some()
            && active.local_player_vitals().is_some()
            && active.local_player_stats().is_some()
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

    /// Publishes the authoritative local-player image required before stock
    /// FrameXML executes its synchronous `OnLoad` queries.
    pub(crate) fn publish_active_player(
        &self,
        active: &ActiveWorld,
        target: &UiWorldState,
    ) -> Result<(), CharacterProjectionError> {
        let identity = active
            .local_player_unit_identity()
            .ok_or(CharacterProjectionError::MissingActiveIdentity)?;
        let player = active
            .local_player_identity()
            .ok_or(CharacterProjectionError::MissingActiveName)?;
        let money = active
            .local_player_money()
            .ok_or(CharacterProjectionError::MissingActiveMoney)?;
        let progression = active
            .local_player_progression()
            .ok_or(CharacterProjectionError::MissingActiveProgression)?;
        let vitals = active
            .local_player_vitals()
            .ok_or(CharacterProjectionError::MissingActiveVitals)?;
        let stats = active
            .local_player_stats()
            .ok_or(CharacterProjectionError::MissingActiveStats)?;
        let race = self.races.race(u32::from(identity.race_id())).ok_or(
            CharacterProjectionError::UnknownRace {
                id: u32::from(identity.race_id()),
            },
        )?;
        let class = self.classes.class(u32::from(identity.class_id())).ok_or(
            CharacterProjectionError::UnknownClass {
                id: identity.class_id(),
            },
        )?;
        let faction = self
            .factions
            .group_for_template(identity.faction_template_id())
            .ok_or(CharacterProjectionError::UnknownFactionTemplate {
                id: identity.faction_template_id(),
            })?;
        let faction_group = match faction.internal_name() {
            "Alliance" => UiFactionGroup::Alliance,
            "Horde" => UiFactionGroup::Horde,
            name => {
                return Err(CharacterProjectionError::UnsupportedPlayerFaction {
                    name: name.to_owned(),
                });
            }
        };
        let level = u8::try_from(identity.level()).map_err(|_source| {
            CharacterProjectionError::InvalidPlayerLevel {
                level: identity.level(),
            }
        })?;
        let power_type = ui_power_type(identity)?;
        let power_index = usize::from(identity.power_type_id());
        let powers = vitals.powers();
        let max_powers = vitals.max_powers();
        let guid = active
            .local_player_guid()
            .map_err(|_| CharacterProjectionError::MissingActiveIdentity)?;

        target.enter_player(UiPlayerState::new(money.copper()));
        target.set_player_guid(guid);
        target.set_player_identity(UiPlayerIdentityState::new(player.name(), level));
        target.set_player_class(UiPlayerClassState::new(
            class.display_name(identity.gender_id()),
            class.file_string().to_ascii_uppercase(),
            identity.class_id(),
        ));
        target.set_player_race(UiPlayerRaceState::new(
            race.display_name(identity.gender_id()),
            race.client_file_string(),
            identity.race_id(),
        ));
        target.set_player_progression(UiPlayerProgressionState::new(
            progression.experience(),
            progression.next_level_experience(),
        ));
        target.set_player_vitals(UiPlayerVitalsState::new(
            vitals.health(),
            vitals.max_health(),
            powers[power_index],
            max_powers[power_index],
            power_type,
        ));
        target.set_player_stats(UiPlayerStatsState::new(
            stats.values(),
            stats.positive_modifiers(),
            stats.negative_modifiers(),
        ));
        target.set_player_faction(UiPlayerFactionState::new(faction_group, faction.name()));
        // FUN_00500910 selects the active player's default language from the
        // learned-language table. Every stock player starts with its faction's
        // common language; later spell initialization can extend that table.
        target.set_player_default_language(match faction_group {
            UiFactionGroup::Alliance => UiPlayerLanguage::new(7, "Common"),
            UiFactionGroup::Horde => UiPlayerLanguage::new(1, "Orcish"),
        });
        Ok(())
    }

    /// Projects an optional terrain-authored area identifier into the labels
    /// exposed by stock world and minimap Lua queries.
    ///
    /// AreaTable field 2 is the exact parent-zone relationship. The recovered
    /// zone provider exposes that parent as the zone, the child as sub-zone,
    /// and the most specific nonempty label to the minimap.
    pub(crate) fn zone_state(
        &self,
        area_id: Option<u32>,
    ) -> Result<UiZoneState, CharacterProjectionError> {
        let Some(area_id) = area_id else {
            return Ok(UiZoneState::new("", "", "", "", None, false, None));
        };
        let area = self
            .areas
            .area(area_id)
            .ok_or(CharacterProjectionError::UnknownArea { id: area_id })?;
        if area.parent_area_id() == 0 {
            return Ok(UiZoneState::new(
                area.name(),
                area.name(),
                "",
                area.name(),
                None,
                false,
                None,
            ));
        }
        let parent = self.areas.area(area.parent_area_id()).ok_or(
            CharacterProjectionError::UnknownArea {
                id: area.parent_area_id(),
            },
        )?;
        Ok(UiZoneState::new(
            parent.name(),
            parent.name(),
            area.name(),
            area.name(),
            None,
            false,
            None,
        ))
    }
}

/// Common resolved location supplied to zone text and the sound owner.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RuntimeWorldLocation {
    pub sound_location_ids: solarity_media::ZoneSoundLocationIds,
    pub chunk_key: Option<solarity_asset::WorldChunkSoundKey>,
    pub area_id: Option<u32>,
    pub sounds: solarity_asset::AreaSoundReferences,
    pub world_model_only: bool,
}

/// Converts `UNIT_FIELD_BYTES_0`'s closed build-12340 power vocabulary.
fn ui_power_type(identity: UnitIdentity) -> Result<UiUnitPowerType, CharacterProjectionError> {
    match identity.power_type_id() {
        0 => Ok(UiUnitPowerType::Mana),
        1 => Ok(UiUnitPowerType::Rage),
        2 => Ok(UiUnitPowerType::Focus),
        3 => Ok(UiUnitPowerType::Energy),
        4 => Ok(UiUnitPowerType::Happiness),
        5 => Ok(UiUnitPowerType::Runes),
        6 => Ok(UiUnitPowerType::RunicPower),
        id => Err(CharacterProjectionError::UnknownPowerType { id }),
    }
}

/// A server character references metadata absent from the mounted client.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
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
    /// FrameXML started before the local create update published unit identity.
    #[error("active player has no projected unit identity")]
    MissingActiveIdentity,
    /// World bootstrap lost the selected character name.
    #[error("active player has no server-validated name")]
    MissingActiveName,
    /// FrameXML started before private coinage arrived.
    #[error("active player has no projected coinage")]
    MissingActiveMoney,
    /// FrameXML started before the adjacent XP words arrived.
    #[error("active player has no projected progression")]
    MissingActiveProgression,
    /// FrameXML started before health and power fields arrived.
    #[error("active player has no projected vitals")]
    MissingActiveVitals,
    /// FrameXML started before primary player attributes arrived.
    #[error("active player has no projected primary attributes")]
    MissingActiveStats,
    /// The local unit references a faction template absent from client DBCs.
    #[error("active player references unknown FactionTemplate identifier {id}")]
    UnknownFactionTemplate {
        /// Missing DBC identifier.
        id: u32,
    },
    /// A player faction resolved outside the two stock playable groups.
    #[error("active player belongs to unsupported faction group {name}")]
    UnsupportedPlayerFaction {
        /// Unexpected stable group token.
        name: String,
    },
    /// The server level does not fit build 12340's byte-sized UI result.
    #[error("active player level {level} exceeds the FrameXML representation")]
    InvalidPlayerLevel {
        /// Server-provided level.
        level: u32,
    },
    /// The local unit uses an unknown build-12340 power identifier.
    #[error("active player references unknown power type {id}")]
    UnknownPowerType {
        /// Byte from `UNIT_FIELD_BYTES_0`.
        id: u8,
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
