//! Retained battlefield queue slots consumed by FrameXML.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use thiserror::Error;

/// Queue slots encoded by `SMSG_BATTLEFIELD_STATUS` in build 12340.
pub const MAX_BATTLEFIELD_QUEUES: usize = 2;

/// World-PvP queue slots authored by build-12340 FrameXML.
pub const MAX_WORLD_PVP_QUEUES: usize = 1;

/// One client-catalog battleground option in display order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiBattlegroundType {
    pub(super) name: String,
    pub(super) description: String,
    pub(super) maximum_group_size: u32,
    pub(super) can_enter: bool,
    pub(super) holiday: bool,
    pub(super) random: bool,
    pub(super) battleground_id: u32,
}

impl UiBattlegroundType {
    /// Creates one battleground selection row.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        can_enter: bool,
        holiday: bool,
        random: bool,
        battleground_id: u32,
    ) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            maximum_group_size: 0,
            can_enter,
            holiday,
            random,
            battleground_id,
        }
    }

    /// Adds the localized description and maximum premade-group size used by
    /// the battlefield-selection window.
    #[must_use]
    pub fn with_details(mut self, description: impl Into<String>, maximum_group_size: u32) -> Self {
        self.description = description.into();
        self.maximum_group_size = maximum_group_size;
        self
    }
}

/// Script-visible lifecycle of one battlefield queue slot.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UiBattlefieldQueueStatus {
    /// The slot has no queue identifier.
    #[default]
    None,
    /// The player is waiting in the queue.
    Queued,
    /// The server has offered entry to the instance.
    Confirm,
    /// The player is inside the battlefield.
    Active,
}

impl UiBattlefieldQueueStatus {
    /// Returns the exact lowercase FrameXML status token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Queued => "queued",
            Self::Confirm => "confirm",
            Self::Active => "active",
        }
    }
}

/// Complete seven-value projection for one queue slot.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UiBattlefieldSlot {
    status: UiBattlefieldQueueStatus,
    map_name: Option<String>,
    instance_id: u32,
    minimum_level: u8,
    maximum_level: u8,
    team_size: u8,
    registered_match: bool,
}

impl UiBattlefieldSlot {
    /// Creates an inactive slot with the stock `none` status.
    #[must_use]
    pub fn inactive() -> Self {
        Self::default()
    }

    /// Creates one active queue projection from decoded session state.
    ///
    /// # Errors
    ///
    /// Returns [`UiBattlefieldQueueError`] when an active status has no map,
    /// its level range is reversed, or arena team size is not 0, 2, 3, or 5.
    pub fn active(
        status: UiBattlefieldQueueStatus,
        map_name: impl Into<String>,
        instance_id: u32,
        level_range: (u8, u8),
        team_size: u8,
        registered_match: bool,
    ) -> Result<Self, UiBattlefieldQueueError> {
        let map_name = map_name.into();
        if status == UiBattlefieldQueueStatus::None || map_name.is_empty() {
            return Err(UiBattlefieldQueueError::InvalidActiveSlot);
        }
        if level_range.0 > level_range.1 {
            return Err(UiBattlefieldQueueError::InvalidLevelRange {
                minimum: level_range.0,
                maximum: level_range.1,
            });
        }
        if !matches!(team_size, 0 | 2 | 3 | 5) {
            return Err(UiBattlefieldQueueError::InvalidTeamSize { team_size });
        }
        Ok(Self {
            status,
            map_name: Some(map_name),
            instance_id,
            minimum_level: level_range.0,
            maximum_level: level_range.1,
            team_size,
            registered_match,
        })
    }

    /// Returns the queue lifecycle token.
    #[must_use]
    pub const fn status(&self) -> UiBattlefieldQueueStatus {
        self.status
    }

    /// Returns the localized battleground or arena name.
    #[must_use]
    pub fn map_name(&self) -> Option<&str> {
        self.map_name.as_deref()
    }

    /// Returns the client-visible instance identifier.
    #[must_use]
    pub const fn instance_id(&self) -> u32 {
        self.instance_id
    }

    /// Returns the bracket's minimum level.
    #[must_use]
    pub const fn minimum_level(&self) -> u8 {
        self.minimum_level
    }

    /// Returns the bracket's maximum level.
    #[must_use]
    pub const fn maximum_level(&self) -> u8 {
        self.maximum_level
    }

    /// Returns 2, 3, or 5 for arenas and zero for battlegrounds.
    #[must_use]
    pub const fn team_size(&self) -> u8 {
        self.team_size
    }

    /// Reports whether this is a rated arena match.
    #[must_use]
    pub const fn registered_match(&self) -> bool {
        self.registered_match
    }
}

/// Shared two-slot battlefield queue projection.
#[derive(Clone, Debug, Default)]
pub struct UiBattlefieldQueueState {
    slots: Rc<RefCell<[UiBattlefieldSlot; MAX_BATTLEFIELD_QUEUES]>>,
    world_pvp: Rc<RefCell<UiWorldPvpQueueSlot>>,
    battleground_types: Rc<RefCell<Vec<UiBattlegroundType>>>,
    selected_battleground: Rc<Cell<usize>>,
}

impl UiBattlefieldQueueState {
    /// Creates the pre-queue world-session state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns one one-based queue slot.
    ///
    /// # Errors
    ///
    /// Returns [`UiBattlefieldQueueError`] outside the stock two-slot range.
    pub fn slot(&self, index: usize) -> Result<UiBattlefieldSlot, UiBattlefieldQueueError> {
        self.slots
            .borrow()
            .get(index.wrapping_sub(1))
            .cloned()
            .ok_or(UiBattlefieldQueueError::InvalidIndex { index })
    }

