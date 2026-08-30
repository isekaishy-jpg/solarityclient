//! SDL3-backed windowing and narrow Windows services still required by stock.
//!
//! The stock `OsCall.cpp`, `OsClipboard.cpp`, `OsIME.cpp`, `OsURLDownload.cpp`,
//! and related files establish the platform surface. Platform behavior enters
//! the rest of the client only through typed runtime events and services.

mod lcd;

mod blizzard_cursor;
mod cursor;
mod os_call;
mod os_clipboard;
mod os_ime;
mod os_secure_random;
mod os_url_download;
mod os_version_hash;
