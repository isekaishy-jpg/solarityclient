//! Party-member frame presentation behavior evidenced by `PartyFrame.cpp`.

mod party_frame;

use std::cell::Cell;
use std::rc::Rc;

use mlua::{Lua, Table, Value};

/// Party loot distribution method consumed by the stock unit menu.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UiLootMethod {
    /// Loot rolls use the standard group-loot rules.
    #[default]
    Group,
    /// Every member may loot freely.
    FreeForAll,
    /// Loot rights rotate through the roster.
    RoundRobin,
    /// One roster member distributes loot.
    Master,
    /// Upgrade eligibility gates need rolls.
    NeedBeforeGreed,
}

impl UiLootMethod {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Group => "group",
            Self::FreeForAll => "freeforall",
            Self::RoundRobin => "roundrobin",
            Self::Master => "master",
            Self::NeedBeforeGreed => "needbeforegreed",
        }
    }
}

/// Server-published party and raid roster sizes.
#[derive(Clone, Debug, Default)]
pub struct UiGroupRosterState {
    party_members: Rc<Cell<u8>>,
    raid_members: Rc<Cell<u8>>,
    arena_opponents: Rc<Cell<u8>>,
    party_leader: Rc<Cell<bool>>,
    raid_leader: Rc<Cell<bool>>,
    raid_officer: Rc<Cell<bool>>,
    loot_method: Rc<Cell<UiLootMethod>>,
    loot_threshold: Rc<Cell<u8>>,
    opt_out_of_loot: Rc<Cell<bool>>,
    raid_info_requested: Rc<Cell<bool>>,
}

impl UiGroupRosterState {
    /// Creates the stock solo roster image.
    #[must_use]
    pub fn new() -> Self {
        let state = Self::default();
        state.loot_threshold.set(2);
        state
    }

    /// Replaces the number of party members excluding the local player.
    pub fn set_party_members(&self, members: u8) {
        self.party_members.set(members);
    }

    /// Replaces the number of raid members including the local player.
    pub fn set_raid_members(&self, members: u8) {
        self.raid_members.set(members);
    }

    /// Replaces the number of arena opponents currently published to unit IDs.
    pub fn set_arena_opponents(&self, opponents: u8) {
        self.arena_opponents.set(opponents);
    }

    /// Replaces local-player party leadership.
    pub fn set_party_leader(&self, leader: bool) {
        self.party_leader.set(leader);
    }

    /// Replaces local-player raid leadership and assistant status.
    pub fn set_raid_authority(&self, leader: bool, officer: bool) {
        self.raid_leader.set(leader);
        self.raid_officer.set(officer);
    }

    /// Replaces the authoritative group loot configuration.
    pub fn set_loot(&self, method: UiLootMethod, threshold: u8) {
        self.loot_method.set(method);
        self.loot_threshold.set(threshold);
    }

    /// Returns party members excluding the local player.
    #[must_use]
    pub fn party_members(&self) -> u8 {
        self.party_members.get()
    }

    /// Returns raid members including the local player.
    #[must_use]
    pub fn raid_members(&self) -> u8 {
        self.raid_members.get()
    }

    /// Returns the number of arena opponent unit IDs currently available.
    #[must_use]
    pub fn arena_opponents(&self) -> u8 {
        self.arena_opponents.get()
    }

    /// Reports local-player party leadership.
    #[must_use]
    pub fn is_party_leader(&self) -> bool {
        self.party_leader.get()
    }

    /// Reports local-player raid leadership.
    #[must_use]
    pub fn is_raid_leader(&self) -> bool {
        self.raid_leader.get()
    }

    /// Reports local-player raid assistant status.
    #[must_use]
    pub fn is_raid_officer(&self) -> bool {
        self.raid_officer.get()
    }

    /// Returns the active party loot method.
    #[must_use]
    pub fn loot_method(&self) -> UiLootMethod {
        self.loot_method.get()
    }

    /// Returns the minimum item quality assigned by group loot rules.
    #[must_use]
    pub fn loot_threshold(&self) -> u8 {
        self.loot_threshold.get()
    }

    /// Reports whether the local player passes on eligible group loot.
    #[must_use]
    pub fn opt_out_of_loot(&self) -> bool {
        self.opt_out_of_loot.get()
    }

    /// Takes the pending stock raid-lockout refresh request.
    pub fn take_raid_info_request(&self) -> bool {
        self.raid_info_requested.replace(false)
    }
}

pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    state: UiGroupRosterState,
) -> mlua::Result<()> {
    let party = state.clone();
    globals.raw_set(
        "GetNumPartyMembers",
        lua.create_function(move |_, ()| Ok(party.party_members()))?,
    )?;
    let party_members = state.clone();
    globals.raw_set(
        "GetPartyMember",
        lua.create_function(move |_, index: u8| {
            Ok((index > 0 && index <= party_members.party_members()).then_some(1_u8))
        })?,
    )?;
    globals.raw_set(
        "GetPartyLeaderIndex",
        lua.create_function(|_, ()| Ok(0_u8))?,
    )?;
    globals.raw_set(
        "GetNumRaidMembers",
        lua.create_function({
            let state = state.clone();
            move |_, ()| Ok(state.raid_members())
        })?,
    )?;
    let arena_opponents = state.clone();
    globals.raw_set(
        "GetNumArenaOpponents",
        lua.create_function(move |_, ()| Ok(arena_opponents.arena_opponents()))?,
    )?;
    let party_leader = state.clone();
    globals.raw_set(
        "IsPartyLeader",
        lua.create_function(move |_, ()| Ok(party_leader.is_party_leader().then_some(1_u8)))?,
    )?;
    let raid_leader = state.clone();
    globals.raw_set(
        "IsRaidLeader",
        lua.create_function(move |_, ()| Ok(raid_leader.is_raid_leader().then_some(1_u8)))?,
    )?;
    let raid_officer = state.clone();
    globals.raw_set(
        "IsRaidOfficer",
        lua.create_function(move |_, ()| Ok(raid_officer.is_raid_officer().then_some(1_u8)))?,
    )?;
    let party = state.clone();
    globals.raw_set(
        "UnitInParty",
        lua.create_function(move |_, unit: Value| {
            let is_player = matches!(unit, Value::String(ref value) if value.to_string_lossy().eq_ignore_ascii_case("player"));
            Ok((is_player && party.party_members() > 0).then_some(1_u8))
        })?,
    )?;
    let raid = state.clone();
    globals.raw_set(
        "UnitInRaid",
        lua.create_function(move |_, unit: Value| {
            let is_player = matches!(unit, Value::String(ref value) if value.to_string_lossy().eq_ignore_ascii_case("player"));
            Ok((is_player && raid.raid_members() > 0).then_some(1_u8))
        })?,
    )?;
    let loot = state.clone();
    globals.raw_set(
        "GetLootMethod",
        lua.create_function(move |_, ()| {
            Ok((
                loot.loot_method().as_str(),
                Option::<u8>::None,
                Option::<u8>::None,
            ))
        })?,
    )?;
    let threshold = state.clone();
    globals.raw_set(
        "GetLootThreshold",
        lua.create_function(move |_, ()| Ok(threshold.loot_threshold()))?,
    )?;
    let opt_out = state.clone();
    globals.raw_set(
        "GetOptOutOfLoot",
        lua.create_function(move |_, ()| Ok(opt_out.opt_out_of_loot().then_some(1_u8)))?,
    )?;
    let set_opt_out = state.clone();
    globals.raw_set(
        "SetOptOutOfLoot",
        lua.create_function(move |_, value: Option<bool>| {
            set_opt_out.opt_out_of_loot.set(value.unwrap_or(false));
            Ok(())
        })?,
    )?;
    globals.raw_set(
        "IsOnePersonParty",
        lua.create_function(|_, ()| Ok(Option::<u8>::None))?,
    )?;
    globals.raw_set(
        "IsInFakeRaid",
        lua.create_function(|_, ()| Ok(Option::<u8>::None))?,
    )?;
    globals.raw_set(
        "GetReadyCheckStatus",
        lua.create_function(|_, _unit: Value| Ok(Value::Nil))?,
    )?;
    globals.raw_set(
        "GetReadyCheckTimeLeft",
        lua.create_function(|_, ()| Ok(0.0_f64))?,
    )?;
    let raid_info = state;
    globals.raw_set(
        "RequestRaidInfo",
        lua.create_function(move |_, ()| {
            // Script_RequestRaidInfo is the build-12340 world-session request
            // boundary. Coalescing retains one pending command until the
            // composition root takes it for wire publication.
            raid_info.raid_info_requested.set(true);
            Ok(())
        })?,
    )
}
