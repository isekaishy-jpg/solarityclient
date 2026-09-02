//! Stock character-creation metadata, mutable preferences, and wire selections.

use std::cell::RefCell;
use std::rc::Rc;

use solarity_asset::{
    AssetError, AssetPath, AssetStore, CharacterAppearanceCatalog, CharacterBaseCatalog,
    CharacterClassCatalog, CharacterFactionCatalog, CharacterRaceCatalog,
};
use solarity_cpu::BlizzardRand;
use thiserror::Error;

const NPC_ONLY_RACE_FLAG: u32 = 0x0000_0001;
const ALLIANCE: &str = "Alliance";
const HORDE: &str = "Horde";

/// One stock account expansion ordinal used by creation availability checks.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct UiCharacterExpansion(u32);

impl UiCharacterExpansion {
    /// Original World of Warcraft entitlement.
    pub const ORIGINAL: Self = Self(0);
    /// The Burning Crusade entitlement.
    pub const THE_BURNING_CRUSADE: Self = Self(1);
    /// Wrath of the Lich King entitlement.
    pub const WRATH_OF_THE_LICH_KING: Self = Self(2);

    /// Creates an exact DBC expansion ordinal.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    const fn admits(self, required: u32) -> bool {
        self.0 >= required
    }
}

/// One complete `CMSG_CHAR_CREATE` payload emitted by built-in Glue.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiCharacterCreationRequest {
    name: String,
    race_id: u8,
    class_id: u8,
    gender_id: u8,
    skin_color: u8,
    face: u8,
    hair_style: u8,
    hair_color: u8,
    facial_hair: u8,
}

/// Current character-model selections consumed by the pre-world renderer.
#[derive(Clone, Debug, PartialEq)]
pub struct UiCharacterCreationPreview {
    race_id: u8,
    class_id: u8,
    gender_id: u8,
    appearance: [u8; 5],
    facing_degrees: f64,
}

impl UiCharacterCreationPreview {
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

    /// Returns the Glue-controlled character facing in degrees.
    #[must_use]
    pub const fn facing_degrees(&self) -> f64 {
        self.facing_degrees
    }
}

impl UiCharacterCreationRequest {
    /// Returns the authored character name without client-side substitution.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
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

    /// Returns the wire gender identifier: zero male or one female.
    #[must_use]
    pub const fn gender_id(&self) -> u8 {
        self.gender_id
    }

    /// Returns the five stock appearance bytes in packet order.
    #[must_use]
    pub const fn appearance(&self) -> [u8; 5] {
        [
            self.skin_color,
            self.face,
            self.hair_style,
            self.hair_color,
            self.facial_hair,
        ]
    }
}

/// Invalid creation metadata or an impossible script selection.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum UiCharacterCreationError {
    /// No playable race is admitted by the authenticated entitlement.
    #[error("character creation has no playable race for expansion {expansion}")]
    NoPlayableRace {
        /// Authenticated account expansion ordinal.
        expansion: u32,
    },
    /// A one-based race selector is outside the stock list.
    #[error("character creation race index {index} is invalid")]
    InvalidRaceIndex {
        /// One-based Glue race-list index.
        index: u32,
    },
    /// A one-based class selector is outside the physical class list.
    #[error("character creation class index {index} is invalid")]
    InvalidClassIndex {
        /// One-based physical class-list index.
        index: u32,
    },
    /// The selected race/class pair is absent or expansion-locked.
    #[error("character creation race/class pair {race_id}/{class_id} is unavailable")]
    InvalidRaceClass {
        /// Protocol race identifier.
        race_id: u8,
        /// Protocol class identifier.
        class_id: u8,
    },
    /// Lua supplied another value than stock's male/two or female/three token.
    #[error("character creation sex token {sex} is invalid")]
    InvalidSex {
        /// Glue sex token supplied by Lua.
        sex: u32,
    },
    /// A playable race/gender has no authored value for a required appearance axis.
    #[error("character creation race {race_id} gender {gender_id} has no {axis} choices")]
    MissingAppearanceChoices {
        /// Protocol race identifier.
        race_id: u8,
        /// Zero-based protocol gender identifier.
        gender_id: u8,
        /// Human-readable customization axis.
        axis: &'static str,
    },
    /// A customization button index lies outside the five stock axes.
    #[error("character customization index {index} is invalid")]
    InvalidCustomizationIndex {
        /// One-based customization-axis index.
        index: u32,
    },
}

