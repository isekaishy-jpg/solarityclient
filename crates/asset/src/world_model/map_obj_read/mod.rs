//! Strict WMO decoding separates root/group steps from whole-generation publication.

mod group;
mod layout;
mod preparation;
mod root;

pub(crate) use preparation::WorldModelPreparation;

/// Exact WMO format revision read by the pinned build-12340 client.
const BUILD_12340_WMO_VERSION: u32 = 17;
