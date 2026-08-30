//! Login, realm, character-selection, and patch-stage glue-screen state.
//!
//! `CGlueMgr.cpp`, `PatchDownloadGlue.cpp`, `ScanDLLGlue.cpp`, and
//! `SurveyDownloadGlue.cpp` establish a stock pre-world UI mode separate from
//! in-world frames.

mod character;

mod c_glue_mgr;
mod patch_download_glue;
mod scan_dll_glue;
mod status;
mod survey_download_glue;
mod types;

pub use c_glue_mgr::GlueManager;
pub use status::GlueError;
pub use types::{GlueObject, GlueStartupReport};