#[derive(Clone, Debug)]
struct UiCreationRace {
    id: u8,
    name: String,
    female_name: String,
    male_name: String,
    file_string: String,
    faction_name: String,
    faction_internal_name: String,
    facial_hair_tokens: [String; 2],
    hair_token: String,
    required_expansion: u32,
    appearances: [UiAppearanceChoices; 2],
}

impl UiCreationRace {
    fn display_name(&self, gender_id: u8) -> &str {
        let authored = match gender_id {
            0 => &self.male_name,
            1 => &self.female_name,
            _ => "",
        };
        if authored.is_empty() {
            &self.name
        } else {
            authored
        }
    }
}

#[derive(Clone, Debug)]
struct UiCreationClass {
    id: u8,
    name: String,
    female_name: String,
    male_name: String,
    file_string: String,
    required_expansion: u32,
    roles: UiCreationClassRoles,
}

impl UiCreationClass {
    fn display_name(&self, gender_id: u8) -> &str {
        let authored = match gender_id {
            0 => &self.male_name,
            1 => &self.female_name,
            _ => "",
        };
        if authored.is_empty() {
            &self.name
        } else {
            authored
        }
    }
}

/// Role flags returned after stock `GetSelectedClass` identity values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiCreationClassRoles {
    tank: bool,
    healer: bool,
    damage: bool,
}

impl UiCreationClassRoles {
    /// Reports whether the class can tank in build 12340's LFG role table.
    #[must_use]
    pub const fn tank(self) -> bool {
        self.tank
    }

    /// Reports whether the class can heal in build 12340's LFG role table.
    #[must_use]
    pub const fn healer(self) -> bool {
        self.healer
    }

    /// Reports whether the class can deal damage in build 12340's LFG table.
    #[must_use]
    pub const fn damage(self) -> bool {
        self.damage
    }
}

#[derive(Clone, Debug, Default)]
struct UiAppearanceChoices {
    skins: Vec<u8>,
    faces: Vec<(u8, Vec<u8>)>,
    hair_styles: Vec<u8>,
    hair_colors: Vec<(u8, Vec<u8>)>,
    facial_hair: Vec<u8>,
}

