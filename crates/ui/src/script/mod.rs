//! PUC Lua 5.1 execution, stock API registration, and protected-call policy.
//!
//! `SimpleScript.cpp` and the numerous `*Script.cpp` binding families establish
//! this boundary. `mlua` is configured for stock Lua 5.1.5 semantics; API
//! permission checks are behavior to reproduce, not bypass.

mod clock;
mod handlers;
mod model_intent;
mod movement_intent;
mod network_intent;
mod process_intent;
mod runtime_state;
mod simple_script;
mod status;
mod templates;

pub(crate) use simple_script::UiSoundSuppression;

pub use clock::UiClientClock;
pub use handlers::{UiScriptBinding, UiScriptHandler, UiScriptNode, UiScriptPlan, UiScriptTarget};
pub use model_intent::{UiModelAction, UiModelInstance};
pub(crate) use movement_intent::UiMovementInput;
pub use movement_intent::{UiMovementAction, UiMovementCommand, UiMovementControl};
pub use network_intent::{
    UiCharacterDirectory, UiCharacterEquipment, UiCharacterInfo, UiCharacterPetPreview,
    UiCharacterSelectionPreview, UiGlueNetworkAction, UiGlueNetworkStatus, UiLoginRequest,
    UiRealmCategory, UiRealmDirectory, UiRealmFlags, UiRealmInfo, UiRealmSort, UiRealmVersion,
};
pub use process_intent::UiProcessAction;
pub use simple_script::{
    UiGlueMediaAction, UiGlueMediaIntent, UiGlueMovieRequest, UiScriptEnvironment, UiScriptRuntime,
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
    UiRuntimeObjectPlan, UiRuntimeText, UiRuntimeTextColorChange,
};
pub(crate) use simple_script::{OBJECT_REGISTRY, UiScriptEventDispatch, mark_live_state_changed};
