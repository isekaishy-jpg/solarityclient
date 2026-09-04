//! Stock character-creation metadata, mutable preferences, and wire selections.

use std::cell::RefCell;
use std::rc::Rc;

use solarity_asset::{
    AssetError, AssetPath, AssetStore, CharacterAppearanceCatalog, CharacterBaseCatalog,
    CharacterClassCatalog, CharacterFactionCatalog, CharacterRaceCatalog, CharacterSection,
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
    /// Ordinary and death-knight selection filters, each indexed by sex.
    appearances: [[UiAppearanceChoices; 2]; 2],
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

/// Precomputed ordinary or death-knight selectors for one race and sex.
#[derive(Clone, Debug, Default)]
struct UiAppearanceChoices {
    skins: Vec<u8>,
    skins_by_face: Vec<(u8, Vec<u8>)>,
    faces: Vec<(u8, Vec<u8>)>,
    face_skin_bounds: Vec<(u8, u32)>,
    hair_styles: Vec<u8>,
    hair_colors: Vec<(u8, Vec<u8>)>,
    hair_styles_by_color: Vec<(u8, Vec<u8>)>,
    facial_hair: Vec<(u8, Vec<u8>)>,
    facial_hair_styles: Vec<u8>,
    facial_hair_color_bounds: Vec<(u8, u32)>,
    facial_hair_geometry_count: u32,
}

impl UiAppearanceChoices {
    fn load(
        catalog: &CharacterAppearanceCatalog,
        race_id: u8,
        gender_id: u8,
        class_id: u8,
    ) -> Result<Self, AssetError> {
        let race = u32::from(race_id);
        let gender = u32::from(gender_id);
        let skins = narrow_values(catalog.player_skin_colors_for_class(race, gender, class_id))?;
        let faces = skins
            .iter()
            .map(|skin| {
                narrow_values(catalog.player_faces_for_class(
                    race,
                    gender,
                    u32::from(*skin),
                    class_id,
                ))
                .map(|values| (*skin, values))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut face_values = faces
            .iter()
            .flat_map(|(_skin, faces)| faces.iter().copied())
            .collect::<Vec<_>>();
        face_values.sort_unstable();
        face_values.dedup();
        let skins_by_face = face_values
            .into_iter()
            .map(|face| {
                narrow_values(catalog.player_skin_colors_for_face(
                    race,
                    gender,
                    u32::from(face),
                    class_id,
                ))
                .map(|skins| (face, skins))
            })
            .collect::<Result<Vec<_>, AssetError>>()?;
        let face_skin_bounds = section_color_bounds(catalog.face_sections(race, gender))?;
        let hair_styles =
            narrow_values(catalog.player_hair_styles_for_class(race, gender, class_id))?;
        let hair_colors = hair_styles
            .iter()
            .map(|style| {
                narrow_values(catalog.player_hair_colors_for_class(
                    race,
                    gender,
                    u32::from(*style),
                    class_id,
                ))
                .map(|values| (*style, values))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut colors = hair_colors
            .iter()
            .flat_map(|(_style, colors)| colors.iter().copied())
            .collect::<Vec<_>>();
        let facial_hair_color_bounds =
            section_color_bounds(catalog.facial_hair_sections(race, gender))?;
        for section in catalog.facial_hair_sections(race, gender) {
            let color = narrow_id(section.color_index(), "facial feature color")?;
            colors.push(color);
        }
        // Fresh stock appearance records start with color zero even when its
        // current style has no selectable colors.
        colors.push(0);
        colors.sort_unstable();
        colors.dedup();
        let hair_styles_by_color = colors
            .iter()
            .map(|color| {
                let styles = hair_colors
                    .iter()
                    .filter(|(_style, colors)| colors.contains(color))
                    .map(|(style, _colors)| *style)
                    .collect();
                (*color, styles)
            })
            .collect();
        let facial_hair = colors
            .iter()
            .map(|color| {
                narrow_values(catalog.player_facial_hair_styles_for_class(
                    race,
                    gender,
                    u32::from(*color),
                    class_id,
                ))
                .map(|styles| (*color, styles))
            })
            .collect::<Result<Vec<_>, AssetError>>()?;
        let mut facial_hair_styles = facial_hair
            .iter()
            .flat_map(|(_color, styles)| styles.iter().copied())
            .collect::<Vec<_>>();
        facial_hair_styles.sort_unstable();
        facial_hair_styles.dedup();
        let facial_hair_geometry_count = catalog
            .player_facial_hair_styles(race, gender)
            .into_iter()
            .max()
            .map_or(0, |style| style + 1);
        Ok(Self {
            skins,
            skins_by_face,
            faces,
            face_skin_bounds,
            hair_styles,
            hair_colors,
            hair_styles_by_color,
            facial_hair,
            facial_hair_styles,
            facial_hair_color_bounds,
            facial_hair_geometry_count,
        })
    }

    fn faces_for(&self, skin: u8) -> &[u8] {
        keyed_values(&self.faces, skin)
    }

    fn skins_for(&self, face: u8) -> &[u8] {
        keyed_values(&self.skins_by_face, face)
    }

    fn hair_colors_for(&self, style: u8) -> &[u8] {
        keyed_values(&self.hair_colors, style)
    }

    fn hair_styles_for(&self, color: u8) -> &[u8] {
        keyed_values(&self.hair_styles_by_color, color)
    }

    fn facial_hair_for(&self, color: u8) -> &[u8] {
        keyed_values(&self.facial_hair, color)
    }

    /// Reproduces the range-presence output of ComponentGet at 0x004F3BA0.
    fn has_facial_texture_range(&self, style: u8, color: u8) -> bool {
        self.facial_hair_color_bounds
            .iter()
            .any(|(key, count)| *key == style && u32::from(color) < *count)
    }

    /// 0x004E7DF0 uses geometry count only when the whole texture array is absent.
    fn facial_hair_choice_count(&self, color: u8) -> usize {
        if self.facial_hair_color_bounds.is_empty() {
            self.facial_hair_geometry_count as usize
        } else {
            self.facial_hair_for(color).len()
        }
    }

    /// 0x004E80E0 maps an ordinal according to the current feature's array range.
    fn facial_hair_choice(&self, appearance: UiAppearance, ordinal: usize) -> Option<u8> {
        if !self.has_facial_texture_range(appearance.facial_hair, appearance.hair_color) {
            return (ordinal < self.facial_hair_geometry_count as usize).then_some(ordinal as u8);
        }
        let choices = self.facial_hair_for(appearance.hair_color);
        // This stock mapper bounds its scan by the filtered count, not the
        // full texture-array extent. Do not compact holes before this scan.
        choices
            .iter()
            .filter(|style| usize::from(**style) < choices.len())
            .nth(ordinal)
            .copied()
    }
}

/// Preserves the allocated color extent independently of each row's eligibility.
fn section_color_bounds(sections: &[CharacterSection]) -> Result<Vec<(u8, u32)>, AssetError> {
    let mut bounds = Vec::<(u8, u32)>::new();
    for section in sections {
        let style = narrow_id(section.variation_index(), "appearance variation")?;
        let color = narrow_id(section.color_index(), "appearance color")?;
        if let Some((_, count)) = bounds.iter_mut().find(|(key, _)| *key == style) {
            *count = (*count).max(u32::from(color) + 1);
        } else {
            bounds.push((style, u32::from(color) + 1));
        }
    }
    Ok(bounds)
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
                        [
                            UiAppearanceChoices::load(&appearances, id, 0, 0)?,
                            UiAppearanceChoices::load(&appearances, id, 1, 0)?,
                        ],
                        [
                            UiAppearanceChoices::load(&appearances, id, 0, 6)?,
                            UiAppearanceChoices::load(&appearances, id, 1, 6)?,
                        ],
                    ],
                });
            }
        }
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
    /// Last explicit skin choice, used as the face selector's skin search origin.
    face_skin_origin: u8,
    /// `CharacterCreation.cpp` caches one appearance per race/sex, not class.
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
                face_skin_origin: 0,
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
        let has_eligible_race = inner.catalog.races.iter().any(|race| {
            inner.expansion.admits(race.required_expansion)
                && inner.catalog.classes.iter().any(|class| {
                    inner.expansion.admits(class.required_expansion)
                        && inner.catalog.supports(race.id, class.id)
                })
        });
        let expansion = inner.expansion.0;
        if !has_eligible_race {
            return Err(UiCharacterCreationError::NoPlayableRace { expansion });
        }
        let mut random = self.random.borrow_mut();
        // 0x004DFF10 consumes sex first, then retries full-list race rolls
        // until entitlement admits one. Prefiltering changes the random stream.
        inner.gender_id = scaled_random_index(2, &mut random) as u8;
        loop {
            let race_choice = scaled_random_index(inner.catalog.races.len(), &mut random);
            let race = &inner.catalog.races[race_choice];
            if inner.expansion.admits(race.required_expansion) {
                inner.selected_race = race_choice;
                break;
            }
        }
        // 0x004E1FD0 first creates an ordinary class-zero appearance, then
        // chooses a class in CharBaseInfo order and validates that appearance.
        initialize_appearance(&mut inner, 0, &mut random);
        choose_random_valid_class(&mut inner, &mut random)?;
        validate_appearance(&mut inner)?;
        save_preference(&mut inner);
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
        if inner.selected_class == class_index {
            return Ok(());
        }
        inner.selected_class = class_index;
        validate_appearance(&mut inner)?;
        save_preference(&mut inner);
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
        // 0x004E01F0 treats delta as a direction and ignores zero.
        if delta == 0 {
            return Ok(());
        }
        let delta = delta.signum();
        let choices = appearance_choices(&inner);
        let mut appearance = inner.appearance;
        match index {
            1 => {
                appearance.skin =
                    cycle_value(choices.skins_for(appearance.face), appearance.skin, delta);
            }
            2 => {
                cycle_face(&mut appearance, choices, inner.face_skin_origin, delta);
            }
            3 => {
                appearance.hair_style =
                    cycle_value(&choices.hair_styles, appearance.hair_style, delta);
                let colors = choices.hair_colors_for(appearance.hair_style);
                if !colors.contains(&appearance.hair_color)
                    && let Some(color) = colors.first()
                {
                    // 0x004F0490/0x004F0630 choose the first eligible color
                    // when a neighboring style cannot retain the current one.
                    appearance.hair_color = *color;
                    if let Some(feature) = choices.facial_hair_choice(appearance, 0) {
                        appearance.facial_hair = feature;
                    }
                }
            }
            4 => {
                appearance.hair_color = cycle_value(
                    choices.hair_colors_for(appearance.hair_style),
                    appearance.hair_color,
                    delta,
                );
            }
            5 => {
                cycle_facial_hair(&mut appearance, choices, delta);
            }
            _ => return Err(UiCharacterCreationError::InvalidCustomizationIndex { index }),
        }
        inner.appearance = appearance;
        if index == 1 {
            inner.face_skin_origin = appearance.skin;
        }
        save_preference(&mut inner);
        Ok(())
    }

    /// Randomizes the five appearance axes while retaining race/class/sex.
    pub fn randomize_customization(&self) -> Result<(), UiCharacterCreationError> {
        let mut inner = self.inner.borrow_mut();
        let choices = appearance_choices(&inner);
        let mut appearance = inner.appearance;
        let mut random = self.random.borrow_mut();
        // 0x004E17F0 differs from fresh-model setup: face precedes hair color.
        randomize_value(&mut appearance.skin, &choices.skins, &mut random);
        let skin = appearance.skin;
        randomize_value(&mut appearance.face, choices.faces_for(skin), &mut random);
        randomize_hair(&mut appearance, choices, &mut random);
        randomize_facial_hair(&mut appearance, choices, &mut random);
        inner.appearance = appearance;
        inner.face_skin_origin = appearance.skin;
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

/// Borrows the immutable choices for the selected race, sex, and class rules.
fn appearance_choices(inner: &UiCharacterCreationInner) -> &UiAppearanceChoices {
    let class_id = inner.catalog.classes[inner.selected_class].id;
    appearance_choices_for_class(inner, class_id)
}

/// Class zero uses ordinary creation rules during the stock reset sequence.
fn appearance_choices_for_class(
    inner: &UiCharacterCreationInner,
    class_id: u8,
) -> &UiAppearanceChoices {
    &inner.catalog.races[inner.selected_race].appearances[usize::from(class_id == 6)]
        [usize::from(inner.gender_id)]
}

/// Saves the race/sex record reused independently of subsequent class selection.
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
        validate_appearance(inner)
    } else {
        let class_id = inner.catalog.classes[inner.selected_class].id;
        initialize_appearance(inner, class_id, random);
        save_preference(inner);
        Ok(())
    }
}

