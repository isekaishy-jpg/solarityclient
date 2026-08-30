//! Login, realm, character-selection, and patch-stage glue-screen state.
//!
//! `CGlueMgr.cpp`, `PatchDownloadGlue.cpp`, `ScanDLLGlue.cpp`, and
//! `SurveyDownloadGlue.cpp` establish a stock pre-world UI mode separate from
//! in-world frames.

mod character;

mod c_glue_mgr;
mod patch_download_glue;
mod scan_dll_glue;
mod survey_download_glue;
