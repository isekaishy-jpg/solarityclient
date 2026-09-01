//! Login, realm, character-selection, and patch-stage glue-screen state.
//!
//! `CGlueMgr.cpp`, `PatchDownloadGlue.cpp`, `ScanDLLGlue.cpp`, and
//! `SurveyDownloadGlue.cpp` establish a stock pre-world UI mode separate from
//! in-world frames.

mod character;

mod c_glue_mgr;
mod patch_download_glue;
mod pointer;
mod scan_dll_glue;
mod status;
mod survey_download_glue;
mod types;

pub use c_glue_mgr::GlueManager;
pub use character::{
    UiCharacterCreationError, UiCharacterCreationPreview, UiCharacterCreationRequest,
    UiCharacterCreationState, UiCharacterExpansion, UiCreationClassRoles,
};
pub use status::GlueError;
pub use types::{
    GlueInitialScreen, GlueObject, GlueStartupReport, UiKeyboardModifiers, UiPointerButton,
    UiPointerDispatch,
};
