//! Realm authentication, SRP proof exchange, and authenticated session keys.
//!
//! The stock `Grunt.cpp` and `GruntLogin.cpp` family establishes the legacy
//! 3.3.5a login path used by the target server. `wow_srp` and
//! `wow_login_messages` provide the wire primitives; this module owns the
//! client state machine.

mod battlenet_login;
mod error;
mod grunt;
mod grunt_login;
mod version_integrity;

pub use error::{LoginError, LoginFailure, LoginStage};
pub use grunt::{GruntCredentials, GruntIntegrity, GruntLoginOptions, LoginLocale};
pub use grunt_login::{AuthenticatedGrunt, GruntLogin, WorldIdentity, WorldSessionKey};
pub use version_integrity::Build12340WindowsIntegrity;
