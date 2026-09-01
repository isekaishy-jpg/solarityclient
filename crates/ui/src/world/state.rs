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
    sub_zone_text: String,
    pvp_type: Option<UiZonePvpType>,
    sub_zone_pvp: bool,
    faction_name: Option<String>,
}

impl UiZoneState {
    /// Creates one complete zone projection from the active map-area state.
    #[must_use]
    pub fn new(
        zone_text: impl Into<String>,
        sub_zone_text: impl Into<String>,
        pvp_type: Option<UiZonePvpType>,
        sub_zone_pvp: bool,
        faction_name: Option<String>,
    ) -> Self {
        Self {
            zone_text: zone_text.into(),
            sub_zone_text: sub_zone_text.into(),
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

    /// Returns the current sub-area label.
    #[must_use]
    pub fn sub_zone_text(&self) -> &str {
        &self.sub_zone_text
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
    zone: RefCell<Option<UiZoneState>>,
    cursor_money_copper: Cell<u32>,
    player_trade_money_copper: Cell<u32>,
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

    /// Publishes the latest complete map-area projection.
    pub fn set_zone(&self, zone: UiZoneState) {
        *self.inner.zone.borrow_mut() = Some(zone);
    }

    /// Clears player facts when the active world ends.
    pub fn leave_world(&self) {
        self.inner.player.set(None);
        *self.inner.zone.borrow_mut() = None;
        self.inner.cursor_money_copper.set(0);
        self.inner.player_trade_money_copper.set(0);
    }

    /// Returns the current player projection when one is authoritative.
    #[must_use]
    pub fn player(&self) -> Option<UiPlayerState> {
        self.inner.player.get()
    }

    /// Returns the current zone projection when map-area state is authoritative.
    #[must_use]
    pub fn zone(&self) -> Option<UiZoneState> {
        self.inner.zone.borrow().clone()
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
