//! Immutable grass/detail buffers, native shader state, and fenced retirement.

mod pipeline;
mod resource;
mod types;

pub(in crate::device) use pipeline::DetailPipeline;
pub(in crate::device) use resource::{DetailCreateContext, DetailRegistry};
pub use types::{GroundDetailDraw, GroundDetailFrame, GroundDetailFrameError};
