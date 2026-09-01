//! Retained saved chat-window presentation state.

use std::cell::RefCell;
use std::rc::Rc;

use mlua::{Lua, MultiValue, Table, Value};

/// Number of persistent chat windows in build 12340.
pub const UI_CHAT_WINDOW_COUNT: usize = 10;

/// Script-visible settings for one persistent chat window.
#[derive(Clone, Debug, PartialEq)]
pub struct UiChatWindow {
    name: String,
    font_size: u32,
    color: (f64, f64, f64, f64),
    shown: bool,
    locked: bool,
    dock_position: Option<u32>,
    uninteractable: bool,
}

impl UiChatWindow {
    fn stock_initial(index: usize) -> Self {
        Self {
            name: String::new(),
            font_size: 0,
            color: (0.0, 0.0, 0.0, 40.0 / 255.0),
            shown: index <= 2,
            locked: true,
            dock_position: (index <= 2).then_some(index as u32),
            uninteractable: false,
        }
    }

    /// Returns the saved tab name. It is empty before chat-cache hydration.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the saved integer font size.
    #[must_use]
    pub const fn font_size(&self) -> u32 {
        self.font_size
    }

    /// Returns red, green, blue, and alpha components in script order.
    #[must_use]
    pub const fn color(&self) -> (f64, f64, f64, f64) {
        self.color
    }

    /// Returns whether the persistent frame is shown.
    #[must_use]
    pub const fn shown(&self) -> bool {
        self.shown
    }

    /// Returns whether movement and resizing are locked.
    #[must_use]
    pub const fn locked(&self) -> bool {
        self.locked
    }

    /// Returns the one-based dock position, when docked.
    #[must_use]
    pub const fn dock_position(&self) -> Option<u32> {
        self.dock_position
    }

    /// Returns whether the frame ignores interaction.
    #[must_use]
    pub const fn uninteractable(&self) -> bool {
        self.uninteractable
    }
}

/// Shared client-owned image of the ten persistent chat windows.
#[derive(Clone, Debug)]
pub struct UiChatWindowState {
    windows: Rc<RefCell<[UiChatWindow; UI_CHAT_WINDOW_COUNT]>>,
}

impl Default for UiChatWindowState {
    fn default() -> Self {
        Self {
            windows: Rc::new(RefCell::new(std::array::from_fn(|index| {
                UiChatWindow::stock_initial(index + 1)
            }))),
        }
    }
}

impl UiChatWindowState {
    /// Creates the exact pre-chat-cache client window image.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a copy of one one-based persistent window.
    #[must_use]
    pub fn window(&self, index: usize) -> Option<UiChatWindow> {
        self.windows.borrow().get(index.wrapping_sub(1)).cloned()
    }
}

pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    state: UiChatWindowState,
) -> mlua::Result<()> {
    globals.raw_set(
        "GetChatWindowInfo",
        lua.create_function(move |lua, value: Value| {
            let index = chat_window_index(value)?;
            let Some(window) = state.window(index) else {
                return Ok(MultiValue::new());
            };
            let (red, green, blue, alpha) = window.color();
            Ok(MultiValue::from_vec(vec![
                Value::String(lua.create_string(window.name())?),
                Value::Number(f64::from(window.font_size())),
                Value::Number(red),
                Value::Number(green),
                Value::Number(blue),
                Value::Number(alpha),
                numeric_flag(window.shown()),
                numeric_flag(window.locked()),
                window
                    .dock_position()
                    .map_or(Value::Nil, |position| Value::Number(f64::from(position))),
                numeric_flag(window.uninteractable()),
            ]))
        })?,
    )?;
    Ok(())
}

fn numeric_flag(enabled: bool) -> Value {
    if enabled {
        Value::Number(1.0)
    } else {
        Value::Nil
    }
}

fn chat_window_index(value: Value) -> mlua::Result<usize> {
    let number = match value {
        Value::Integer(number) => number as f64,
        Value::Number(number) => number,
        Value::String(number) => number.to_str()?.parse::<f64>().map_err(|_| usage_error())?,
        _ => return Err(usage_error()),
    };
    if !number.is_finite() {
        return Err(usage_error());
    }
    Ok(number.trunc() as usize)
}

fn usage_error() -> mlua::Error {
    mlua::Error::runtime("Usage: GetChatWindowInfo(index)")
}
