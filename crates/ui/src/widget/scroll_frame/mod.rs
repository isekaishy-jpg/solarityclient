//! Scriptable scroll-frame viewport and child-offset behavior recovered from `CSimpleScrollFrame` RTTI.

mod types;

pub(crate) use types::nearest_owning_scroll_frame;
pub use types::{UiScrollFramePlan, UiScrollFrameState};
