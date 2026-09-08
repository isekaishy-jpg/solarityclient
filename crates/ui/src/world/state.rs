//! Main-thread projection of authoritative world facts consumed by FrameXML.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::{UiPlayerLanguage, realm_date::UiRealmDate, realm_time::UiRealmTime};

/// Player facts exposed synchronously through the stock FrameXML API.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiPlayerState {
    money_copper: u32,
}

/// Local-player character identity published by the selected world session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiPlayerIdentityState {
    name: String,
    level: u8,
}

impl UiPlayerIdentityState {
    /// Creates an identity from the server-validated character name.
    #[must_use]
    pub fn new(name: impl Into<String>, level: u8) -> Self {
        Self {
            name: name.into(),
            level,
        }
    }

    /// Returns the local character name without a realm suffix.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the server-published character level.
    #[must_use]
    pub const fn level(&self) -> u8 {
        self.level
    }
}

/// Localized class identity and stable class token.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiPlayerClassState {
    name: String,
    token: String,
    id: u8,
}

impl UiPlayerClassState {
    /// Creates a class projection from the character and class catalogs.
    #[must_use]
    pub fn new(name: impl Into<String>, token: impl Into<String>, id: u8) -> Self {
        Self {
            name: name.into(),
            token: token.into(),
            id,
        }
    }

    /// Returns the localized class name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the stable uppercase class token.
    #[must_use]
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Returns the numeric class identifier.
    #[must_use]
    pub const fn id(&self) -> u8 {
        self.id
    }
}

/// Localized race identity and stable race token.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiPlayerRaceState {
    name: String,
    token: String,
    id: u8,
}

impl UiPlayerRaceState {
    /// Creates a race projection from the character and race catalogs.
    #[must_use]
    pub fn new(name: impl Into<String>, token: impl Into<String>, id: u8) -> Self {
        Self {
            name: name.into(),
            token: token.into(),
            id,
        }
    }

    /// Returns the localized race name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the stable race file token.
    #[must_use]
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Returns the numeric race identifier.
    #[must_use]
    pub const fn id(&self) -> u8 {
        self.id
    }
}

impl UiPlayerState {
    /// Creates the UI projection from authoritative player currency.
    #[must_use]
    pub const fn new(money_copper: u32) -> Self {
        Self { money_copper }
    }

    /// Returns the exact `PLAYER_FIELD_COINAGE` value.
    #[must_use]
    pub const fn money_copper(self) -> u32 {
        self.money_copper
    }
}

/// Stock unit-power classifications used by status bars.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiUnitPowerType {
    /// Spellcasting mana.
    Mana,
    /// Warrior and bear-form rage.
    Rage,
    /// Hunter-pet focus.
    Focus,
    /// Rogue and cat-form energy.
    Energy,
    /// Legacy hunter-pet happiness.
    Happiness,
    /// Death-knight rune slots.
    Runes,
    /// Death-knight runic power.
    RunicPower,
}

impl UiUnitPowerType {
    /// Returns the numeric power identifier stored by build 12340.
    #[must_use]
    pub const fn id(self) -> u8 {
        match self {
            Self::Mana => 0,
            Self::Rage => 1,
            Self::Focus => 2,
            Self::Energy => 3,
            Self::Happiness => 4,
            Self::Runes => 5,
            Self::RunicPower => 6,
        }
    }

    /// Returns the stable uppercase FrameXML token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mana => "MANA",
            Self::Rage => "RAGE",
            Self::Focus => "FOCUS",
            Self::Energy => "ENERGY",
            Self::Happiness => "HAPPINESS",
            Self::Runes => "RUNES",
            Self::RunicPower => "RUNIC_POWER",
        }
    }
}

/// Local-player health, power, and lifecycle flags from the active update.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiPlayerVitalsState {
    health: u32,
    predicted_health: i32,
    max_health: u32,
    power: u32,
    max_power: u32,
    power_type: UiUnitPowerType,
    connected: bool,
    dead: bool,
    ghost: bool,
    threat_situation: Option<u8>,
}

