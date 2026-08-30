//! Cross-registry validation and immutable UI draw packets.

mod prepare;
mod types;

pub(in crate::device) use prepare::prepare_draw;
pub use types::UiPreparedDraw;
