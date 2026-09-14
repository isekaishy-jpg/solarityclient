//! Shared native Unit and GameObject spatial registration.

mod cache;
mod query;
mod receivers;
mod types;

pub(super) use cache::UnitRegistrationCache;
pub(in crate::application) use types::UnitWorldModelLocation;
pub use types::{
    RuntimeMovementReference, RuntimeMovementRegistrationError, RuntimeMovementRegistrationQuery,
};
