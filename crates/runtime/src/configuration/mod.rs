//! Startup configuration and persistent client preference ownership.
//!
//! The boundary follows `Profile.cpp`, `Status.cpp`, and the console-variable
//! family. Parsing failures remain explicit and do not substitute guessed
//! defaults unless stock behavior documents that default.

mod login;
mod profile;
mod status;
mod window;

pub use login::LoginConfiguration;
pub use profile::RuntimeConfiguration;
pub use status::ConfigurationError;
pub use window::{WindowConfiguration, WindowMode};
