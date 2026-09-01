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

    fn update(&self, index: usize, update: impl FnOnce(&mut UiChatWindow)) {
        if let Some(window) = self.windows.borrow_mut().get_mut(index.wrapping_sub(1)) {
            update(window);
        }
    }
}

pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    state: UiChatWindowState,
) -> mlua::Result<()> {
    let info = state.clone();
    globals.raw_set(
        "GetChatWindowInfo",
        lua.create_function(move |lua, value: Value| {
            let index = chat_window_index(value)?;
            let Some(window) = info.window(index) else {
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
    register_mutators(lua, globals, state)?;
    Ok(())
}

fn register_mutators(lua: &Lua, globals: &Table, state: UiChatWindowState) -> mlua::Result<()> {
    let names = state.clone();
    globals.raw_set(
        "SetChatWindowName",
        lua.create_function(move |lua, (index, name): (Value, Option<Value>)| {
            let index = chat_window_index_for(index, "SetChatWindowName(index, \"name\")")?;
            let name = name
                .map(|value| lua.coerce_string(value))
                .transpose()?
                .flatten()
                .map_or_else(String::new, |name| name.to_string_lossy());
            names.update(index, |window| window.name = name);
            Ok(())
        })?,
    )?;
    let sizes = state.clone();
    globals.raw_set(
        "SetChatWindowSize",
        lua.create_function(move |_, (index, size): (Value, Value)| {
            let index = chat_window_index_for(index, "SetChatWindowSize(index, size)")?;
            let size = lua_number(size, "SetChatWindowSize(index, size)")?.round() as i64;
            if size > 0 {
                sizes.update(index, |window| window.font_size = size as u32);
            }
            Ok(())
        })?,
    )?;
    let colors = state.clone();
    globals.raw_set(
        "SetChatWindowColor",
        lua.create_function(
            move |_, (index, red, green, blue): (Value, Value, Value, Value)| {
                let usage = "SetChatWindowColor(index, r, g, b)";
                let index = chat_window_index_for(index, usage)?;
                let color = (
                    quantized_color(lua_number(red, usage)?),
                    quantized_color(lua_number(green, usage)?),
                    quantized_color(lua_number(blue, usage)?),
                );
                colors.update(index, |window| {
                    window.color.0 = color.0;
                    window.color.1 = color.1;
                    window.color.2 = color.2;
                });
                Ok(())
            },
        )?,
    )?;
    let alphas = state.clone();
    globals.raw_set(
        "SetChatWindowAlpha",
        lua.create_function(move |_, (index, alpha): (Value, Value)| {
            let usage = "SetChatWindowAlpha(index, alpha)";
            let index = chat_window_index_for(index, usage)?;
            let alpha = quantized_color(lua_number(alpha, usage)?);
            alphas.update(index, |window| window.color.3 = alpha);
            Ok(())
        })?,
    )?;
    let locked = state.clone();
    globals.raw_set(
        "SetChatWindowLocked",
        lua.create_function(move |_, (index, value): (Value, Option<bool>)| {
            let index = chat_window_index_for(index, "SetChatWindowLocked(index, locked)")?;
            locked.update(index, |window| window.locked = value.unwrap_or(false));
            Ok(())
        })?,
    )?;
    let uninteractable = state.clone();
    globals.raw_set(
        "SetChatWindowUninteractable",
        lua.create_function(move |_, (index, value): (Value, Option<bool>)| {
            let index =
                chat_window_index_for(index, "SetChatWindowUninteractable(index, uninteractable)")?;
            uninteractable.update(index, |window| {
                window.uninteractable = value.unwrap_or(false);
            });
            Ok(())
        })?,
    )?;
    let docked = state.clone();
    globals.raw_set(
        "SetChatWindowDocked",
        lua.create_function(move |_, (index, position): (Value, Option<Value>)| {
            let index = chat_window_index_for(index, "SetChatWindowDocked(index, docked)")?;
            let position = position
                .map(|value| lua_number(value, "SetChatWindowDocked(index, docked)"))
                .transpose()?
                .map(|value| value.round() as u32)
                .filter(|position| *position != 0);
            docked.update(index, |window| window.dock_position = position);
            Ok(())
        })?,
    )?;
    globals.raw_set(
        "SetChatWindowShown",
        lua.create_function(move |_, (index, value): (Value, Option<bool>)| {
            let index = chat_window_index_for(index, "SetChatWindowShown(index, shown)")?;
            state.update(index, |window| window.shown = value.unwrap_or(true));
            Ok(())
        })?,
    )
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
    Ok(number.round() as usize)
}

fn chat_window_index_for(value: Value, usage: &'static str) -> mlua::Result<usize> {
    chat_window_index(value).map_err(|_| mlua::Error::runtime(format!("Usage: {usage}")))
}

fn lua_number(value: Value, usage: &'static str) -> mlua::Result<f64> {
    let number = match value {
        Value::Integer(number) => number as f64,
        Value::Number(number) => number,
        Value::String(number) => number
            .to_str()?
            .parse::<f64>()
            .map_err(|_| mlua::Error::runtime(format!("Usage: {usage}")))?,
        _ => return Err(mlua::Error::runtime(format!("Usage: {usage}"))),
    };
    number
        .is_finite()
        .then_some(number)
        .ok_or_else(|| mlua::Error::runtime(format!("Usage: {usage}")))
}

fn quantized_color(value: f64) -> f64 {
    f64::from((value * 255.0).round() as i32 as u8) / 255.0
}

fn usage_error() -> mlua::Error {
    mlua::Error::runtime("Usage: GetChatWindowInfo(index)")
}
