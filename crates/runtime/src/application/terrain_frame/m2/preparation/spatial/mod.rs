//! Owned static admission leaves callback-dependent models in native scene order.

mod batch;
mod input;
mod job;

pub(in crate::application::terrain_frame::m2) use batch::SpatialBatch;
pub(in crate::application::terrain_frame::m2) use input::{SpatialView, StaticAdmissionInput};

#[cfg(test)]
#[path = "../../../../../../tests/application/m2_spatial_batch.rs"]
mod tests;
