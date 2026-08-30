//! Stable failures while preparing stock XML script handlers.

use solarity_asset::AssetPath;
use thiserror::Error;

/// A failure while converting XML callbacks into Lua 5.1 functions.
#[derive(Debug, Error)]
pub enum UiScriptError {
    /// A handler is not part of the selected stock widget's callback table.
    #[error("unsupported UI script handler {handler} on {object} in {path}")]
    Handler {
        /// XML source containing the handler.
        path: AssetPath,
        /// Expanded object name or an unnamed-object marker.
        object: String,
        /// XML handler element name.
        handler: String,
    },
    /// A handler body did not compile with its stock callback signature.
    #[error("failed to compile UI handler {handler} on {object} in {path}: {message}")]
    Lua {
        /// XML source containing the handler.
        path: AssetPath,
        /// Expanded object name or an unnamed-object marker.
        object: String,
        /// Stock callback name.
        handler: &'static str,
        /// Lua 5.1 compiler context.
        message: String,
    },
    /// A compact function index exceeded its representation.
    #[error("could not construct UI script plan: {message}")]
    Plan {
        /// Index or arena mismatch context.
        message: String,
    },
}