/// 0x004E13A0 starts from zero and rolls skin, color, style, face, then features.
fn initialize_appearance(
    inner: &mut UiCharacterCreationInner,
    class_id: u8,
    random: &mut BlizzardRand,
) {
    let choices = appearance_choices_for_class(inner, class_id);
    let mut appearance = UiAppearance::default();
    randomize_value(&mut appearance.skin, &choices.skins, random);
    randomize_hair(&mut appearance, choices, random);
    randomize_value(
        &mut appearance.face,
        choices.faces_for(appearance.skin),
        random,
    );
    randomize_facial_hair(&mut appearance, choices, random);
    inner.appearance = appearance;
    inner.face_skin_origin = appearance.skin;
}

/// Stock chooses color using the current style, then style using the new color.
fn randomize_hair(
    appearance: &mut UiAppearance,
    choices: &UiAppearanceChoices,
    random: &mut BlizzardRand,
) {
    randomize_value(
        &mut appearance.hair_color,
        choices.hair_colors_for(appearance.hair_style),
        random,
    );
    randomize_value(
        &mut appearance.hair_style,
        choices.hair_styles_for(appearance.hair_color),
        random,
    );
}

/// Empty stock selectors leave their previous value intact and consume no roll.
fn randomize_value(value: &mut u8, choices: &[u8], random: &mut BlizzardRand) {
    if let Some(selected) = random_choice(choices, random) {
        *value = *selected;
    }
}

