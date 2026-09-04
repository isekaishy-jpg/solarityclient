//! Stock voice-chat globals backed by the optional client voice service.

use std::cell::Cell;
use std::rc::Rc;

use mlua::{Lua, Table, Value};

/// Shared voice-service availability visible to FrameXML.
///
/// The client starts without sessions. A later voice transport can extend this
/// state without changing the disconnected values required during bootstrap.
#[derive(Clone, Debug, Default)]
pub struct UiVoiceChatState {
    disabled_by_client: Rc<Cell<bool>>,
    enabled: Rc<Cell<bool>>,
    active_channel: Rc<Cell<Option<u32>>>,
}

impl UiVoiceChatState {
    /// Creates the stock pre-session voice state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Reports whether the user has disabled the voice subsystem.
    #[must_use]
    pub fn is_disabled_by_client(&self) -> bool {
        self.disabled_by_client.get()
    }

    /// Applies the authoritative client voice-disable setting.
    pub fn set_disabled_by_client(&self, disabled: bool) {
        self.disabled_by_client.set(disabled);
    }

    /// Reports whether a configured voice service is enabled.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled.get()
    }

    /// Applies the authoritative voice-service enabled state.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.set(enabled);
    }

    /// Replaces the display-row identifier of the active voice channel.
    pub fn set_active_channel(&self, channel: Option<u32>) {
        self.active_channel.set(channel);
    }
}

pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    state: UiVoiceChatState,
) -> mlua::Result<()> {
    // The disconnected session arena is genuinely empty. These are the exact
    // native result shapes observed before a voice transport publishes data.
    globals.raw_set(
        "GetNumVoiceSessions",
        lua.create_function(|_, ()| Ok(0_u32))?,
    )?;
    globals.raw_set(
        "GetVoiceSessionInfo",
        lua.create_function(|_, _: Value| Ok(Value::Nil))?,
    )?;
    globals.raw_set(
        "GetVoiceCurrentSessionID",
        lua.create_function(|_, ()| Ok(Value::Nil))?,
    )?;
    let active_channel = state.clone();
    globals.raw_set(
        "GetActiveVoiceChannel",
        lua.create_function(move |_, ()| Ok(active_channel.active_channel.get()))?,
    )?;
    let disabled = state.clone();
    globals.raw_set(
        "VoiceIsDisabledByClient",
        lua.create_function(move |_, ()| {
            Ok(disabled
                .is_disabled_by_client()
                .then_some(Value::Number(1.0)))
        })?,
    )?;
    globals.raw_set(
        "IsVoiceChatEnabled",
        lua.create_function(move |_, ()| Ok(state.is_enabled().then_some(Value::Number(1.0))))?,
    )?;
    globals.raw_set(
        "IsVoiceChatAllowedByServer",
        lua.create_function(|_, ()| Ok(false))?,
    )
}
