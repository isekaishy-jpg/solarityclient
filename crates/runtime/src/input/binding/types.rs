//! Borrowed output of the input-binding routing boundary.

use solarity_ui::{UiBindingAssignment, UiBindingDefinition};

/// Stock `keystate` supplied to a binding body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputBindingPhase {
    /// The physical control entered its active state.
    Down,
    /// A `runOnUp` control left its active state or focus was lost.
    Up,
}

impl InputBindingPhase {
    /// Returns the exact lowercase string accepted by stock binding Lua.
    #[must_use]
    pub const fn stock_keystate(self) -> &'static str {
        match self {
            Self::Down => "down",
            Self::Up => "up",
        }
    }
}

/// One resolved assignment transition ready for UI or secure-action dispatch.
#[derive(Clone, Copy, Debug)]
pub struct InputBindingInvocation<'a> {
    assignment: &'a UiBindingAssignment,
    definition: Option<&'a UiBindingDefinition>,
    phase: InputBindingPhase,
}

impl<'a> InputBindingInvocation<'a> {
    /// Constructs an invocation after action and platform validation.
    pub(super) const fn new(
        assignment: &'a UiBindingAssignment,
        definition: Option<&'a UiBindingDefinition>,
        phase: InputBindingPhase,
    ) -> Self {
        Self {
            assignment,
            definition,
            phase,
        }
    }

    /// Returns the effective key-to-action assignment.
    #[must_use]
    pub const fn assignment(self) -> &'a UiBindingAssignment {
        self.assignment
    }

    /// Returns the declared Lua command for named actions.
    ///
    /// Spell, item, macro, and click assignments are dynamic secure actions
    /// and therefore have no `Bindings.xml` declaration.
    #[must_use]
    pub const fn definition(self) -> Option<&'a UiBindingDefinition> {
        self.definition
    }

    /// Returns whether this invocation represents press or release.
    #[must_use]
    pub const fn phase(self) -> InputBindingPhase {
        self.phase
    }
}
