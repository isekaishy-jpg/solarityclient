//! Validated cross-resource terrain indexed-draw packets.

mod prepare;
mod types;

pub(in crate::device) use prepare::prepare_draw;
pub use types::TerrainPreparedDraw;