/// Primary attributes returned by build 12340's four-result `UnitStat` API.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UiPlayerStatsState {
    values: [i32; 5],
    positive_modifiers: [i32; 5],
    negative_modifiers: [i32; 5],
}

impl UiPlayerStatsState {
    /// Creates a complete Strength-through-Spirit projection.
    #[must_use]
    pub const fn new(
        values: [i32; 5],
        positive_modifiers: [i32; 5],
        negative_modifiers: [i32; 5],
    ) -> Self {
        Self {
            values,
            positive_modifiers,
            negative_modifiers,
        }
    }

    /// Returns `(base, effective, positive, negative)` for a zero-based stat.
    #[must_use]
    pub const fn stat(self, index: usize) -> Option<(i32, i32, i32, i32)> {
        if index >= self.values.len() {
            return None;
        }
        let value = self.values[index];
        // Build 12340's wrapper reads the current stat twice: once directly
        // and once through the nonnegative accessor at FUN_005774B0.
        Some((
            value,
            if value < 0 { 0 } else { value },
            self.positive_modifiers[index],
            self.negative_modifiers[index],
        ))
    }
}

impl UiPlayerVitalsState {
    /// Creates one complete live unit-bar projection.
    #[must_use]
    pub const fn new(
        health: u32,
        max_health: u32,
        power: u32,
        max_power: u32,
        power_type: UiUnitPowerType,
    ) -> Self {
        Self {
            health,
            predicted_health: health as i32,
            max_health,
            power,
            max_power,
            power_type,
            connected: true,
            dead: (health as i32) <= 0,
            ghost: false,
            threat_situation: None,
        }
    }

    /// Returns current health.
    #[must_use]
    pub const fn health(self) -> u32 {
        self.health
    }

    /// Replaces health presentation while retaining the other unit resources.
    #[must_use]
    pub const fn with_health(
        mut self,
        health: u32,
        maximum: u32,
        predicted: i32,
        ghost: bool,
    ) -> Self {
        self.health = health;
        self.max_health = maximum;
        self.predicted_health = predicted;
        self.dead = (health as i32) <= 0;
        self.ghost = ghost;
        self
    }

    /// Selects the native signed health query through the predictedHealth CVar.
    #[must_use]
    pub const fn displayed_health(self, predicted: bool) -> i32 {
        if predicted {
            self.predicted_health
        } else {
            self.health as i32
        }
    }
    /// Returns maximum health.
    #[must_use]
    pub const fn max_health(self) -> u32 {
        self.max_health
    }
    /// Returns current primary power.
    #[must_use]
    pub const fn power(self) -> u32 {
        self.power
    }
    /// Returns maximum primary power.
    #[must_use]
    pub const fn max_power(self) -> u32 {
        self.max_power
    }
    /// Returns the primary power classification.
    #[must_use]
    pub const fn power_type(self) -> UiUnitPowerType {
        self.power_type
    }
    /// Reports whether the unit has an active connection.
    #[must_use]
    pub const fn connected(self) -> bool {
        self.connected
    }
    /// Reports whether the unit is dead.
    #[must_use]
    pub const fn dead(self) -> bool {
        self.dead
    }
    /// Reports whether the unit is a released ghost.
    #[must_use]
    pub const fn ghost(self) -> bool {
        self.ghost
    }

    /// Returns the 0..=3 threat classification when one is active.
    #[must_use]
    pub const fn threat_situation(self) -> Option<u8> {
        self.threat_situation
    }
}

/// Local-player progression values consumed by the stock experience bar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiPlayerProgressionState {
    experience: u32,
    next_level_experience: u32,
}

/// Realm friend-list totals synchronously consumed by the social UI.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UiFriendCounts {
    total: u32,
    online: u32,
}