/// Facial randomization has separate count and current-array mapping branches.
fn randomize_facial_hair(
    appearance: &mut UiAppearance,
    choices: &UiAppearanceChoices,
    random: &mut BlizzardRand,
) {
    let count = choices.facial_hair_choice_count(appearance.hair_color);
    let ordinal = if count == 0 {
        0
    } else {
        scaled_random_index(count, random)
    };
    if let Some(feature) = choices.facial_hair_choice(*appearance, ordinal) {
        appearance.facial_hair = feature;
    }
}

/// 0x004EBCA0/0x004EBE80 distinguish geometry cycling from textured features.
fn cycle_facial_hair(appearance: &mut UiAppearance, choices: &UiAppearanceChoices, delta: i32) {
    if !choices.has_facial_texture_range(appearance.facial_hair, appearance.hair_color) {
        let count = choices.facial_hair_geometry_count;
        if count > 0 {
            appearance.facial_hair = (i64::from(appearance.facial_hair) + i64::from(delta))
                .rem_euclid(i64::from(count)) as u8;
        }
        return;
    }
    let feature = cycle_value(&choices.facial_hair_styles, appearance.facial_hair, delta);
    if !choices
        .facial_hair_for(appearance.hair_color)
        .contains(&feature)
        && let Some((color, _styles)) = choices
            .facial_hair
            .iter()
            .find(|(_color, styles)| styles.contains(&feature))
    {
        appearance.hair_color = *color;
    }
    appearance.facial_hair = feature;
}

