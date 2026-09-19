//! Native shadow admission and independent packet construction have separate owners.

mod admission;
mod packets;

pub(super) use admission::{admits_root, environment_maps};
pub(super) use packets::{ShadowInput, append_packets};
