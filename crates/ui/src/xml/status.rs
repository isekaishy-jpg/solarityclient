//! Stable failures at the stock user-interface loading boundary.

use solarity_asset::{AssetError, AssetPath};
use thiserror::Error;

/// A failure while loading stock GlueXML or FrameXML content.
#[derive(Debug, Error)]
pub enum UiLoadError {
    /// An archive lookup or read failed.
    #[error(transparent)]
    Asset(#[from] AssetError),
    /// A UI text asset was not valid UTF-8.
    #[error("failed to decode UI text asset {path}: {message}")]
    TextEncoding {
        /// The normalized archive path.
        path: AssetPath,
        /// Decoder context.
        message: String,
    },
    /// A manifest entry did not name a stock XML or Lua source file.
    #[error("invalid UI manifest entry in {path} at line {line}: {value}")]
    ManifestEntry {
        /// The normalized manifest path.
        path: AssetPath,
        /// The one-based source line.
        line: usize,
        /// The rejected entry text.
        value: String,
    },
    /// An XML source file was malformed.
    #[error("failed to parse UI XML asset {path}: {message}")]
    Xml {
        /// The normalized XML path.
        path: AssetPath,
        /// Parser or structural context.
        message: String,
    },
    /// A Lua source file did not compile as Lua 5.1.
    #[error("failed to compile UI Lua asset {path}: {message}")]
    Lua {
        /// The normalized Lua path.
        path: AssetPath,
        /// Compiler context.
        message: String,
    },
    /// An XML include or external-script directive was invalid.
    #[error("invalid UI load directive in {path}: {message}")]
    Directive {
        /// The XML source containing the directive.
        path: AssetPath,
        /// Path, extension, or include-cycle context.
        message: String,
    },
}
