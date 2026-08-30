//! Typed Wrath login and world packet encoding and decoding.
//!
//! This module adapts the pinned `wow_login_messages` and
//! `wow_world_messages` crates into stable Solarity types. Opcode dispatch and
//! malformed-packet behavior must remain build-12340 specific.

mod wow_svcs_client_services;
