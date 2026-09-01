//! Stock feature-frame ownership above the reusable FrameXML widget layer.

mod action_bar;
mod battlefield;
mod battlenet;
mod chat;
mod commentator;
mod container;
mod dress_up;
mod guild_bank;
mod health_bar;
mod item_text;
mod loot;
mod merchant;
mod minimap;
mod name_plate;
mod paper_doll;
mod party;
mod portrait;
mod quest;
mod spell_book;
mod taxi_map;
mod tooltip;
mod trade;
mod trade_skill;
mod trainer;

pub(crate) use action_bar::register_globals as register_action_bar_globals;
pub use action_bar::{UiActionBarPageError, UiActionBarState};
pub(crate) use battlefield::register_globals as register_battlefield_globals;
pub use battlefield::{
    MAX_BATTLEFIELD_QUEUES, MAX_WORLD_PVP_QUEUES, UiBattlefieldQueueError, UiBattlefieldQueueState,
    UiBattlefieldQueueStatus, UiBattlefieldSlot, UiWorldPvpQueueSlot,
};
pub(crate) use battlenet::UiBattleNetState;
pub(crate) use minimap::{
    initialize_state as initialize_minimap_state, register_methods as register_minimap_methods,
};