impl UiFriendCounts {
    /// Creates one server-projected friend-list count pair.
    ///
    /// Returns `None` when the online count exceeds the total count.
    #[must_use]
    pub const fn new(total: u32, online: u32) -> Option<Self> {
        if online <= total {
            Some(Self { total, online })
        } else {
            None
        }
    }

    /// Returns all realm friends present in the client list.
    #[must_use]
    pub const fn total(self) -> u32 {
        self.total
    }

    /// Returns the connected subset of the realm friend list.
    #[must_use]
    pub const fn online(self) -> u32 {
        self.online
    }
}

impl UiPlayerProgressionState {
    /// Creates a projection of the adjacent build-12340 player XP words.
    #[must_use]
    pub const fn new(experience: u32, next_level_experience: u32) -> Self {
        Self {
            experience,
            next_level_experience,
        }
    }

    /// Returns current experience within the player's level.
    #[must_use]
    pub const fn experience(self) -> u32 {
        self.experience
    }

    /// Returns experience required to complete the player's level.
    #[must_use]
    pub const fn next_level_experience(self) -> u32 {
        self.next_level_experience
    }
}

/// Stable faction token returned by `UnitFactionGroup`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiFactionGroup {
    /// Alliance player races.
    Alliance,
    /// Horde player races.
    Horde,
}

impl UiFactionGroup {
    /// Returns the nonlocalized token used by stock FrameXML branches.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Alliance => "Alliance",
            Self::Horde => "Horde",
        }
    }
}

/// Local-player faction identity after race-catalog composition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiPlayerFactionState {
    group: UiFactionGroup,
    name: String,
}

impl UiPlayerFactionState {
    /// Creates a faction projection with its selected-locale display name.
    #[must_use]
    pub fn new(group: UiFactionGroup, name: impl Into<String>) -> Self {
        Self {
            group,
            name: name.into(),
        }
    }

    /// Returns the stable Alliance/Horde identity.
    #[must_use]
    pub const fn group(&self) -> UiFactionGroup {
        self.group
    }

    /// Returns the selected-locale faction display name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Territory classification returned by build-12340's `GetZonePVPInfo`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiZonePvpType {
    /// Capital or other sanctuary territory.
    Sanctuary,
    /// Free-for-all arena territory.
    Arena,
    /// Territory controlled by the player's faction.
    Friendly,
    /// Territory controlled by an opposing faction.
    Hostile,
    /// Contested territory.
    Contested,
    /// Active combat-zone territory.
    Combat,
}

/// Active map instance classification returned by `IsInInstance`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UiInstanceType {
    /// An outdoor map or no active map.
    #[default]
    None,
    /// A five-player dungeon instance.
    Party,
    /// A raid instance.
    Raid,
    /// A battleground instance.
    Pvp,
    /// An arena instance.
    Arena,
}

impl UiInstanceType {
    /// Returns the exact lowercase FrameXML classification token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Party => "party",
            Self::Raid => "raid",
            Self::Pvp => "pvp",
            Self::Arena => "arena",
        }
    }
}

impl UiZonePvpType {
    /// Returns the exact lowercase token consumed by stock FrameXML.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sanctuary => "sanctuary",
            Self::Arena => "arena",
            Self::Friendly => "friendly",
            Self::Hostile => "hostile",
            Self::Contested => "contested",
            Self::Combat => "combat",
        }
    }
}

/// Current map-area labels and PvP classification projected to FrameXML.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiZoneState {
    zone_text: String,
    real_zone_text: String,
    sub_zone_text: String,
    minimap_zone_text: String,
    pvp_type: Option<UiZonePvpType>,
    sub_zone_pvp: bool,
    faction_name: Option<String>,
}

impl UiZoneState {
    /// Creates one complete zone projection from the active map-area state.
    #[must_use]
    pub fn new(
        zone_text: impl Into<String>,
        real_zone_text: impl Into<String>,
        sub_zone_text: impl Into<String>,
        minimap_zone_text: impl Into<String>,
        pvp_type: Option<UiZonePvpType>,
        sub_zone_pvp: bool,
        faction_name: Option<String>,
    ) -> Self {
        Self {
            zone_text: zone_text.into(),
            real_zone_text: real_zone_text.into(),
            sub_zone_text: sub_zone_text.into(),
            minimap_zone_text: minimap_zone_text.into(),
            pvp_type,
            sub_zone_pvp,
            faction_name,
        }
    }

