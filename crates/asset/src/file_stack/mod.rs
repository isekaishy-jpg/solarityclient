//! Ordered resolution of archive, streaming, and permitted loose-file sources.
//!
//! Stock names this responsibility in `FileStack_Streaming.cpp` and
//! `FileStack_Win32.cpp`. Resolution order must follow observed stock behavior;
//! this module must not invent a missing-file fallback.

mod file_cache;
mod filestack_addons;
mod filestack_cinematics;
mod filestack_localized_document;
mod filestack_streaming;
mod filestack_win32;
mod handle;

pub use filestack_localized_document::LocalizedDocument;
pub use filestack_streaming::{AssetRead, AssetStore};
pub use filestack_win32::ArchiveCatalog;
pub use handle::AssetStoreHandle;
