//! Retained group-finder proposal, queue, and role-check state.

use std::cell::RefCell;
use std::rc::Rc;

use thiserror::Error;

/// Role assigned to the local player by an LFG proposal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiGroupFinderRole {
    /// Group leader without a combat-role assignment.
    Leader,
    /// Tank role.
    Tank,
    /// Healer role.
    Healer,
    /// Damage role.
    Damage,
    /// No role has been selected.
    None,
}

impl UiGroupFinderRole {
    /// Returns the exact stock uppercase role token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Leader => "LEADER",
            Self::Tank => "TANK",
            Self::Healer => "HEALER",
            Self::Damage => "DAMAGER",
            Self::None => "NONE",
        }
    }
}

/// Complete 12-value dungeon proposal projected to FrameXML.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiGroupFinderProposal {
    type_id: u32,
    dungeon_id: u32,
    name: String,
    texture: String,
    role: UiGroupFinderRole,
    has_responded: bool,
    total_encounters: u32,
    completed_encounters: u32,
    member_count: u8,
    leader: bool,
    holiday: bool,
}

impl UiGroupFinderProposal {
    /// Creates one server-authored proposal.
    ///
    /// # Errors
    ///
    /// Returns [`UiGroupFinderError`] for missing identities, impossible
    /// encounter progress, or a zero-member proposal.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        type_id: u32,
        dungeon_id: u32,
        name: impl Into<String>,
        texture: impl Into<String>,
        role: UiGroupFinderRole,
        has_responded: bool,
        total_encounters: u32,
        completed_encounters: u32,
        member_count: u8,
        leader: bool,
        holiday: bool,
    ) -> Result<Self, UiGroupFinderError> {
        let name = name.into();
        let texture = texture.into();
        if type_id == 0 || dungeon_id == 0 || name.is_empty() || texture.is_empty() {
            return Err(UiGroupFinderError::InvalidProposalIdentity);
        }
        if completed_encounters > total_encounters {
            return Err(UiGroupFinderError::InvalidEncounterProgress {
                completed: completed_encounters,
                total: total_encounters,
            });
        }
        if member_count == 0 {
            return Err(UiGroupFinderError::EmptyProposal);
        }
        Ok(Self {
            type_id,
            dungeon_id,
            name,
            texture,
            role,
            has_responded,
            total_encounters,
            completed_encounters,
            member_count,
            leader,
            holiday,
        })
    }

    /// Returns the dungeon finder type identifier.
    #[must_use]
    pub const fn type_id(&self) -> u32 {
        self.type_id
    }
    /// Returns the selected dungeon identifier.
    #[must_use]
    pub const fn dungeon_id(&self) -> u32 {
        self.dungeon_id
    }
    /// Returns the localized dungeon name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Returns the proposal texture identity.
    #[must_use]
    pub fn texture(&self) -> &str {
        &self.texture
    }
    /// Returns the local player's proposed role.
    #[must_use]
    pub const fn role(&self) -> UiGroupFinderRole {
        self.role
    }
    /// Reports whether the local player answered the proposal.
    #[must_use]
    pub const fn has_responded(&self) -> bool {
        self.has_responded
    }
    /// Returns the encounter count.
    #[must_use]
    pub const fn total_encounters(&self) -> u32 {
        self.total_encounters
    }
    /// Returns encounters already completed by the proposed group.
    #[must_use]
    pub const fn completed_encounters(&self) -> u32 {
        self.completed_encounters
    }
    /// Returns proposed member count.
    #[must_use]
    pub const fn member_count(&self) -> u8 {
        self.member_count
    }
    /// Reports whether the local player is proposed leader.
    #[must_use]
    pub const fn is_leader(&self) -> bool {
        self.leader
    }
    /// Reports whether this proposal targets a holiday dungeon.
    #[must_use]
    pub const fn is_holiday(&self) -> bool {
        self.holiday
    }
}

/// Seven-value server queue summary used by `GetLFGInfoServer`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UiGroupFinderServerInfo {
    in_party: bool,
    joined: bool,
    queued: bool,
    no_partial_clear: bool,
    achievement_count: u32,
    comment: String,
    slot_count: u32,
}

impl UiGroupFinderServerInfo {
    /// Creates one complete server queue summary.
    #[must_use]
    pub fn new(
        in_party: bool,
        joined: bool,
        queued: bool,
        no_partial_clear: bool,
        achievement_count: u32,
        comment: impl Into<String>,
        slot_count: u32,
    ) -> Self {
        Self {
            in_party,
            joined,
            queued,
            no_partial_clear,
            achievement_count,
            comment: comment.into(),
            slot_count,
        }
    }
    /// Reports whether the request represents a party.
    #[must_use]
    pub const fn in_party(&self) -> bool {
        self.in_party
    }
    /// Reports whether the player joined the finder.
    #[must_use]
    pub const fn joined(&self) -> bool {
        self.joined
    }
    /// Reports whether the request is queued.
    #[must_use]
    pub const fn queued(&self) -> bool {
        self.queued
    }
    /// Reports whether partially cleared instances are excluded.
    #[must_use]
    pub const fn no_partial_clear(&self) -> bool {
        self.no_partial_clear
    }
    /// Returns the number of authored achievement requirements.
    #[must_use]
    pub const fn achievement_count(&self) -> u32 {
        self.achievement_count
    }
    /// Returns the listing comment.
    #[must_use]
    pub fn comment(&self) -> &str {
        &self.comment
    }
    /// Returns the number of selected dungeon slots.
    #[must_use]
    pub const fn slot_count(&self) -> u32 {
        self.slot_count
    }
}

