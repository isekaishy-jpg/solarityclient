//! Ground-detail cache policy and ordered, dependency-driven source preparation.
mod preparation;
mod residency;
pub(super) use preparation::GroundDetailPreparation;
pub(super) use residency::GroundDetailAssetCache;
pub(in crate::application) use residency::ResidentGroundDetailTile;
