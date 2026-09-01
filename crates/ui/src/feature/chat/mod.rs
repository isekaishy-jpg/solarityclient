//! Chat frames, chat bubbles, channel presentation, and message interaction behavior.

mod channel;
mod chat_bubble_frame;
mod chat_frame;
mod chat_type;
mod chat_window;

pub(crate) use channel::register_globals as register_channel_globals;
pub use channel::{UiChannelCategory, UiChannelDisplay, UiChannelMember, UiChannelState};
pub(crate) use chat_type::register_globals as register_chat_type_globals;
pub(crate) use chat_window::register_globals as register_chat_window_globals;
pub use chat_window::{UI_CHAT_WINDOW_COUNT, UiChatWindow, UiChatWindowState};
