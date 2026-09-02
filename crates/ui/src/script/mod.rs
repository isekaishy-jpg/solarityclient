//! PUC Lua 5.1 execution, stock API registration, and protected-call policy.
//!
//! `SimpleScript.cpp` and the numerous `*Script.cpp` binding families establish
//! this boundary. `mlua` is configured for stock Lua 5.1.5 semantics; API
//! permission checks are behavior to reproduce, not bypass.

mod clock;
mod handlers;
mod network_intent;
mod process_intent;
mod runtime_state;
mod simple_script;
mod status;
mod templates;

pub use clock::UiClientClock;
pub use handlers::{UiScriptBinding, UiScriptHandler, UiScriptNode, UiScriptPlan, UiScriptTarget};
pub use network_intent::{
    UiCharacterDirectory, UiCharacterEquipment, UiCharacterInfo, UiCharacterPetPreview,
    UiCharacterSelectionPreview, UiGlueNetworkAction, UiGlueNetworkStatus, UiLoginRequest,
    UiRealmCategory, UiRealmDirectory, UiRealmFlags, UiRealmInfo, UiRealmSort, UiRealmVersion,
};
pub use process_intent::UiProcessAction;
pub use simple_script::{
    UiGlueMediaIntent, UiGlueMovieRequest, UiScriptEnvironment, UiScriptRuntime,
    UiScriptRuntimePlan,
};
pub use status::UiScriptError;
pub use templates::{
    UiDeferredRuntimeTemplate, UiRuntimeTemplate, UiRuntimeTemplateNode, UiRuntimeTemplatePlan,
};

pub(crate) use network_intent::UiGlueNetworkBridge;
pub(crate) use process_intent::UiProcessBridge;
pub(crate) use runtime_state::{
    UiRuntimeAnchor, UiRuntimeModelLight, UiRuntimeModelLightSets, UiRuntimeObject,
    UiRuntimeObjectPlan, UiRuntimeText,
};
pub(crate) use simple_script::OBJECT_REGISTRY;