/// 0x004EB710/0x004EB990 scan faces across skins, retaining the explicit skin origin.
fn cycle_face(
    appearance: &mut UiAppearance,
    choices: &UiAppearanceChoices,
    origin: u8,
    delta: i32,
) {
    let count = choices
        .face_skin_bounds
        .iter()
        .map(|(face, _)| u32::from(*face) + 1)
        .max()
        .unwrap_or(0);
    for offset in 1..count {
        let face = (i64::from(appearance.face) + i64::from(delta) * i64::from(offset))
            .rem_euclid(i64::from(count)) as u8;
        let Some((_, skin_count)) = choices
            .face_skin_bounds
            .iter()
            .find(|(key, _)| *key == face)
        else {
            continue;
        };
        let eligible = choices.skins_for(face);
        let mut skin = 0;
        while skin < *skin_count {
            // The stock loop adds the origin on every iteration, then tests
            // the incremented index against the allocated range's upper bound.
            skin = (skin + u32::from(origin)) % skin_count;
            if eligible.contains(&(skin as u8)) {
                if !eligible.contains(&appearance.skin) {
                    appearance.skin = skin as u8;
                }
                appearance.face = face;
                return;
            }
            skin += 1;
        }
    }
}

/// Replays 0x004E9D50's deterministic repair after a class or cached-model change.
fn validate_appearance(
    inner: &mut UiCharacterCreationInner,
) -> Result<(), UiCharacterCreationError> {
    let choices = appearance_choices(inner);
    let mut appearance = inner.appearance;
    appearance.skin = validated_value(&choices.skins, appearance.skin);
    appearance.face = validated_value(choices.faces_for(appearance.skin), appearance.face);
    let styles = choices.hair_styles_for(appearance.hair_color);
    if !styles.contains(&appearance.hair_style) {
        if !styles.is_empty() {
            appearance.hair_style = validated_value(styles, appearance.hair_style);
        } else if choices.hair_styles.is_empty() {
            appearance.hair_style = 0;
            appearance.hair_color = 0;
        } else {
            // Stock scans actual style indices after taking the remainder by
            // the number of styles having colors. Preserve that scan rather
            // than treating an unavailable style index as a compact ordinal.
            let candidate = usize::from(appearance.hair_style) % choices.hair_styles.len();
            if let Some(style) = choices
                .hair_styles
                .iter()
                .find(|style| usize::from(**style) == candidate)
            {
                appearance.hair_style = *style;
            }
            let colors = choices.hair_colors_for(appearance.hair_style);
            if colors.is_empty() {
                return Err(missing_axis(inner, "hair color"));
            }
            appearance.hair_color = validated_value(colors, appearance.hair_color);
        }
    }
    let feature_valid = u32::from(appearance.facial_hair) <= choices.facial_hair_geometry_count
        && (!choices.has_facial_texture_range(appearance.facial_hair, appearance.hair_color)
            || choices
                .facial_hair_for(appearance.hair_color)
                .contains(&appearance.facial_hair));
    if !feature_valid {
        let count = choices.facial_hair_choice_count(appearance.hair_color);
        appearance.facial_hair = if count == 0 {
            0
        } else {
            let feature =
                choices.facial_hair_choice(appearance, usize::from(appearance.facial_hair) % count);
            let Some(feature) = feature else {
                return Err(missing_axis(inner, "facial hair"));
            };
            feature
        };
    }
    inner.appearance = appearance;
    inner.face_skin_origin = appearance.skin;
    Ok(())
}