impl UiAppearanceChoices {
    fn load(
        catalog: &CharacterAppearanceCatalog,
        race_id: u8,
        gender_id: u8,
    ) -> Result<Self, AssetError> {
        let race = u32::from(race_id);
        let gender = u32::from(gender_id);
        let skins = narrow_values(catalog.player_skin_colors(race, gender))?;
        let faces = skins
            .iter()
            .map(|skin| {
                narrow_values(catalog.player_faces(race, gender, u32::from(*skin)))
                    .map(|values| (*skin, values))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let hair_styles = narrow_values(catalog.player_hair_styles(race, gender))?;
        let hair_colors = hair_styles
            .iter()
            .map(|style| {
                narrow_values(catalog.player_hair_colors(race, gender, u32::from(*style)))
                    .map(|values| (*style, values))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let facial_hair = narrow_values(catalog.player_facial_hair_styles(race, gender))?;
        Ok(Self {
            skins,
            faces,
            hair_styles,
            hair_colors,
            facial_hair,
        })
    }

    fn faces_for(&self, skin: u8) -> &[u8] {
        keyed_values(&self.faces, skin)
    }

    fn hair_colors_for(&self, style: u8) -> &[u8] {
        keyed_values(&self.hair_colors, style)
    }
}

fn keyed_values(values: &[(u8, Vec<u8>)], key: u8) -> &[u8] {
    values
        .iter()
        .find(|(candidate, _values)| *candidate == key)
        .map_or(&[], |(_key, values)| values.as_slice())
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct UiAppearance {
    skin: u8,
    face: u8,
    hair_style: u8,
    hair_color: u8,
    facial_hair: u8,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct UiPreference {
    appearance: UiAppearance,
    initialized: bool,
}

struct UiCharacterCreationCatalog {
    races: Vec<UiCreationRace>,
    classes: Vec<UiCreationClass>,
    combinations: Vec<(u8, u8)>,
    streaming_trial: bool,
}

impl UiCharacterCreationCatalog {
    fn load(store: &mut AssetStore, streaming_trial: bool) -> Result<Self, AssetError> {
        let races = CharacterRaceCatalog::load(store)?;
        let classes = CharacterClassCatalog::load(store)?;
        let combinations = CharacterBaseCatalog::load(store)?;
        let factions = CharacterFactionCatalog::load(store)?;
        let appearances = CharacterAppearanceCatalog::load(store)?;

        let mut creation_races = Vec::new();
        for side in [ALLIANCE, HORDE] {
            for race in races.physical_races() {
                if race.flags() & NPC_ONLY_RACE_FLAG != 0 {
                    continue;
                }
                let Some(group) = factions.group_for_template(race.faction_id()) else {
                    continue;
                };
                if group.internal_name() != side {
                    continue;
                }
                let id = narrow_id(race.id(), "ChrRaces.dbc race")?;
                creation_races.push(UiCreationRace {
                    id,
                    name: race.name().to_owned(),
                    female_name: race.female_name().to_owned(),
                    male_name: race.male_name().to_owned(),
                    file_string: race.client_file_string().to_owned(),
                    faction_name: group.name().to_owned(),
                    faction_internal_name: group.internal_name().to_owned(),
                    facial_hair_tokens: [
                        race.facial_hair_customization(0)
                            .unwrap_or_default()
                            .to_owned(),
                        race.facial_hair_customization(1)
                            .unwrap_or_default()
                            .to_owned(),
                    ],
                    hair_token: race.hair_customization().to_owned(),
                    required_expansion: race.required_expansion(),
                    appearances: [
                        UiAppearanceChoices::load(&appearances, id, 0)?,
                        UiAppearanceChoices::load(&appearances, id, 1)?,
                    ],
                });
            }
        }
        let creation_classes = classes
            .physical_classes()
            .map(|class| {
                let id = narrow_id(class.id(), "ChrClasses.dbc class")?;
                Ok(UiCreationClass {
                    id,
                    name: class.name().to_owned(),
                    female_name: class.female_name().to_owned(),
                    male_name: class.male_name().to_owned(),
                    file_string: class.file_string().to_owned(),
                    required_expansion: class.required_expansion(),
                    roles: class_roles(id),
                })
            })
            .collect::<Result<Vec<_>, AssetError>>()?;
        Ok(Self {
            races: creation_races,
            classes: creation_classes,
            combinations: combinations
                .entries()
                .iter()
                .map(|entry| (entry.race_id(), entry.class_id()))
                .collect(),
            streaming_trial,
        })
    }

    fn supports(&self, race_id: u8, class_id: u8) -> bool {
        self.combinations.contains(&(race_id, class_id))
    }
}

struct UiCharacterCreationInner {
    catalog: UiCharacterCreationCatalog,
    expansion: UiCharacterExpansion,
    selected_race: usize,
    selected_class: usize,
    gender_id: u8,
    appearance: UiAppearance,
    preferences: Vec<[UiPreference; 2]>,
    facing_degrees: f64,
}

/// Shared creation state queried synchronously by stock Glue Lua.
#[derive(Clone)]
pub struct UiCharacterCreationState {
    inner: Rc<RefCell<UiCharacterCreationInner>>,
    random: Rc<RefCell<BlizzardRand>>,
}

impl UiCharacterCreationState {
    /// Loads all stock DBC inputs and attaches the process-wide random stream.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] for missing or malformed creation metadata.
    pub fn load(
        store: &mut AssetStore,
        streaming_trial: bool,
        random: Rc<RefCell<BlizzardRand>>,
    ) -> Result<Self, AssetError> {
        let catalog = UiCharacterCreationCatalog::load(store, streaming_trial)?;
        let preferences = vec![[UiPreference::default(); 2]; catalog.races.len()];
        Ok(Self {
            inner: Rc::new(RefCell::new(UiCharacterCreationInner {
                catalog,
                expansion: UiCharacterExpansion::ORIGINAL,
                selected_race: 0,
                selected_class: 0,
                gender_id: 0,
                appearance: UiAppearance::default(),
                preferences,
                facing_degrees: 0.0,
            })),
            random,
        })
    }

    /// Applies the authenticated account's expansion before creation is shown.
    pub fn set_expansion(&self, expansion: UiCharacterExpansion) {
        self.inner.borrow_mut().expansion = expansion;
    }

    /// Reproduces stock's random race, class, and appearance reset.
    ///
    /// # Errors
    ///
    /// Returns [`UiCharacterCreationError`] if client metadata has no complete
    /// playable combination for the authenticated expansion.
    pub fn reset(&self) -> Result<(), UiCharacterCreationError> {
        let mut inner = self.inner.borrow_mut();
        // ResetCharCustomize clears the per-race/sex model cache but leaves
        // the current sex untouched. CharacterCreate enters with the authored
        // male default and immediately reapplies that same selection.
        inner.preferences.fill([UiPreference::default(); 2]);
        let eligible_races = inner
            .catalog
            .races
            .iter()
            .enumerate()
            .filter(|(_index, race)| {
                inner.expansion.admits(race.required_expansion)
                    && inner.catalog.classes.iter().any(|class| {
                        inner.expansion.admits(class.required_expansion)
                            && inner.catalog.supports(race.id, class.id)
                    })
            })
            .map(|(index, _race)| index)
            .collect::<Vec<_>>();
        let expansion = inner.expansion.0;
        let race_choice = random_choice(&eligible_races, &mut self.random.borrow_mut())
            .copied()
            .ok_or(UiCharacterCreationError::NoPlayableRace { expansion })?;
        inner.selected_race = race_choice;
        choose_random_valid_class(&mut inner, &mut self.random.borrow_mut())?;
        randomize_appearance(&mut inner, &mut self.random.borrow_mut())?;
        let gender_index = usize::from(inner.gender_id);
        inner.preferences[race_choice][gender_index] = UiPreference {
            appearance: inner.appearance,
            initialized: true,
        };
        Ok(())
    }

    /// Returns race triples in stock Glue list order.
    #[must_use]
    pub fn available_races(&self) -> Vec<(String, String, bool)> {
        let inner = self.inner.borrow();
        inner
            .catalog
            .races
            .iter()
            .map(|race| {
                (
                    race.display_name(inner.gender_id).to_owned(),
                    race.file_string.clone(),
                    inner.expansion.admits(race.required_expansion),
                )
            })
            .collect()
    }

    /// Returns class triples in physical `ChrClasses.dbc` order.
    #[must_use]
    pub fn available_classes(&self) -> Vec<(String, String, bool)> {
        let inner = self.inner.borrow();
        inner
            .catalog
            .classes
            .iter()
            .map(|class| {
                (
                    class.display_name(inner.gender_id).to_owned(),
                    class.file_string.clone(),
                    inner.expansion.admits(class.required_expansion),
                )
            })
            .collect()
    }

    /// Returns whether one-based race and class indices form an authored pair.
    #[must_use]
    pub fn is_race_class_valid(&self, race_index: u32, class_index: u32) -> bool {
        let inner = self.inner.borrow();
        let Some(race) = one_based(&inner.catalog.races, race_index) else {
            return false;
        };
        let Some(class) = one_based(&inner.catalog.classes, class_index) else {
            return false;
        };
        inner.catalog.supports(race.id, class.id)
    }

    /// Returns the selected one-based race index.
    #[must_use]
    pub fn selected_race(&self) -> u32 {
        (self.inner.borrow().selected_race + 1) as u32
    }

    /// Returns the selected one-based class index.
    #[must_use]
    pub fn selected_class(&self) -> u32 {
        (self.inner.borrow().selected_class + 1) as u32
    }

    /// Returns the stock Lua sex token: two male or three female.
    #[must_use]
    pub fn selected_sex(&self) -> u32 {
        u32::from(self.inner.borrow().gender_id) + 2
    }

    /// Selects one race and restores that race/sex preference when available.
    pub fn set_selected_race(&self, index: u32) -> Result<(), UiCharacterCreationError> {
        let mut inner = self.inner.borrow_mut();
        let new_index = one_based_index(inner.catalog.races.len(), index)
            .ok_or(UiCharacterCreationError::InvalidRaceIndex { index })?;
        if new_index == inner.selected_race {
            return Ok(());
        }
        save_preference(&mut inner);
        inner.selected_race = new_index;
        ensure_valid_class(&mut inner, &mut self.random.borrow_mut())?;
        restore_or_randomize_appearance(&mut inner, &mut self.random.borrow_mut())
    }

    /// Selects one physical-order class for the current race.
    pub fn set_selected_class(&self, index: u32) -> Result<(), UiCharacterCreationError> {
        let mut inner = self.inner.borrow_mut();
        let class_index = one_based_index(inner.catalog.classes.len(), index)
            .ok_or(UiCharacterCreationError::InvalidClassIndex { index })?;
        let race_id = inner.catalog.races[inner.selected_race].id;
        let class = &inner.catalog.classes[class_index];
        if !inner.expansion.admits(class.required_expansion)
            || !inner.catalog.supports(race_id, class.id)
        {
            return Err(UiCharacterCreationError::InvalidRaceClass {
                race_id,
                class_id: class.id,
            });
        }
        inner.selected_class = class_index;
        Ok(())
    }

    /// Selects stock's male/two or female/three token and restores preferences.
    pub fn set_selected_sex(&self, sex: u32) -> Result<(), UiCharacterCreationError> {
        let gender_id = match sex {
            2 => 0,
            3 => 1,
            _ => return Err(UiCharacterCreationError::InvalidSex { sex }),
        };
        let mut inner = self.inner.borrow_mut();
        if gender_id == inner.gender_id {
            return Ok(());
        }
        save_preference(&mut inner);
        inner.gender_id = gender_id;
        restore_or_randomize_appearance(&mut inner, &mut self.random.borrow_mut())
    }

    /// Returns selected class identity and its exact LFG role flags.
    #[must_use]
    pub fn selected_class_info(&self) -> (String, String, u32, UiCreationClassRoles) {
        let inner = self.inner.borrow();
        let class = &inner.catalog.classes[inner.selected_class];
        (
            class.display_name(inner.gender_id).to_owned(),
            class.file_string.clone(),
            (inner.selected_class + 1) as u32,
            class.roles,
        )
    }

    /// Returns selected race display identity.
    #[must_use]
    pub fn selected_race_info(&self) -> (String, String) {
        let inner = self.inner.borrow();
        let race = &inner.catalog.races[inner.selected_race];
        (
            race.display_name(inner.gender_id).to_owned(),
            race.file_string.clone(),
        )
    }

    /// Returns localized faction identity for one one-based race index.
    #[must_use]
    pub fn faction_for_race(&self, index: u32) -> Option<(String, String)> {
        let inner = self.inner.borrow();
        let race = one_based(&inner.catalog.races, index)?;
        Some((
            race.faction_name.clone(),
            race.faction_internal_name.clone(),
        ))
    }

    /// Returns stock's background filename token for the current selection.
    #[must_use]
    pub fn background_model(&self) -> String {
        let inner = self.inner.borrow();
        if inner.catalog.streaming_trial {
            return "CharacterSelect".to_owned();
        }
        let class = &inner.catalog.classes[inner.selected_class];
        if class.id == 6 {
            return class.file_string.clone();
        }
        let race = &inner.catalog.races[inner.selected_race];
        let background_race_id = match race.id {
            7 => 3,
            8 => 2,
            id => id,
        };
        inner
            .catalog
            .races
            .iter()
            .find(|candidate| candidate.id == background_race_id)
            .map_or_else(String::new, |race| race.file_string.clone())
    }

    /// Returns the current race's hair label token.
    #[must_use]
    pub fn hair_customization(&self) -> String {
        let inner = self.inner.borrow();
        inner.catalog.races[inner.selected_race].hair_token.clone()
    }

    /// Returns the current race/sex facial-feature label token.
    #[must_use]
    pub fn facial_hair_customization(&self) -> String {
        let inner = self.inner.borrow();
        inner.catalog.races[inner.selected_race].facial_hair_tokens[usize::from(inner.gender_id)]
            .clone()
    }

    /// Cycles one of the five stock appearance axes with wraparound.
    pub fn cycle_customization(
        &self,
        index: u32,
        delta: i32,
    ) -> Result<(), UiCharacterCreationError> {
        let mut inner = self.inner.borrow_mut();
        let choices = appearance_choices(&inner);
        match index {
            1 => {
                inner.appearance.skin = cycle_value(
                    &choices.skins,
                    inner.appearance.skin,
                    delta,
                    &inner,
                    "skin color",
                )?;
                inner.appearance.face = first_or_current(
                    choices.faces_for(inner.appearance.skin),
                    inner.appearance.face,
                    &inner,
                    "face",
                )?;
            }
            2 => {
                inner.appearance.face = cycle_value(
                    choices.faces_for(inner.appearance.skin),
                    inner.appearance.face,
                    delta,
                    &inner,
                    "face",
                )?;
            }
            3 => {
                inner.appearance.hair_style = cycle_value(
                    &choices.hair_styles,
                    inner.appearance.hair_style,
                    delta,
                    &inner,
                    "hair style",
                )?;
                inner.appearance.hair_color = first_or_current(
                    choices.hair_colors_for(inner.appearance.hair_style),
                    inner.appearance.hair_color,
                    &inner,
                    "hair color",
                )?;
            }
            4 => {
                inner.appearance.hair_color = cycle_value(
                    choices.hair_colors_for(inner.appearance.hair_style),
                    inner.appearance.hair_color,
                    delta,
                    &inner,
                    "hair color",
                )?;
            }
            5 => {
                inner.appearance.facial_hair = cycle_value(
                    &choices.facial_hair,
                    inner.appearance.facial_hair,
                    delta,
                    &inner,
                    "facial hair",
                )?;
            }
            _ => return Err(UiCharacterCreationError::InvalidCustomizationIndex { index }),
        }
        save_preference(&mut inner);
        Ok(())
    }

    /// Randomizes the five appearance axes while retaining race/class/sex.
    pub fn randomize_customization(&self) -> Result<(), UiCharacterCreationError> {
        let mut inner = self.inner.borrow_mut();
        randomize_appearance(&mut inner, &mut self.random.borrow_mut())?;
        save_preference(&mut inner);
        Ok(())
    }

    /// Returns current character-model facing in degrees.
    #[must_use]
    pub fn facing_degrees(&self) -> f64 {
        self.inner.borrow().facing_degrees
    }

    /// Stores character-model facing in degrees exactly as supplied by Glue.
    pub fn set_facing_degrees(&self, facing: f64) {
        self.inner.borrow_mut().facing_degrees = facing;
    }

    /// Captures the current protocol fields with one user-authored name.
    #[must_use]
    pub fn create_request(&self, name: String) -> UiCharacterCreationRequest {
        let inner = self.inner.borrow();
        let race = &inner.catalog.races[inner.selected_race];
        let class = &inner.catalog.classes[inner.selected_class];
        UiCharacterCreationRequest {
            name,
            race_id: race.id,
            class_id: class.id,
            gender_id: inner.gender_id,
            skin_color: inner.appearance.skin,
            face: inner.appearance.face,
            hair_style: inner.appearance.hair_style,
            hair_color: inner.appearance.hair_color,
            facial_hair: inner.appearance.facial_hair,
        }
    }

    /// Captures renderer-facing creation selections without a submitted name.
    #[must_use]
    pub fn preview(&self) -> UiCharacterCreationPreview {
        let inner = self.inner.borrow();
        let race = &inner.catalog.races[inner.selected_race];
        let class = &inner.catalog.classes[inner.selected_class];
        UiCharacterCreationPreview {
            race_id: race.id,
            class_id: class.id,
            gender_id: inner.gender_id,
            appearance: [
                inner.appearance.skin,
                inner.appearance.face,
                inner.appearance.hair_style,
                inner.appearance.hair_color,
                inner.appearance.facial_hair,
            ],
            facing_degrees: inner.facing_degrees,
        }
    }
}

fn appearance_choices(inner: &UiCharacterCreationInner) -> UiAppearanceChoices {
    inner.catalog.races[inner.selected_race].appearances[usize::from(inner.gender_id)].clone()
}

fn save_preference(inner: &mut UiCharacterCreationInner) {
    inner.preferences[inner.selected_race][usize::from(inner.gender_id)] = UiPreference {
        appearance: inner.appearance,
        initialized: true,
    };
}

fn restore_or_randomize_appearance(
    inner: &mut UiCharacterCreationInner,
    random: &mut BlizzardRand,
) -> Result<(), UiCharacterCreationError> {
    let preference = inner.preferences[inner.selected_race][usize::from(inner.gender_id)];
    if preference.initialized {
        inner.appearance = preference.appearance;
        Ok(())
    } else {
        randomize_appearance(inner, random)?;
        save_preference(inner);
        Ok(())
    }
}

fn randomize_appearance(
    inner: &mut UiCharacterCreationInner,
    random: &mut BlizzardRand,
) -> Result<(), UiCharacterCreationError> {
    let choices = appearance_choices(inner);
    inner.appearance.skin = required_random(&choices.skins, inner, "skin color", random)?;
    inner.appearance.face = required_random(
        choices.faces_for(inner.appearance.skin),
        inner,
        "face",
        random,
    )?;
    inner.appearance.hair_style =
        required_random(&choices.hair_styles, inner, "hair style", random)?;
    inner.appearance.hair_color = required_random(
        choices.hair_colors_for(inner.appearance.hair_style),
        inner,
        "hair color",
        random,
    )?;
    inner.appearance.facial_hair =
        required_random(&choices.facial_hair, inner, "facial hair", random)?;
    Ok(())
}

fn choose_random_valid_class(
    inner: &mut UiCharacterCreationInner,
    random: &mut BlizzardRand,
) -> Result<(), UiCharacterCreationError> {
    let race_id = inner.catalog.races[inner.selected_race].id;
    let classes = inner
        .catalog
        .classes
        .iter()
        .enumerate()
        .filter(|(_index, class)| {
            inner.expansion.admits(class.required_expansion)
                && inner.catalog.supports(race_id, class.id)
        })
        .map(|(index, _class)| index)
        .collect::<Vec<_>>();
    let Some(selected) = random_choice(&classes, random).copied() else {
        return Err(UiCharacterCreationError::InvalidRaceClass {
            race_id,
            class_id: 0,
        });
    };
    inner.selected_class = selected;
    Ok(())
}

fn ensure_valid_class(
    inner: &mut UiCharacterCreationInner,
    random: &mut BlizzardRand,
) -> Result<(), UiCharacterCreationError> {
    let race_id = inner.catalog.races[inner.selected_race].id;
    let class = &inner.catalog.classes[inner.selected_class];
    if inner.expansion.admits(class.required_expansion) && inner.catalog.supports(race_id, class.id)
    {
        return Ok(());
    }
    choose_random_valid_class(inner, random)
}

fn required_random(
    values: &[u8],
    inner: &UiCharacterCreationInner,
    axis: &'static str,
    random: &mut BlizzardRand,
) -> Result<u8, UiCharacterCreationError> {
    random_choice(values, random)
        .copied()
        .ok_or_else(|| missing_axis(inner, axis))
}

fn random_choice<'values, T>(
    values: &'values [T],
    random: &mut BlizzardRand,
) -> Option<&'values T> {
    if values.is_empty() {
        return None;
    }
    let index = usize::try_from(random.next_u32()).ok()? % values.len();
    values.get(index)
}

fn cycle_value(
    values: &[u8],
    current: u8,
    delta: i32,
    inner: &UiCharacterCreationInner,
    axis: &'static str,
) -> Result<u8, UiCharacterCreationError> {
    if values.is_empty() {
        return Err(missing_axis(inner, axis));
    }
    let current_index = values
        .iter()
        .position(|value| *value == current)
        .unwrap_or(0);
    let length = i64::try_from(values.len()).map_err(|_source| missing_axis(inner, axis))?;
    let next = (i64::try_from(current_index).map_err(|_source| missing_axis(inner, axis))?
        + i64::from(delta))
    .rem_euclid(length);
    values
        .get(usize::try_from(next).map_err(|_source| missing_axis(inner, axis))?)
        .copied()
        .ok_or_else(|| missing_axis(inner, axis))
}

fn first_or_current(
    values: &[u8],
    current: u8,
    inner: &UiCharacterCreationInner,
    axis: &'static str,
) -> Result<u8, UiCharacterCreationError> {
    if values.contains(&current) {
        return Ok(current);
    }
    values
        .first()
        .copied()
        .ok_or_else(|| missing_axis(inner, axis))
}

fn missing_axis(inner: &UiCharacterCreationInner, axis: &'static str) -> UiCharacterCreationError {
    UiCharacterCreationError::MissingAppearanceChoices {
        race_id: inner.catalog.races[inner.selected_race].id,
        gender_id: inner.gender_id,
        axis,
    }
}

fn one_based<T>(values: &[T], index: u32) -> Option<&T> {
    values.get(one_based_index(values.len(), index)?)
}

fn one_based_index(length: usize, index: u32) -> Option<usize> {
    let index = usize::try_from(index.checked_sub(1)?).ok()?;
    (index < length).then_some(index)
}

fn narrow_values(values: Vec<u32>) -> Result<Vec<u8>, AssetError> {
    values
        .into_iter()
        .map(|value| narrow_id(value, "CharSections.dbc customization"))
        .collect()
}

fn narrow_id(value: u32, label: &str) -> Result<u8, AssetError> {
    u8::try_from(value).map_err(|_source| AssetError::DatabaseDecode {
        path: AssetPath::new("DBFilesClient\\CharSections.dbc")
            .unwrap_or_else(|_source| unreachable!()),
        message: format!("{label} identifier {value} exceeds the packet byte domain"),
    })
}

const fn class_roles(class_id: u8) -> UiCreationClassRoles {
    match class_id {
        1 | 6 => UiCreationClassRoles {
            tank: true,
            healer: false,
            damage: true,
        },
        2 | 11 => UiCreationClassRoles {
            tank: true,
            healer: true,
            damage: true,
        },
        5 | 7 => UiCreationClassRoles {
            tank: false,
            healer: true,
            damage: true,
        },
        _ => UiCreationClassRoles {
            tank: false,
            healer: false,
            damage: true,
        },
    }
}
