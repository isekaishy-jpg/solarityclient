//! Quest frame and quest point-of-interest presentation above systems-owned quest state.

mod log;
mod quest_frame;

pub(crate) use log::register_globals;
pub use log::{UiQuestLogEntry, UiQuestLogQuest, UiQuestLogState};
