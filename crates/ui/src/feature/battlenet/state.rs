//! Retained availability and connection state for the build-12340 Battle.net UI.

use std::rc::Rc;

/// Conversation capacity pushed by build 12340's native Lua binding.
const MAX_CONVERSATION_PLAYERS: u8 = 12;

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

    /// Returns the stock conversation capacity only while the platform service
    /// is both available and connected.
    ///
    /// The native binding returns no Lua values when its service predicates
    /// fail, so a disabled installation must expose `nil` rather than zero.
    pub(crate) fn max_conversation_players(&self) -> Option<u8> {
        self.connected().then_some(MAX_CONVERSATION_PLAYERS)
    }
}
