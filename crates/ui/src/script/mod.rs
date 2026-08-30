//! PUC Lua 5.1 execution, stock API registration, and protected-call policy.
//!
//! `SimpleScript.cpp` and the numerous `*Script.cpp` binding families establish
//! this boundary. `mlua` is configured for stock Lua 5.1.5 semantics; API
//! permission checks are behavior to reproduce, not bypass.

mod handlers;
mod runtime_state;
mod script_events;
mod simple_script;
mod status;
mod templates;

pub use handlers::{UiScriptBinding, UiScriptHandler, UiScriptNode, UiScriptPlan, UiScriptTarget};
pub use simple_script::{UiScriptEnvironment, UiScriptRuntime, UiScriptRuntimePlan};
pub use status::UiScriptError;
pub use templates::{UiRuntimeTemplate, UiRuntimeTemplateNode, UiRuntimeTemplatePlan};

pub(crate) use runtime_state::{UiRuntimeAnchor, UiRuntimeObject, UiRuntimeObjectPlan};
