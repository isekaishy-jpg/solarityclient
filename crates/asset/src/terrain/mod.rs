//! WDT, ADT, WDL, map-chunk, liquid, and shadow asset decoding.
//!
//! This module owns the data contracts evidenced by `Map.cpp`, `MapChunk.cpp`,
//! `MapChunkLiquid.cpp`, `MapLowDetail.cpp`, and `MapShadow.cpp`; runtime world
//! simulation and rendering remain outside the asset crate.

mod map;
mod map_area;
mod map_chunk;
mod map_chunk_liquid;
mod map_load;
mod map_low_detail;
mod map_mem;
mod map_shadow;

pub use map::{DecodedTerrainTile, TerrainMap};
pub use map_area::{TerrainTile, TerrainTileIndex};
pub use map_chunk::{
    TerrainChunk, TerrainChunkIndex, TerrainDoodadPlacement, TerrainSoundEmitter,
    TerrainTextureLayer, TerrainWorldModelPlacement,
};
