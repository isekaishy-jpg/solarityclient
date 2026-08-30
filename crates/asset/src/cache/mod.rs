//! Lifetime and lookup policy for decoded asset caches.
//!
//! `DBCache.cpp`, `M2Cache.cpp`, and `TextureCache.cpp` show that stock keeps
//! cache policy distinct from archive access and format decoding.

mod types;

mod m2_model;

pub use m2_model::M2ModelCache;
