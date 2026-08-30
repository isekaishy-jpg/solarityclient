//! Vulkan image allocation, upload, sampling, and renderer-side texture cache.
//!
//! `Texture.cpp`, `TextureBlob.cpp`, `TextureCache.cpp`, and the stock GX
//! texture backends establish the split between decoded pixels and GPU images.

mod types;