    /// Returns the current top-level area label.
    #[must_use]
    pub fn zone_text(&self) -> &str {
        &self.zone_text
    }

    /// Returns the unmodified top-level area label used by queries and PvP UI.
    #[must_use]
    pub fn real_zone_text(&self) -> &str {
        &self.real_zone_text
    }

    /// Returns the current sub-area label.
    #[must_use]
    pub fn sub_zone_text(&self) -> &str {
        &self.sub_zone_text
    }

    /// Returns the map-area label selected specifically for the minimap.
    #[must_use]
    pub fn minimap_zone_text(&self) -> &str {
        &self.minimap_zone_text
    }

    /// Returns the current territory classification when map data supplies it.
    #[must_use]
    pub const fn pvp_type(&self) -> Option<UiZonePvpType> {
        self.pvp_type
    }

    /// Reports whether the PvP label describes the sub-zone.
    #[must_use]
    pub const fn is_sub_zone_pvp(&self) -> bool {
        self.sub_zone_pvp
    }

    /// Returns the controlling faction label for friendly or hostile territory.
    #[must_use]
    pub fn faction_name(&self) -> Option<&str> {
        self.faction_name.as_deref()
    }
}

/// Shared main-thread world projection read directly by native Lua closures.
///
/// `Rc<Cell<_>>` matches the actual ownership: the composition root publishes
/// state and the same-thread Lua VM reads it. Queries therefore require no
/// lock, allocation, ECS dependency, or copied value table.
#[derive(Clone, Debug)]
pub struct UiWorldState {
    inner: Rc<UiWorldStateInner>,
}

/// Individually addressable values avoid copying unrelated UI state per query.
#[derive(Debug, Default)]
struct UiWorldStateInner {
    tutorials: crate::UiTutorialState,
    mirror_timers: RefCell<[super::UiMirrorTimer; 3]>,
    release_timer: Cell<super::UiPlayerReleaseTimer>,
    player: Cell<Option<UiPlayerState>>,
    player_guid: Cell<Option<u64>>,
    player_identity: RefCell<Option<UiPlayerIdentityState>>,
    player_class: RefCell<Option<UiPlayerClassState>>,
    player_race: RefCell<Option<UiPlayerRaceState>>,
    player_progression: Cell<Option<UiPlayerProgressionState>>,
    player_vitals: Cell<Option<UiPlayerVitalsState>>,
    player_stats: Cell<Option<UiPlayerStatsState>>,
    player_faction: RefCell<Option<UiPlayerFactionState>>,
    player_default_language: RefCell<Option<UiPlayerLanguage>>,
    zone: RefCell<Option<UiZoneState>>,
    instance_type: Cell<UiInstanceType>,
    dungeon_difficulty: Cell<u8>,
    raid_difficulty: Cell<u8>,
    realm_date: Cell<Option<UiRealmDate>>,
    realm_time: Cell<Option<UiRealmTime>>,
    cursor_money_copper: Cell<u32>,
    player_trade_money_copper: Cell<u32>,
    target_trade_money_copper: Cell<u32>,
    area_resurrection_available: Cell<bool>,
    resting: Cell<bool>,
    swimming: Cell<bool>,
    combat_lockdown: Cell<bool>,
    friend_counts: Cell<UiFriendCounts>,
}

impl Default for UiWorldState {
    fn default() -> Self {
        let state = Self {
            inner: Rc::new(UiWorldStateInner::default()),
        };
        state.inner.dungeon_difficulty.set(1);
        state.inner.raid_difficulty.set(1);
        state
    }
}

impl UiWorldState {
    /// Publishes the native timer initialized when the local player dies.
    pub fn set_release_timer(&self, timer: super::UiPlayerReleaseTimer) {
        self.inner.release_timer.set(timer);
    }

