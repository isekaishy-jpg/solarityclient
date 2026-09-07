//! Remote command admission and simulation share the native contact integrator.

mod inbox;
mod service;
mod state;

pub(crate) use inbox::{baseline, receive, receive_path};
pub(in crate::application) use service::RuntimeRemoteMovement;
