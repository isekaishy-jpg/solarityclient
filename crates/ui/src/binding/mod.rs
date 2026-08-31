//! Key bindings, modified clicks, macros, and script-callable binding vocabulary.

mod assignments;
mod error;
mod key;
mod types;
mod ui_bindings;
mod ui_macro_options;
mod ui_macros;

pub use assignments::{
    UiBindingAction, UiBindingAssignment, UiBindingAssignments, UiBindingMode,
    UiModifiedClickAssignment,
};
pub use error::{UiBindingAssignmentError, UiBindingError};
pub use key::{UiBindingKey, UiModifiedClickChord};
pub use types::{
    UiBindingDefinition, UiBindingDocument, UiBindingPlatform, UiModifiedClickDefinition,
};
pub use ui_bindings::UiBindingCatalog;
