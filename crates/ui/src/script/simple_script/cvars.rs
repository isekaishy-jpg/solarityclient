//! Script-visible console-variable state and stock startup registrations.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Clone, Debug, Eq, PartialEq)]
struct UiCVar {
    value: String,
    default: String,
    read_only: bool,
}

/// Shared CVar state queried and mutated by GlueXML script functions.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct UiCVarRegistry {
    entries: Rc<RefCell<HashMap<String, UiCVar>>>,
}

impl UiCVarRegistry {
    /// Registers defaults recovered from the stock client startup path.
    pub(super) fn stock_initial() -> Self {
        let registry = Self::default();
        registry.register("accountName", "", false);
        registry.register("accountList", "", false);
        registry.register("showToolsUI", "-1", false);
        registry
    }

    pub(super) fn get(&self, name: &str) -> Option<String> {
        self.entries
            .borrow()
            .get(&canonical_name(name))
            .map(|entry| entry.value.clone())
    }

    pub(super) fn default_value(&self, name: &str) -> Option<String> {
        self.entries
            .borrow()
            .get(&canonical_name(name))
            .map(|entry| entry.default.clone())
    }

    pub(super) fn set(&self, name: &str, value: String) -> Result<(), UiCVarSetError> {
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

    fn register(&self, name: &str, default: &str, read_only: bool) {
        self.entries.borrow_mut().insert(
            canonical_name(name),
            UiCVar {
                value: default.to_owned(),
                default: default.to_owned(),
                read_only,
            },
        );
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum UiCVarSetError {
    Missing,
    ReadOnly,
}

fn canonical_name(name: &str) -> String {
    name.to_ascii_lowercase()
}