    /// Returns the retained timer queried by the stock release-spirit dialog.
    #[must_use]
    pub fn release_timer(&self) -> super::UiPlayerReleaseTimer {
        self.inner.release_timer.get()
    }

    /// Publishes the frame manager's protected-action lockdown state.
    pub fn set_combat_lockdown(&self, locked: bool) {
        self.inner.combat_lockdown.set(locked);
    }

    /// Reads the native frame manager state queried by InCombatLockdown.
    #[must_use]
    pub fn in_combat_lockdown(&self) -> bool {
        self.inner.combat_lockdown.get()
    }

    /// Returns session tutorial flags, completion history, and pending commands.
    #[must_use]
    pub fn tutorials(&self) -> crate::UiTutorialState {
        self.inner.tutorials.clone()
    }
    /// Replaces the native mirror-timer slot after its event was delivered.
    pub fn set_mirror_timer(&self, index: usize, timer: super::UiMirrorTimer) {
        if let Some(slot) = self.inner.mirror_timers.borrow_mut().get_mut(index) {
            *slot = timer;
        }
    }

    /// Returns a zero-based native timer slot, including its inactive sentinel.
    #[must_use]
    pub fn mirror_timer(&self, index: usize) -> Option<super::UiMirrorTimer> {
        self.inner.mirror_timers.borrow().get(index).cloned()
    }

    /// Samples a native timer without copying or allocating its label.
    #[must_use]
    pub fn mirror_timer_progress(&self, index: usize, timestamp_ms: u32) -> Option<i32> {
        self.inner
            .mirror_timers
            .borrow()
            .get(index)
            .map(|timer| timer.progress(timestamp_ms))
    }
    /// Creates a world boundary with no active player.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Publishes the latest complete player projection.
    pub fn enter_player(&self, player: UiPlayerState) {
        self.inner.player.set(Some(player));
    }

    /// Publishes the authoritative local-player world object identity.
    pub fn set_player_guid(&self, guid: u64) {
        self.inner.player_guid.set((guid != 0).then_some(guid));
    }

    /// Returns local-player identity while the world has an active player.
    #[must_use]
    pub fn player_guid(&self) -> Option<u64> {
        self.player().and(self.inner.player_guid.get())
    }

    /// Publishes the latest local-player experience projection.
    pub fn set_player_progression(&self, progression: UiPlayerProgressionState) {
        self.inner.player_progression.set(Some(progression));
    }

    /// Publishes the latest complete local-player unit-bar projection.
    pub fn set_player_vitals(&self, vitals: UiPlayerVitalsState) {
        self.inner.player_vitals.set(Some(vitals));
    }

    /// Publishes the latest primary-attribute projection.
    pub fn set_player_stats(&self, stats: UiPlayerStatsState) {
        self.inner.player_stats.set(Some(stats));
    }

    /// Publishes the selected character identity for stock unit queries.
    pub fn set_player_identity(&self, identity: UiPlayerIdentityState) {
        *self.inner.player_identity.borrow_mut() = Some(identity);
    }

    /// Publishes the selected character's class identity.
    pub fn set_player_class(&self, class: UiPlayerClassState) {
        *self.inner.player_class.borrow_mut() = Some(class);
    }

    /// Publishes the selected character's race identity.
    pub fn set_player_race(&self, race: UiPlayerRaceState) {
        *self.inner.player_race.borrow_mut() = Some(race);
    }

    /// Publishes faction identity composed from the local player's race row.
    pub fn set_player_faction(&self, faction: UiPlayerFactionState) {
        *self.inner.player_faction.borrow_mut() = Some(faction);
    }

    /// Publishes the active player's localized default chat language.
    pub fn set_player_default_language(&self, language: UiPlayerLanguage) {
        *self.inner.player_default_language.borrow_mut() = Some(language);
    }

