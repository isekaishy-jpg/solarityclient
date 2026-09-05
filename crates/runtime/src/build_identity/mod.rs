//! Solarity product identity, independent of the build-12340 wire contract.

use std::fmt;

/// Immutable version and source identity embedded in one executable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClientBuild {
    version: &'static str,
    number: u32,
    revision: &'static str,
    dirty: bool,
}

impl ClientBuild {
    /// Joins fields validated by the Cargo build script.
    const fn new(version: &'static str, number: u32, revision: &'static str, dirty: bool) -> Self {
        Self {
            version,
            number,
            revision,
            dirty,
        }
    }

    /// Returns the product version with its a/b/rc/s release suffix.
    pub const fn version(self) -> &'static str {
        self.version
    }

    /// Returns the package sequence; zero predates the first numbered package.
    pub const fn number(self) -> u32 {
        self.number
    }

    /// Returns the source checkout's HEAD captured at compilation.
    pub const fn revision(self) -> &'static str {
        self.revision
    }

    /// Reports whether the compiling checkout had uncommitted changes.
    pub const fn is_dirty(self) -> bool {
        self.dirty
    }
}

impl fmt::Display for ClientBuild {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Solarity {} (Build {:06})",
            self.version, self.number
        )
    }
}

include!(concat!(env!("OUT_DIR"), "/client_build.rs"));
