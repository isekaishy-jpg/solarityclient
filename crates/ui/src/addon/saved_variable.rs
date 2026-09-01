//! Built-in UI saved-variable declarations collected before persistence.

use std::cell::RefCell;
use std::rc::Rc;

/// Shared ordered declaration registry for Blizzard-owned global variables.
#[derive(Clone, Debug, Default)]
pub struct UiSavedVariableState {
    account: Rc<RefCell<Vec<String>>>,
    character: Rc<RefCell<Vec<String>>>,
}

impl UiSavedVariableState {
    /// Creates an empty process declaration registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers one account-wide global variable name once.
    pub fn register_account(&self, name: impl Into<String>) {
        register_unique(&mut self.account.borrow_mut(), name.into());
    }

    /// Registers one character-specific global variable name once.
    pub fn register_character(&self, name: impl Into<String>) {
        register_unique(&mut self.character.borrow_mut(), name.into());
    }

    /// Returns account-wide names in first-registration order.
    #[must_use]
    pub fn account_names(&self) -> Vec<String> {
        self.account.borrow().clone()
    }

    /// Returns character-specific names in first-registration order.
    #[must_use]
    pub fn character_names(&self) -> Vec<String> {
        self.character.borrow().clone()
    }
}

fn register_unique(names: &mut Vec<String>, name: String) {
    if !names.iter().any(|registered| registered == &name) {
        names.push(name);
    }
}