/// Summary of the current group role-check operation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UiGroupFinderRoleCheck {
    in_progress: bool,
    slot_count: u32,
    member_count: u32,
}

impl UiGroupFinderRoleCheck {
    /// Creates a role-check summary.
    #[must_use]
    pub const fn new(in_progress: bool, slot_count: u32, member_count: u32) -> Self {
        Self {
            in_progress,
            slot_count,
            member_count,
        }
    }
    /// Reports whether a role check is active.
    #[must_use]
    pub const fn in_progress(self) -> bool {
        self.in_progress
    }
    /// Returns selected slot count.
    #[must_use]
    pub const fn slot_count(self) -> u32 {
        self.slot_count
    }
    /// Returns participating member count.
    #[must_use]
    pub const fn member_count(self) -> u32 {
        self.member_count
    }
}

#[derive(Clone, Debug, Default)]
struct UiGroupFinderSnapshot {
    proposal: Option<UiGroupFinderProposal>,
    server: UiGroupFinderServerInfo,
    role_check: UiGroupFinderRoleCheck,
    listed_in_lfr: bool,
    party_lfg: bool,
    in_lfg_dungeon: bool,
    restrictions: bool,
}

/// Shared group-finder lifecycle read by native Lua closures.
#[derive(Clone, Debug, Default)]
pub struct UiGroupFinderState {
    inner: Rc<RefCell<UiGroupFinderSnapshot>>,
}

impl UiGroupFinderState {
    /// Creates the authoritative inactive world-session state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    /// Replaces the current proposal; `None` clears it.
    pub fn set_proposal(&self, proposal: Option<UiGroupFinderProposal>) {
        self.inner.borrow_mut().proposal = proposal;
    }
    /// Replaces the server queue summary.
    pub fn set_server_info(&self, server: UiGroupFinderServerInfo) {
        self.inner.borrow_mut().server = server;
    }
    /// Replaces the role-check summary.
    pub fn set_role_check(&self, role_check: UiGroupFinderRoleCheck) {
        self.inner.borrow_mut().role_check = role_check;
    }
    /// Publishes raid-finder listing state.
    pub fn set_listed_in_lfr(&self, listed: bool) {
        self.inner.borrow_mut().listed_in_lfr = listed;
    }
    /// Publishes whether the current party was assembled by LFG.
    pub fn set_party_lfg(&self, party_lfg: bool) {
        self.inner.borrow_mut().party_lfg = party_lfg;
    }
    /// Publishes whether the player occupies an LFG dungeon.
    pub fn set_in_lfg_dungeon(&self, in_dungeon: bool) {
        self.inner.borrow_mut().in_lfg_dungeon = in_dungeon;
    }
    /// Publishes whether the player is currently restricted from queueing.
    pub fn set_restrictions(&self, restrictions: bool) {
        self.inner.borrow_mut().restrictions = restrictions;
    }
    /// Returns the current proposal.
    #[must_use]
    pub fn proposal(&self) -> Option<UiGroupFinderProposal> {
        self.inner.borrow().proposal.clone()
    }
    /// Returns the server queue summary.
    #[must_use]
    pub fn server_info(&self) -> UiGroupFinderServerInfo {
        self.inner.borrow().server.clone()
    }
    /// Returns the role-check summary.
    #[must_use]
    pub fn role_check(&self) -> UiGroupFinderRoleCheck {
        self.inner.borrow().role_check
    }
    /// Reports raid-finder listing state.
    #[must_use]
    pub fn is_listed_in_lfr(&self) -> bool {
        self.inner.borrow().listed_in_lfr
    }
    /// Reports whether the current party was assembled by LFG.
    #[must_use]
    pub fn is_party_lfg(&self) -> bool {
        self.inner.borrow().party_lfg
    }
    /// Reports whether the player occupies an LFG dungeon.
    #[must_use]
    pub fn is_in_lfg_dungeon(&self) -> bool {
        self.inner.borrow().in_lfg_dungeon
    }
    /// Reports active queue restrictions.
    #[must_use]
    pub fn has_restrictions(&self) -> bool {
        self.inner.borrow().restrictions
    }
}

/// Invalid group-finder state supplied by session projection.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UiGroupFinderError {
    /// Proposal identity fields are absent.
    #[error("group-finder proposal requires type, dungeon, name, and texture identities")]
    InvalidProposalIdentity,
    /// Encounter progress exceeds the dungeon encounter count.
    #[error("group-finder proposal completed {completed} of only {total} encounters")]
    InvalidEncounterProgress {
        /// Rejected completed encounter count.
        completed: u32,
        /// Authored total encounter count.
        total: u32,
    },
    /// A proposal cannot contain no members.
    #[error("group-finder proposal must contain at least one member")]
    EmptyProposal,
}
