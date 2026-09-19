//! Scoped main-thread continuation between M2 admission and ordered publication.

mod admission;
mod completion;
mod continuation;
mod entry;
mod input;
mod scene;

use super::super::M2Frame;
use input::{FrameAdmission, FrameView};

/// Pins the current M2 owner while independent main-thread work uses disjoint owners.
/// Root poses and admitted geometry may still be running. Dropping an unfinished
/// preparation reclaims that state at an explicitly profiled abandonment boundary;
/// the non-Send frame owner keeps this cleanup on the coordinator thread.
#[must_use = "complete the M2 frame after independent scene work"]
pub(in crate::application::terrain_frame) struct PendingM2Frame<'frame> {
    frame: Option<&'frame mut M2Frame>,
    view: FrameView,
    admission: FrameAdmission,
    /// One causal identity covers setup, any resumed traversal and worker results.
    trace: solarity_profiling::TraceContext,
}
