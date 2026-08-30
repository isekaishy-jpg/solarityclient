//! Ordered resolution of archive, streaming, and permitted loose-file sources.
//!
//! Stock names this responsibility in `FileStack_Streaming.cpp` and
//! `FileStack_Win32.cpp`. Resolution order must follow observed stock behavior;
//! this module must not invent a missing-file fallback.

mod file_cache;
mod filestack_streaming;
mod filestack_win32;
