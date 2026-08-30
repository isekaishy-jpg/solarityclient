//! Startup configuration and persistent client preference ownership.
//!
//! The boundary follows `Profile.cpp`, `Status.cpp`, and the console-variable
//! family. Parsing failures remain explicit and do not substitute guessed
//! defaults unless stock behavior documents that default.

mod profile;
mod status;

pub use profile::RuntimeConfiguration;
pub use status::ConfigurationError;
