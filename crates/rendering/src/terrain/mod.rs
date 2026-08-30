//! GPU terrain, liquid, shadow, doodad, and low-detail map rendering.
//!
//! The boundary follows `MapChunk.cpp`, `MapChunkLiquid.cpp`, `MapShadow.cpp`,
//! `MapLowDetail.cpp`, and `DetailDoodad.cpp`. Decoded map data is borrowed from
//! the asset facade.

mod detail_doodad;
mod map_weather;