    /// Publishes the current realm friend-list totals.
    pub fn set_friend_counts(&self, counts: UiFriendCounts) {
        self.inner.friend_counts.set(counts);
    }

    /// Publishes the latest complete map-area projection.
    pub fn set_zone(&self, zone: UiZoneState) {
        *self.inner.zone.borrow_mut() = Some(zone);
    }

    /// Publishes the active map's instance classification.
    pub fn set_instance_type(&self, instance_type: UiInstanceType) {
        self.inner.instance_type.set(instance_type);
    }

    /// Publishes the selected five-player and raid difficulty identifiers.
    pub fn set_instance_difficulties(&self, dungeon: u8, raid: u8) {
        self.inner.dungeon_difficulty.set(dungeon);
        self.inner.raid_difficulty.set(raid);
    }

    /// Publishes the current server-anchored hour and minute.
    pub fn set_realm_time(&self, time: UiRealmTime) {
        self.inner.realm_time.set(Some(time));
    }

    /// Publishes the current server-anchored Gregorian calendar date.
    pub fn set_realm_date(&self, date: UiRealmDate) {
        self.inner.realm_date.set(Some(date));
    }

    /// Clears player facts when the active world ends.
    pub fn leave_world(&self) {
        *self.inner.mirror_timers.borrow_mut() =
            std::array::from_fn(|_| super::UiMirrorTimer::default());
        self.inner.player.set(None);
        self.inner.player_guid.set(None);
        *self.inner.player_identity.borrow_mut() = None;
        *self.inner.player_class.borrow_mut() = None;
        *self.inner.player_race.borrow_mut() = None;
        self.inner.player_progression.set(None);
        self.inner.player_vitals.set(None);
        self.inner.player_stats.set(None);
        *self.inner.player_faction.borrow_mut() = None;
        *self.inner.player_default_language.borrow_mut() = None;
        *self.inner.zone.borrow_mut() = None;
        self.inner.instance_type.set(UiInstanceType::None);
        self.inner.realm_date.set(None);
        self.inner.realm_time.set(None);
        self.inner.cursor_money_copper.set(0);
        self.inner.player_trade_money_copper.set(0);
        self.inner.target_trade_money_copper.set(0);
        self.inner.area_resurrection_available.set(false);
        self.inner.resting.set(false);
        self.inner.swimming.set(false);
        self.inner.combat_lockdown.set(false);
    }

    /// Returns the current player projection when one is authoritative.
    #[must_use]
    pub fn player(&self) -> Option<UiPlayerState> {
        self.inner.player.get()
    }

    /// Returns local-player identity only while a player owns the world.
    #[must_use]
    pub fn player_identity(&self) -> Option<UiPlayerIdentityState> {
        self.player()
            .and_then(|_| self.inner.player_identity.borrow().clone())
    }

    /// Returns local-player class identity while a player owns the world.
    #[must_use]
    pub fn player_class(&self) -> Option<UiPlayerClassState> {
        self.player()
            .and_then(|_| self.inner.player_class.borrow().clone())
    }

    /// Returns local-player race identity while a player owns the world.
    #[must_use]
    pub fn player_race(&self) -> Option<UiPlayerRaceState> {
        self.player()
            .and_then(|_| self.inner.player_race.borrow().clone())
    }

    /// Returns local-player experience after both stock fields are projected.
    #[must_use]
    pub fn player_progression(&self) -> Option<UiPlayerProgressionState> {
        self.inner.player_progression.get()
    }

    /// Returns local-player vitals after the world has published them.
    #[must_use]
    pub fn player_vitals(&self) -> Option<UiPlayerVitalsState> {
        self.player().and_then(|_| self.inner.player_vitals.get())
    }

    /// Returns local-player attributes after the world has published them.
    #[must_use]
    pub fn player_stats(&self) -> Option<UiPlayerStatsState> {
        self.player().and_then(|_| self.inner.player_stats.get())
    }

    /// Returns the local player's composed faction identity when available.
    #[must_use]
    pub fn player_faction(&self) -> Option<UiPlayerFactionState> {
        self.inner.player_faction.borrow().clone()
    }

