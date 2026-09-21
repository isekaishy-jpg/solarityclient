//! Owned shadow recording overlaps scene recording without sharing command pools.

mod batch;
mod capture;
mod encode;
mod execution;
mod pools;
pub(super) mod scene;
mod types;

pub(super) use batch::ShadowRecording;
pub use execution::{WorldFrameExecution, WorldRecordingCompletion};
pub(super) use pools::RecordingPools;
pub(super) use types::ShadowSubmission;
