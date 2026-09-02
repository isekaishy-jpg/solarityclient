//! Validated cross-resource M2 indexed-draw packets.

mod light_bank;
mod prepare;
mod types;

pub use light_bank::M2SceneLightBank;
pub(in crate::device) use prepare::prepare_draw;
pub use types::M2PreparedDraw;
