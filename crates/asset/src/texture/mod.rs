//! BLP and TGA decoding plus CPU-side texture descriptions.
//!
//! `blp.cpp`, `tga.cpp`, `TextureBlob.cpp`, and `Texture.cpp` establish the
//! stock decoding boundary. Vulkan images and samplers belong to rendering.

mod block_compression;
mod blp;
mod texture_blob;
mod texture_cache;
mod texture_int;
mod texture_source;
mod tga;

pub use block_compression::{BlpBlockCompression, BlpBlockMip};
pub use blp::DecodedBlpTexture;
pub use texture_source::BlpTextureSource;
