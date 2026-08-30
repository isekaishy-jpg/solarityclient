//! Owned realm-directory state recovered from `RealmList.cpp`.

use wow_login_messages::all::{Population, Version};
use wow_login_messages::version_2::{RealmCategory as ProtocolCategory, RealmType as ProtocolType};
use wow_login_messages::version_8::Realm;

/// The stock rule set advertised for one realm.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RealmType {
    /// Player-versus-environment rules.
    PlayerVsEnvironment,
    /// Player-versus-player rules.
    PlayerVsPlayer,
    /// Roleplaying rules.
    Roleplaying,
    /// Roleplaying player-versus-player rules.
    RoleplayingPlayerVsPlayer,
}

impl RealmType {
    /// Returns the numeric rule-set identifier used by `Cfg_Configs.dbc`.
    #[must_use]
    pub const fn id(self) -> u32 {
        match self {
            Self::PlayerVsEnvironment => 0,
            Self::PlayerVsPlayer => 1,
            Self::Roleplaying => 6,
            Self::RoleplayingPlayerVsPlayer => 8,
        }
    }
}

/// The region/category byte carried by the legacy realm list.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RealmCategory {
    /// Default category.
    Default,
    /// Category value one.
    One,
    /// Category value two.
    Two,
    /// Category value three.
    Three,
    /// Category value five.
    Five,
}

impl RealmCategory {
    /// Returns the category identifier used by `Cfg_Categories.dbc`.
    #[must_use]
    pub const fn id(self) -> u32 {
        match self {
            Self::Default => 0,
            Self::One => 1,
            Self::Two => 2,
            Self::Three => 3,
            Self::Five => 5,
        }
    }
}

/// Stock color recommendation encoded by population and realm flags.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RealmRecommendation {
    /// No forced recommendation color.
    None,
    /// Blue recommended state.
    Blue,
    /// Green recommended state.
    Green,
    /// Red/full state.
    RedFull,
}

/// One owned realm row independent of the wire-message dependency.
#[derive(Clone, Debug, PartialEq)]
pub struct RealmEntry {
    id: u8,
    name: String,
    address: String,
    realm_type: RealmType,
    category: RealmCategory,
    population: f32,
    character_count: u8,
    locked: bool,
    invalid: bool,
    offline: bool,
    recommendation: RealmRecommendation,
    required_build: Option<(u8, u8, u8, u16)>,
}

impl RealmEntry {
    /// Returns the realm identifier later included in character selection.
    #[must_use]
    pub const fn id(&self) -> u8 {
        self.id
    }

    /// Returns the display name supplied by realmd.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the world-server address exactly as advertised.
    #[must_use]
    pub fn address(&self) -> &str {
        &self.address
    }

    /// Returns the realm rule set.
    #[must_use]
    pub const fn realm_type(&self) -> RealmType {
        self.realm_type
    }

    /// Returns the legacy category byte as a typed value.
    #[must_use]
    pub const fn category(&self) -> RealmCategory {
        self.category
    }

    /// Returns the raw server population value.
    #[must_use]
    pub const fn population(&self) -> f32 {
        self.population
    }

    /// Returns the number of account characters reported on this realm.
    #[must_use]
    pub const fn character_count(&self) -> u8 {
        self.character_count
    }

    /// Returns whether the realm is locked for this account.
    #[must_use]
    pub const fn is_locked(&self) -> bool {
        self.locked
    }

    /// Returns whether the realm row is marked invalid.
    #[must_use]
    pub const fn is_invalid(&self) -> bool {
        self.invalid
    }

    /// Returns whether the realm is offline.
    #[must_use]
    pub const fn is_offline(&self) -> bool {
        self.offline
    }

    /// Returns the forced or population-derived recommendation color.
    #[must_use]
    pub const fn recommendation(&self) -> RealmRecommendation {
        self.recommendation
    }

    /// Returns an explicitly advertised required client build.
    #[must_use]
    pub const fn required_build(&self) -> Option<(u8, u8, u8, u16)> {
        self.required_build
    }

    pub(super) fn from_protocol(realm: Realm) -> Self {
        let recommendation = if realm.flag.get_force_red_full() {
            RealmRecommendation::RedFull
        } else if realm.flag.get_force_green_recommended() {
            RealmRecommendation::Green
        } else if realm.flag.get_force_blue_recommended() {
            RealmRecommendation::Blue
        } else {
            population_recommendation(realm.population)
        };
        let required_build = realm
            .flag
            .get_specify_build()
            .map(|build| version_tuple(build.version));
        Self {
            id: realm.realm_id,
            name: realm.name,
            address: realm.address,
            realm_type: protocol_realm_type(realm.realm_type),
            category: protocol_category(realm.category),
            population: population_value(realm.population),
            character_count: realm.number_of_characters_on_realm,
            locked: realm.locked,
            invalid: realm.flag.get_invalid(),
            offline: realm.flag.get_offline(),
            recommendation,
            required_build,
        }
    }
}

/// One immutable realm-list response in server order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RealmDirectory {
    entries: Vec<RealmEntry>,
}

impl RealmDirectory {
    pub(crate) fn from_protocol(realms: Vec<Realm>) -> Self {
        Self {
            entries: realms.into_iter().map(RealmEntry::from_protocol).collect(),
        }
    }

    /// Returns realm rows in exact server order.
    #[must_use]
    pub fn entries(&self) -> &[RealmEntry] {
        &self.entries
    }

    /// Finds a realm by its protocol identifier.
    #[must_use]
    pub fn by_id(&self, id: u8) -> Option<&RealmEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    /// Finds the first realm with an exact display name.
    #[must_use]
    pub fn by_name(&self, name: &str) -> Option<&RealmEntry> {
        self.entries.iter().find(|entry| entry.name == name)
    }
}

const fn protocol_realm_type(value: ProtocolType) -> RealmType {
    match value {
        ProtocolType::PlayerVsEnvironment => RealmType::PlayerVsEnvironment,
        ProtocolType::PlayerVsPlayer => RealmType::PlayerVsPlayer,
        ProtocolType::Roleplaying => RealmType::Roleplaying,
        ProtocolType::RoleplayingPlayerVsPlayer => RealmType::RoleplayingPlayerVsPlayer,
    }
}

const fn protocol_category(value: ProtocolCategory) -> RealmCategory {
    match value {
        ProtocolCategory::Default => RealmCategory::Default,
        ProtocolCategory::One => RealmCategory::One,
        ProtocolCategory::Two => RealmCategory::Two,
        ProtocolCategory::Three => RealmCategory::Three,
        ProtocolCategory::Five => RealmCategory::Five,
    }
}

const fn population_value(value: Population) -> f32 {
    match value {
        Population::GreenRecommended => 200.0,
        Population::RedFull => 400.0,
        Population::BlueRecommended => 600.0,
        Population::Other(value) => value,
    }
}

const fn population_recommendation(value: Population) -> RealmRecommendation {
    match value {
        Population::GreenRecommended => RealmRecommendation::Green,
        Population::RedFull => RealmRecommendation::RedFull,
        Population::BlueRecommended => RealmRecommendation::Blue,
        Population::Other(_) => RealmRecommendation::None,
    }
}

const fn version_tuple(version: Version) -> (u8, u8, u8, u16) {
    (version.major, version.minor, version.patch, version.build)
}
