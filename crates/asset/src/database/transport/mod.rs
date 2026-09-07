//! Exact build-12340 route, physics, and animation database records.

mod catalog;
mod rows;

pub use catalog::TransportCatalog;
pub use rows::{
    TaxiPathNode, TransportAnimationNode, TransportPhysicsRecord, TransportRotationNode,
};
