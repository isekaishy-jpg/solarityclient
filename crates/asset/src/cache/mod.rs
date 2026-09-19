//! Lifetime and lookup policy for decoded asset caches.
//!
//! `DBCache.cpp`, `M2Cache.cpp`, and `TextureCache.cpp` show that stock keeps
//! cache policy distinct from archive access and format decoding.

mod resource;
mod types;

mod blp_texture;
mod m2_model;
mod wmo_model;

pub use blp_texture::BlpTextureCache;
pub use m2_model::{
    M2CacheCollection, M2CacheService, M2Load, M2LoadDependency, M2LoadError, M2LoadProducer,
    M2LoadRequest, M2ModelCache,
};
pub use resource::{ResourceCacheClock, ResourceLease, ResourceWeak};
pub use wmo_model::WmoModelCache;
