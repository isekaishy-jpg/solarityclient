//! Validated cross-resource M2 indexed-draw packets.

mod instance;
mod light_bank;
mod prepare;
mod template;
mod types;
pub use template::M2DrawTemplate;

pub use light_bank::M2SceneLightBank;
pub(in crate::device) use prepare::prepare_draw;
pub use types::M2PreparedDraw;
