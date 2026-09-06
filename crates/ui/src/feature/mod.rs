//! Stock feature-frame ownership above the reusable FrameXML widget layer.

mod account;
mod action_bar;
mod battlefield;
mod battlenet;
mod chat;
mod commentator;
mod companion;
mod container;
mod dress_up;
mod group_finder;
mod guild;
mod guild_bank;
mod health_bar;
mod item_text;
mod loot;
mod mail;
mod merchant;
mod minimap;
mod name_plate;
mod paper_doll;
mod party;
mod pet_action;
mod portrait;
mod quest;
mod rune;
mod skill;
mod social;
mod spell_book;
mod stance;
mod support;
mod tabard;
mod taxi_map;
mod tooltip;
mod trade;
mod trade_skill;
mod trainer;
mod voice_chat;
mod world_map;
mod world_state_ui;

pub(crate) use account::register_globals as register_account_globals;
pub use account::{UiAccountExpansion, UiAccountState};
pub(crate) use action_bar::register_globals as register_action_bar_globals;
pub use action_bar::{
    UI_ACTION_SLOT_COUNT, UiActionBarPageError, UiActionBarState, UiActionBarStateError,
};
pub(crate) use battlefield::register_globals as register_battlefield_globals;
pub use battlefield::{
    MAX_BATTLEFIELD_QUEUES, MAX_WORLD_PVP_QUEUES, UiBattlefieldQueueError, UiBattlefieldQueueState,
    UiBattlefieldQueueStatus, UiBattlefieldSlot, UiBattlegroundType, UiWorldPvpQueueSlot,
};
pub(crate) use battlenet::UiBattleNetState;
pub use chat::{
    UI_CHAT_WINDOW_COUNT, UiChannelCategory, UiChannelDisplay, UiChannelMember, UiChannelState,
    UiChatWindow, UiChatWindowState,
};
pub(crate) use chat::{
    register_channel_globals, register_chat_type_globals, register_chat_window_globals,
};
pub(crate) use companion::register_globals as register_companion_globals;
pub use companion::{UiCompanion, UiCompanionState, UiCompanionType};
pub(crate) use container::register_globals as register_container_globals;
pub(crate) use group_finder::register_globals as register_group_finder_globals;
pub use group_finder::{
    UiGroupFinderError, UiGroupFinderProposal, UiGroupFinderRole, UiGroupFinderRoleCheck,
    UiGroupFinderServerInfo, UiGroupFinderState,
};
pub use guild::UiGuildState;
pub(crate) use guild::register_globals as register_guild_globals;
pub use loot::UiLootState;
pub(crate) use loot::register_globals as register_loot_globals;
pub(crate) use mail::register_globals as register_mail_globals;
pub use mail::{UI_BASE_SEND_MAIL_PRICE, UiMailComposeState};
pub(crate) use minimap::register_globals as register_minimap_globals;
pub use minimap::{
    UiMinimapState, UiMinimapTrackingState, UiTrackingCategory, UiTrackingError, UiTrackingType,
};
pub(crate) use minimap::{
    initialize_state as initialize_minimap_state, register_methods as register_minimap_methods,
};
pub(crate) use paper_doll::register_globals as register_paper_doll_globals;
pub(crate) use party::register_globals as register_group_roster_globals;
pub use party::{UiGroupRosterState, UiLootMethod};
pub(crate) use pet_action::register_globals as register_pet_action_globals;
pub use pet_action::{UI_PET_ACTION_SLOT_COUNT, UiPetAction, UiPetActionState};
pub(crate) use quest::register_globals as register_quest_log_globals;
pub use quest::{UiQuestLogEntry, UiQuestLogQuest, UiQuestLogState};
pub(crate) use rune::register_globals as register_rune_globals;
pub use rune::{UI_RUNE_SLOT_COUNT, UiRune, UiRuneState, UiRuneType};
pub(crate) use skill::register_globals as register_skill_globals;
pub use skill::{UiSkillLine, UiSkillLineSkill, UiSkillLineState};
pub use social::UiSocialQueryState;
pub(crate) use social::register_globals as register_social_globals;
pub(crate) use spell_book::register_globals as register_spell_book_globals;
pub use spell_book::{UiSpellBookState, UiSpellBookTab};
pub(crate) use stance::register_globals as register_stance_globals;
pub use stance::{UiPossessAction, UiShapeshiftForm, UiStanceState};
pub use support::UiSupportState;
pub(crate) use support::register_globals as register_support_globals;
pub use tabard::UiTabardState;
pub(crate) use tabard::register_globals as register_tabard_globals;
pub use voice_chat::UiVoiceChatState;
pub(crate) use voice_chat::register_globals as register_voice_chat_globals;
pub use world_map::UiWorldMapState;
pub(crate) use world_map::{
    register_globals as register_world_map_globals, register_quest_poi_methods,
};
pub(crate) use world_state_ui::register_globals as register_world_state_ui_globals;
pub use world_state_ui::{UiWorldStateIndicator, UiWorldStateUiState};
