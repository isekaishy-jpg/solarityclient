//! Ordered process-lifetime actions emitted by built-in UI Lua.

use std::collections::VecDeque;

/// One process action requested by stock GlueXML or FrameXML.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiProcessAction {
    /// End the client lifetime through the composition root's orderly shutdown path.
    Quit,
    /// Save the next completed framebuffer through the application renderer.
    Screenshot,
}

/// Main-thread mailbox shared by Lua bindings and the application owner.
#[derive(Default)]
pub(crate) struct UiProcessBridge {
    actions: VecDeque<UiProcessAction>,
}

impl UiProcessBridge {
    /// Appends one native process action in Lua execution order.
    pub(crate) fn push(&mut self, action: UiProcessAction) {
        self.actions.push_back(action);
    }

    /// Takes the oldest action for application-level orchestration.
    pub(crate) fn take(&mut self) -> Option<UiProcessAction> {
        self.actions.pop_front()
    }
}