/// Invalid stock values become the same ordinal modulo the available count.
fn validated_value(choices: &[u8], value: u8) -> u8 {
    if choices.contains(&value) {
        value
    } else if choices.is_empty() {
        0
    } else {
        choices[usize::from(value) % choices.len()]
    }
}

fn choose_random_valid_class(
    inner: &mut UiCharacterCreationInner,
    random: &mut BlizzardRand,
) -> Result<(), UiCharacterCreationError> {
    let race_id = inner.catalog.races[inner.selected_race].id;
    let classes = inner
        .catalog
        .combinations
        .iter()
        .filter(|(race, _class)| *race == race_id)
        .filter_map(|(_race, class_id)| {
            inner
                .catalog
                .classes
                .iter()
                .position(|class| class.id == *class_id)
        })
        .filter(|index| {
            inner
                .expansion
                .admits(inner.catalog.classes[*index].required_expansion)
        })
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

/// Uses the unsigned multiply-high range reduction at 0x004EB416.
fn random_choice<'values, T>(
    values: &'values [T],
    random: &mut BlizzardRand,
) -> Option<&'values T> {
    if values.is_empty() {
        return None;
    }
    let index = scaled_random_index(values.len(), random);
    values.get(index)
}

/// Counts are bounded by the byte-sized creation catalog and never zero here.
fn scaled_random_index(count: usize, random: &mut BlizzardRand) -> usize {
    ((u64::from(random.next_u32()) * count as u64) >> 32) as usize
}

/// Scans sorted authored values in the requested direction and wraps once.
/// An empty selector cannot change the current value in the stock setters.
fn cycle_value(values: &[u8], current: u8, delta: i32) -> u8 {
    let next = if delta > 0 {
        values
            .iter()
            .find(|value| **value > current)
            .or_else(|| values.first())
    } else {
        values
            .iter()
            .rev()
            .find(|value| **value < current)
            .or_else(|| values.last())
    };
    next.copied().unwrap_or(current)
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
