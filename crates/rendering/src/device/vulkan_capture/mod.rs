//! On-demand screenshot and reusable video readback of the final framebuffer.

mod frame;
mod scale;
mod video;

pub use frame::CapturedFrame;
pub(super) use frame::FrameReadback;
pub(super) use video::VideoReadback;

use super::VulkanError;
