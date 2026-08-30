//! PUC Lua 5.1 execution, stock API registration, and protected-call policy.
//!
//! `SimpleScript.cpp` and the numerous `*Script.cpp` binding families establish
//! this boundary. `mlua` is configured for stock Lua 5.1.5 semantics; API
//! permission checks are behavior to reproduce, not bypass.

mod script_events;
mod simple_script;