    /// Returns the default language only while an active player owns it.
    #[must_use]
    pub fn player_default_language(&self) -> Option<UiPlayerLanguage> {
        self.player()
            .and_then(|_| self.inner.player_default_language.borrow().clone())
    }

    /// Returns realm friend-list totals, including the stock empty initial state.
    #[must_use]
    pub fn friend_counts(&self) -> UiFriendCounts {
        self.inner.friend_counts.get()
    }

    /// Returns the current zone projection when map-area state is authoritative.
    #[must_use]
    pub fn zone(&self) -> Option<UiZoneState> {
        self.inner.zone.borrow().clone()
    }

    /// Returns the current map instance classification.
    #[must_use]
    pub fn instance_type(&self) -> UiInstanceType {
        self.inner.instance_type.get()
    }

    /// Returns the selected five-player difficulty identifier.
    #[must_use]
    pub fn dungeon_difficulty(&self) -> u8 {
        self.inner.dungeon_difficulty.get()
    }

    /// Returns the selected raid difficulty identifier.
    #[must_use]
    pub fn raid_difficulty(&self) -> u8 {
        self.inner.raid_difficulty.get()
    }

    /// Returns realm time only after the world session has supplied it.
    #[must_use]
    pub fn realm_time(&self) -> Option<UiRealmTime> {
        self.inner.realm_time.get()
    }

    /// Returns the realm date only after the world session has supplied it.
    #[must_use]
    pub fn realm_date(&self) -> Option<UiRealmDate> {
        self.inner.realm_date.get()
    }

    /// Publishes whether the current outdoor battlefield permits area exit.
    pub fn set_area_resurrection_available(&self, available: bool) {
        self.inner.area_resurrection_available.set(available);
    }

    /// Reports whether FrameXML may offer hearth-and-resurrect area exit.
    #[must_use]
    pub fn area_resurrection_available(&self) -> bool {
        self.inner.area_resurrection_available.get()
    }

    /// Publishes whether the player is in a rested area.
    pub fn set_resting(&self, resting: bool) {
        self.inner.resting.set(resting);
    }

    /// Reports whether the player is in a rested area.
    #[must_use]
    pub fn is_resting(&self) -> bool {
        self.inner.resting.get()
    }

    /// Publishes Unit_C's immersion bit before deferred movement transitions.
    pub fn set_swimming(&self, swimming: bool) {
        self.inner.swimming.set(swimming);
    }

    /// Returns the local unit's 0xA30 bit 0x200000 (`IsSwimming`, 6124A0).
    #[must_use]
    pub fn is_swimming(&self) -> bool {
        self.player().is_some() && self.inner.swimming.get()
    }

    /// Replaces copper currently attached to the stock money cursor.
    pub fn set_cursor_money_copper(&self, money_copper: u32) {
        self.inner.cursor_money_copper.set(money_copper);
    }

    /// Returns cursor-held copper, zero when the cursor carries no money.
    #[must_use]
    pub fn cursor_money_copper(&self) -> u32 {
        self.inner.cursor_money_copper.get()
    }

    /// Replaces copper offered by the local player in the active trade.
    pub fn set_player_trade_money_copper(&self, money_copper: u32) {
        self.inner.player_trade_money_copper.set(money_copper);
    }

    /// Returns locally offered trade copper, zero outside an active offer.
    #[must_use]
    pub fn player_trade_money_copper(&self) -> u32 {
        self.inner.player_trade_money_copper.get()
    }

    /// Replaces copper offered by the other participant in the active trade.
    pub fn set_target_trade_money_copper(&self, money_copper: u32) {
        self.inner.target_trade_money_copper.set(money_copper);
    }

    /// Returns remotely offered trade copper, zero outside an active offer.
    #[must_use]
    pub fn target_trade_money_copper(&self) -> u32 {
        self.inner.target_trade_money_copper.get()
    }
}
