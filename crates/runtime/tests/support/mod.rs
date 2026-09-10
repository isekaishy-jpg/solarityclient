//! Shared encrypted loopback transport for crate-internal integration tests.

mod transfer_authentication;
mod transfer_world_server;

pub(crate) use transfer_world_server::authenticate as authenticate_test_world;
pub(crate) use transfer_world_server::{TestError, WorldServer};
