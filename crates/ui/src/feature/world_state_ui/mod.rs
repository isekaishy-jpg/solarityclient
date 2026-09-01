//! Always-up world-PvP and battleground indicator state.

use std::cell::RefCell;
use std::rc::Rc;

use mlua::{Lua, MultiValue, Table, Value};

/// One ordered native world-state indicator row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiWorldStateIndicator {
    ui_type: u32,
    state: u32,
    text: String,
    icon: String,
    dynamic_icon: String,
    tooltip: String,
    dynamic_tooltip: String,
    extended_ui: String,
    extended_states: [u32; 3],
}

impl UiWorldStateIndicator {
    /// Creates one ordinary always-up row without icon or extended UI data.
    #[must_use]
    pub fn new(ui_type: u32, state: u32, text: impl Into<String>) -> Self {
        Self {
            ui_type,
            state,
            text: text.into(),
            icon: String::new(),
            dynamic_icon: String::new(),
            tooltip: String::new(),
            dynamic_tooltip: String::new(),
            extended_ui: String::new(),
            extended_states: [0; 3],
        }
    }

    /// Adds the static and state-changing texture paths.
    #[must_use]
    pub fn with_icons(mut self, icon: impl Into<String>, dynamic_icon: impl Into<String>) -> Self {
        self.icon = icon.into();
        self.dynamic_icon = dynamic_icon.into();
        self
    }

    /// Adds the static-icon and dynamic-icon tooltip text.
    #[must_use]
    pub fn with_tooltips(
        mut self,
        tooltip: impl Into<String>,
        dynamic_tooltip: impl Into<String>,
    ) -> Self {
        self.tooltip = tooltip.into();
        self.dynamic_tooltip = dynamic_tooltip.into();
        self
    }

    /// Selects an authored extended-UI handler and its three state words.
    #[must_use]
    pub fn with_extended_ui(mut self, name: impl Into<String>, states: [u32; 3]) -> Self {
        self.extended_ui = name.into();
        self.extended_states = states;
        self
    }
}

/// Shared ordered world-state indicator list.
#[derive(Clone, Debug, Default)]
pub struct UiWorldStateUiState {
    indicators: Rc<RefCell<Vec<UiWorldStateIndicator>>>,
}

impl UiWorldStateUiState {
    /// Creates the no-indicator state used outside an active world objective.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the complete server-ordered indicator list.
    pub fn replace(&self, indicators: Vec<UiWorldStateIndicator>) {
        *self.indicators.borrow_mut() = indicators;
    }

    fn indicator(&self, index: usize) -> Option<UiWorldStateIndicator> {
        index
            .checked_sub(1)
            .and_then(|index| self.indicators.borrow().get(index).cloned())
    }
}

/// Registers the complete world-state indicator query family.
pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    state: UiWorldStateUiState,
) -> mlua::Result<()> {
    let info_state = state.clone();
    globals.raw_set(
        "GetNumWorldStateUI",
        lua.create_function(move |_, ()| Ok(state.indicators.borrow().len()))?,
    )?;
    globals.raw_set(
        "GetWorldStateUIInfo",
        lua.create_function(move |lua, index: usize| {
            let Some(indicator) = info_state.indicator(index) else {
                return Ok(MultiValue::new());
            };
            Ok(MultiValue::from_vec(vec![
                Value::Integer(i64::from(indicator.ui_type)),
                Value::Integer(i64::from(indicator.state)),
                Value::String(lua.create_string(indicator.text)?),
                Value::String(lua.create_string(indicator.icon)?),
                Value::String(lua.create_string(indicator.dynamic_icon)?),
                Value::String(lua.create_string(indicator.tooltip)?),
                Value::String(lua.create_string(indicator.dynamic_tooltip)?),
                Value::String(lua.create_string(indicator.extended_ui)?),
                Value::Integer(i64::from(indicator.extended_states[0])),
                Value::Integer(i64::from(indicator.extended_states[1])),
                Value::Integer(i64::from(indicator.extended_states[2])),
            ]))
        })?,
    )
}
