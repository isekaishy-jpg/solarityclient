//! Stock UI event identifiers, subscriptions, argument contracts, and dispatch.
//!
//! `ScriptEvents.cpp` provides direct source evidence. Dispatch preserves stock
//! ordering and Lua-visible argument shapes rather than adding convenience
//! coercions or silent event fallbacks.

mod dispatch;
mod frame_registry;
mod payload;
mod registry;
mod types;

pub use payload::{UiEventArgument, UiEventPayload};
pub use types::{UiEventDispatch, UiEventError};

pub(crate) use frame_registry::canonical_frame_event;
pub(crate) use payload::LEGACY_EVENT_ARGUMENT_GLOBALS;
pub(crate) use registry::canonical_glue_event;
