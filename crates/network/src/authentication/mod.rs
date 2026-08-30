//! Realm authentication, SRP proof exchange, and authenticated session keys.
//!
//! The stock `Grunt.cpp` and `GruntLogin.cpp` family establishes the legacy
//! 3.3.5a login path used by the target server. `wow_srp` and
//! `wow_login_messages` provide the wire primitives; this module owns the
//! client state machine.

mod battlenet_login;
mod grunt;
mod grunt_login;
