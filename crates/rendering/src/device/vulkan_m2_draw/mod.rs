//! Validated cross-resource M2 indexed-draw packets.

mod prepare;
mod types;

pub(in crate::device) use prepare::prepare_draw;
pub use types::M2PreparedDraw;
