//! Stock-ordered world capture, parallel encoding and main-owned graphics submission.
mod bindings;
mod context;
mod draws;
mod frame;
mod ground_detail;
mod instances;
mod low_detail;
mod order;
mod sky;
mod submission;
use bindings::WorldCommandBindings;
pub(super) use context::RecordContext;
pub(super) use draws::dynamic_offset;
pub(super) use frame::record;
pub(super) use submission::submit_and_present;