    /// Replaces one one-based slot after a session status packet.
    ///
    /// # Errors
    ///
    /// Returns [`UiBattlefieldQueueError`] outside the stock two-slot range.
    pub fn set_slot(
        &self,
        index: usize,
        slot: UiBattlefieldSlot,
    ) -> Result<(), UiBattlefieldQueueError> {
        let mut slots = self.slots.borrow_mut();
        let destination = slots
            .get_mut(index.wrapping_sub(1))
            .ok_or(UiBattlefieldQueueError::InvalidIndex { index })?;
        *destination = slot;
        Ok(())
    }

    /// Returns the single world-PvP queue slot.
    ///
    /// # Errors
    ///
    /// Returns [`UiBattlefieldQueueError`] unless `index` is one.
    pub fn world_pvp_slot(
        &self,
        index: usize,
    ) -> Result<UiWorldPvpQueueSlot, UiBattlefieldQueueError> {
        if index != 1 {
            return Err(UiBattlefieldQueueError::InvalidWorldPvpIndex { index });
        }
        Ok(self.world_pvp.borrow().clone())
    }

    /// Replaces the sole world-PvP queue slot from battlefield-manager state.
    pub fn set_world_pvp_slot(&self, slot: UiWorldPvpQueueSlot) {
        *self.world_pvp.borrow_mut() = slot;
    }

    /// Replaces the level-filtered battleground selection catalog.
    pub fn replace_battleground_types(&self, battleground_types: Vec<UiBattlegroundType>) {
        *self.battleground_types.borrow_mut() = battleground_types;
        self.selected_battleground.set(0);
    }

    pub(super) fn battleground_type(&self, index: usize) -> Option<UiBattlegroundType> {
        index
            .checked_sub(1)
            .and_then(|index| self.battleground_types.borrow().get(index).cloned())
    }

    /// Returns the number of level-filtered battleground selection rows.
    #[must_use]
    pub fn battleground_type_count(&self) -> usize {
        self.battleground_types.borrow().len()
    }

    pub(super) fn set_selected_battleground(&self, index: usize) {
        self.selected_battleground.set(index);
    }

    pub(super) fn selected_battleground(&self) -> Option<UiBattlegroundType> {
        self.battleground_types
            .borrow()
            .get(self.selected_battleground.get())
            .cloned()
    }
}

/// Script-visible world-PvP battlefield-manager queue state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UiWorldPvpQueueSlot {
    status: UiBattlefieldQueueStatus,
    map_name: Option<String>,
    queue_id: u32,
    expiration_milliseconds: u32,
}

impl UiWorldPvpQueueSlot {
    /// Creates the stock inactive world-PvP slot.
    #[must_use]
    pub fn inactive() -> Self {
        Self::default()
    }

    /// Creates an active battlefield-manager queue projection.
    ///
    /// # Errors
    ///
    /// Returns [`UiBattlefieldQueueError`] for `none` or an empty map name.
    pub fn active(
        status: UiBattlefieldQueueStatus,
        map_name: impl Into<String>,
        queue_id: u32,
        expiration_milliseconds: u32,
    ) -> Result<Self, UiBattlefieldQueueError> {
        let map_name = map_name.into();
        if status == UiBattlefieldQueueStatus::None || map_name.is_empty() {
            return Err(UiBattlefieldQueueError::InvalidActiveSlot);
        }
        Ok(Self {
            status,
            map_name: Some(map_name),
            queue_id,
            expiration_milliseconds,
        })
    }

    /// Returns the queue lifecycle token.
    #[must_use]
    pub const fn status(&self) -> UiBattlefieldQueueStatus {
        self.status
    }

    /// Returns the localized outdoor battlefield name.
    #[must_use]
    pub fn map_name(&self) -> Option<&str> {
        self.map_name.as_deref()
    }

    /// Returns the battlefield-manager battle identifier.
    #[must_use]
    pub const fn queue_id(&self) -> u32 {
        self.queue_id
    }

    /// Returns remaining invitation time in milliseconds.
    #[must_use]
    pub const fn expiration_milliseconds(&self) -> u32 {
        self.expiration_milliseconds
    }
}

/// Invalid battlefield state supplied by the session projection.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UiBattlefieldQueueError {
    /// A caller addressed a slot outside the build-12340 range.
    #[error("battlefield queue index {index} is outside 1..={MAX_BATTLEFIELD_QUEUES}")]
    InvalidIndex {
        /// Rejected one-based index.
        index: usize,
    },
    /// A caller addressed a world-PvP slot other than the sole stock slot.
    #[error("world-PVP queue index {index} is outside 1..={MAX_WORLD_PVP_QUEUES}")]
    InvalidWorldPvpIndex {
        /// Rejected one-based index.
        index: usize,
    },
    /// An active slot lacks an active status or map identity.
    #[error("active battlefield queue slot requires a status and map name")]
    InvalidActiveSlot,
    /// The bracket endpoints are reversed.
    #[error("battlefield level range {minimum}..={maximum} is reversed")]
    InvalidLevelRange {
        /// Rejected lower endpoint.
        minimum: u8,
        /// Rejected upper endpoint.
        maximum: u8,
    },
    /// Arena team size is not part of the build-12340 vocabulary.
    #[error("battlefield team size {team_size} is not 0, 2, 3, or 5")]
    InvalidTeamSize {
        /// Rejected team size.
        team_size: u8,
    },
}
