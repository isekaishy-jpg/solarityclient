//! PUC Lua 5.1 execution, stock API registration, and protected-call policy.
//!
//! `SimpleScript.cpp` and the numerous `*Script.cpp` binding families establish
//! this boundary. `mlua` is configured for stock Lua 5.1.5 semantics; API
//! permission checks are behavior to reproduce, not bypass.

mod handlers;
mod script_events;
mod simple_script;
mod status;

pub use handlers::{UiScriptBinding, UiScriptHandler, UiScriptNode, UiScriptPlan, UiScriptTarget};
pub use simple_script::UiScriptRuntime;
pub use status::UiScriptError;
