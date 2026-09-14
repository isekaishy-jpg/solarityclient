//! Frame orchestration preserves independent animation, attachment, light, shadow and visible work.

pub(super) mod demand;
mod diagnostics;
mod frame;
pub(super) mod poses;
mod publication;
pub(super) mod receivers;
