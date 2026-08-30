//! Stock UI event identifiers, subscriptions, argument contracts, and dispatch.
//!
//! `ScriptEvents.cpp` provides direct source evidence. Dispatch preserves stock
//! ordering and Lua-visible argument shapes rather than adding convenience
//! coercions or silent event fallbacks.

mod dispatch;
mod payload;
mod registry;
mod types;
