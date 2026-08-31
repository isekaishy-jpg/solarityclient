//! Key bindings, modified clicks, macros, and script-callable binding vocabulary.

mod error;
mod types;
mod ui_bindings;
mod ui_macro_options;
mod ui_macros;

pub use error::UiBindingError;
pub use types::{UiBindingDefinition, UiBindingDocument, UiModifiedClickDefinition};
pub use ui_bindings::UiBindingCatalog;
