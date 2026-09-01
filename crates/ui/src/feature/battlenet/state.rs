//! Retained availability and connection state for the build-12340 Battle.net UI.

use std::rc::Rc;

/// Shared capability state read by the synchronous Battle.net Lua predicates.
#[derive(Clone)]
pub(crate) struct UiBattleNetState {
    inner: Rc<UiBattleNetStateInner>,
}

struct UiBattleNetStateInner {
    features_enabled: bool,
    connected: bool,
}

impl UiBattleNetState {
    /// Creates the state advertised by the attached platform service.
    pub(crate) fn new(features_enabled: bool) -> Self {
        Self {
            inner: Rc::new(UiBattleNetStateInner {
                features_enabled,
                connected: false,
            }),
        }
    }

    /// Reports whether this installation has an attached Battle.net service.
    pub(crate) fn features_enabled(&self) -> bool {
        self.inner.features_enabled
    }

    /// Reports whether the enabled service currently has a live connection.
    pub(crate) fn connected(&self) -> bool {
        self.inner.features_enabled && self.inner.connected
    }
}
