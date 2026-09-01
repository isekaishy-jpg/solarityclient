//! Case-insensitive CVar storage shared by native and Lua-facing UI paths.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::definitions::{STOCK_INITIAL_DEFINITIONS, UiCVarDefinition};

/// Mutable value paired with immutable native metadata.
#[derive(Clone, Debug, PartialEq)]
struct UiCVar {
    value: String,
    default: &'static str,
    minimum: Option<f64>,
    maximum: Option<f64>,
    read_only: bool,
}

/// Shared CVar state queried and mutated by built-in UI scripts.
#[derive(Clone, Debug, Default, PartialEq)]
pub(in crate::script::simple_script) struct UiCVarRegistry {
    entries: Rc<RefCell<HashMap<String, UiCVar>>>,
}

impl UiCVarRegistry {
    /// Registers every evidenced native default needed by built-in UI code.
    pub(in crate::script::simple_script) fn stock_initial() -> Self {
        let registry = Self::default();
        for definition in STOCK_INITIAL_DEFINITIONS {
            registry.register(*definition);
        }
        registry
    }

    /// Returns a copy of the current string value for one known CVar.
    pub(in crate::script::simple_script) fn get(&self, name: &str) -> Option<String> {
        self.entries
            .borrow()
            .get(&canonical_name(name))
            .map(|entry| entry.value.clone())
    }

    /// Returns a copy of the native default for one known CVar.
    pub(in crate::script::simple_script) fn default_value(&self, name: &str) -> Option<String> {
        self.entries
            .borrow()
            .get(&canonical_name(name))
            .map(|entry| entry.default.to_owned())
    }

    /// Returns the native numeric minimum when the CVar publishes one.
    pub(in crate::script::simple_script) fn minimum(&self, name: &str) -> Option<f64> {
        self.entries
            .borrow()
            .get(&canonical_name(name))
            .and_then(|entry| entry.minimum)
    }

    /// Returns the native numeric maximum when the CVar publishes one.
    pub(in crate::script::simple_script) fn maximum(&self, name: &str) -> Option<f64> {
        self.entries
            .borrow()
            .get(&canonical_name(name))
            .and_then(|entry| entry.maximum)
    }

    /// Applies stock's numeric truth test to a registered CVar.
    pub(in crate::script::simple_script) fn boolean(&self, name: &str) -> bool {
        self.get(name)
            .and_then(|value| value.parse::<f64>().ok())
            .is_some_and(|value| value != 0.0)
    }

    /// Replaces one mutable CVar while preserving its native metadata.
    pub(in crate::script::simple_script) fn set(
        &self,
        name: &str,
        value: String,
    ) -> Result<(), UiCVarSetError> {
        let mut entries = self.entries.borrow_mut();
        let Some(entry) = entries.get_mut(&canonical_name(name)) else {
            return Err(UiCVarSetError::Missing);
        };
        if entry.read_only {
            return Err(UiCVarSetError::ReadOnly);
        }
        entry.value = value;
        Ok(())
    }

    /// Installs one definition using its canonical case-insensitive identity.
    fn register(&self, definition: UiCVarDefinition) {
        self.entries.borrow_mut().insert(
            canonical_name(definition.name),
            UiCVar {
                value: definition.default.to_owned(),
                default: definition.default,
                minimum: definition.minimum,
                maximum: definition.maximum,
                read_only: definition.read_only,
            },
        );
    }
}

/// Failure returned when Lua attempts to mutate an invalid CVar target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::script::simple_script) enum UiCVarSetError {
    /// No native definition registered the requested case-insensitive name.
    Missing,
    /// Native metadata prohibits mutation through the script API.
    ReadOnly,
}

/// Normalizes the stock case-insensitive CVar identity once at lookup.
fn canonical_name(name: &str) -> String {
    name.to_ascii_lowercase()
}
