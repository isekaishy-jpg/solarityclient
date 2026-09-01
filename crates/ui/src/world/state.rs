//! Main-thread projection of authoritative world facts consumed by FrameXML.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Player facts exposed synchronously through the stock FrameXML API.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiPlayerState {
    money_copper: u32,
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

/// Local-player progression values consumed by the stock experience bar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiPlayerProgressionState {
    experience: u32,
    next_level_experience: u32,
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
#[derive(Clone, Debug, Default)]
pub struct UiWorldState {
    inner: Rc<UiWorldStateInner>,
}

/// Individually addressable values avoid copying unrelated UI state per query.
#[derive(Debug, Default)]
struct UiWorldStateInner {
    player: Cell<Option<UiPlayerState>>,
    player_progression: Cell<Option<UiPlayerProgressionState>>,
    player_faction: RefCell<Option<UiPlayerFactionState>>,
    zone: RefCell<Option<UiZoneState>>,
    cursor_money_copper: Cell<u32>,
    player_trade_money_copper: Cell<u32>,
    area_resurrection_available: Cell<bool>,
}

impl UiWorldState {
    /// Creates a world boundary with no active player.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Publishes the latest complete player projection.
    pub fn enter_player(&self, player: UiPlayerState) {
        self.inner.player.set(Some(player));
    }

    /// Publishes the latest local-player experience projection.
    pub fn set_player_progression(&self, progression: UiPlayerProgressionState) {
        self.inner.player_progression.set(Some(progression));
    }

    /// Publishes faction identity composed from the local player's race row.
    pub fn set_player_faction(&self, faction: UiPlayerFactionState) {
        *self.inner.player_faction.borrow_mut() = Some(faction);
    }

    /// Publishes the latest complete map-area projection.
    pub fn set_zone(&self, zone: UiZoneState) {
        *self.inner.zone.borrow_mut() = Some(zone);
    }

    /// Clears player facts when the active world ends.
    pub fn leave_world(&self) {
        self.inner.player.set(None);
        self.inner.player_progression.set(None);
        *self.inner.player_faction.borrow_mut() = None;
        *self.inner.zone.borrow_mut() = None;
        self.inner.cursor_money_copper.set(0);
        self.inner.player_trade_money_copper.set(0);
        self.inner.area_resurrection_available.set(false);
    }

    /// Returns the current player projection when one is authoritative.
    #[must_use]
    pub fn player(&self) -> Option<UiPlayerState> {
        self.inner.player.get()
    }

    /// Returns local-player experience after both stock fields are projected.
    #[must_use]
    pub fn player_progression(&self) -> Option<UiPlayerProgressionState> {
        self.inner.player_progression.get()
    }

    /// Returns the local player's composed faction identity when available.
    #[must_use]
    pub fn player_faction(&self) -> Option<UiPlayerFactionState> {
        self.inner.player_faction.borrow().clone()
    }

    /// Returns the current zone projection when map-area state is authoritative.
    #[must_use]
    pub fn zone(&self) -> Option<UiZoneState> {
        self.inner.zone.borrow().clone()
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
}
