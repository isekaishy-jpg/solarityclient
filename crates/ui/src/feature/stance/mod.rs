//! Shapeshift forms and possession-bar visibility for the controlled unit.

use std::cell::RefCell;
use std::rc::Rc;

use mlua::{Lua, MultiValue, Table, Value};

/// One player shapeshift form button.
#[derive(Clone, Debug, PartialEq)]
pub struct UiShapeshiftForm {
    texture: String,
    name: String,
    active: bool,
    castable: bool,
    cooldown_start: f64,
    cooldown_duration: f64,
    cooldown_enabled: bool,
}

/// One of the two possession-bar actions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiPossessAction {
    texture: String,
    name: String,
    enabled: bool,
}

impl UiPossessAction {
    /// Creates one enabled possession action.
    #[must_use]
    pub fn new(texture: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            texture: texture.into(),
            name: name.into(),
            enabled: true,
        }
    }
}

impl UiShapeshiftForm {
    /// Creates one castable inactive form without a cooldown.
    #[must_use]
    pub fn new(texture: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            texture: texture.into(),
            name: name.into(),
            active: false,
            castable: true,
            cooldown_start: 0.0,
            cooldown_duration: 0.0,
            cooldown_enabled: false,
        }
    }
}

#[derive(Debug, Default)]
struct UiStanceInner {
    forms: Vec<UiShapeshiftForm>,
    possess_bar_visible: bool,
    possess_actions: Vec<UiPossessAction>,
}

/// Shared shapeshift and possession capability image.
#[derive(Clone, Debug, Default)]
pub struct UiStanceState {
    inner: Rc<RefCell<UiStanceInner>>,
}

impl UiStanceState {
    /// Creates the ordinary no-form, no-possession state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the ordered shapeshift-form list.
    pub fn replace_forms(&self, forms: Vec<UiShapeshiftForm>) {
        self.inner.borrow_mut().forms = forms;
    }

    /// Replaces whether a possession bar controls the active unit.
    pub fn set_possess_bar_visible(&self, visible: bool) {
        self.inner.borrow_mut().possess_bar_visible = visible;
    }

    /// Replaces the ordered possession-bar actions.
    pub fn replace_possess_actions(&self, actions: Vec<UiPossessAction>) {
        self.inner.borrow_mut().possess_actions = actions;
    }

    fn form(&self, index: usize) -> Option<UiShapeshiftForm> {
        index
            .checked_sub(1)
            .and_then(|index| self.inner.borrow().forms.get(index).cloned())
    }
}

/// Registers shapeshift and possession queries consumed by bonus bars.
pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    state: UiStanceState,
) -> mlua::Result<()> {
    let info_state = state.clone();
    let cooldown_state = state.clone();
    let possess_state = state.clone();
    let possess_info_state = state.clone();
    globals.raw_set(
        "GetNumShapeshiftForms",
        lua.create_function(move |_, ()| Ok(state.inner.borrow().forms.len()))?,
    )?;
    globals.raw_set(
        "GetShapeshiftFormInfo",
        lua.create_function(move |lua, value: Value| {
            let form = info_state.form(required_index(lua, value)?);
            let Some(form) = form else {
                return Ok(MultiValue::new());
            };
            Ok(MultiValue::from_vec(vec![
                Value::String(lua.create_string(form.texture)?),
                Value::String(lua.create_string(form.name)?),
                flag(form.active),
                flag(form.castable),
            ]))
        })?,
    )?;
    globals.raw_set(
        "GetShapeshiftFormCooldown",
        lua.create_function(move |lua, value: Value| {
            Ok(cooldown_state
                .form(required_index(lua, value)?)
                .map_or((0.0, 0.0, 0_u8), |form| {
                    (
                        form.cooldown_start,
                        form.cooldown_duration,
                        u8::from(form.cooldown_enabled),
                    )
                }))
        })?,
    )?;
    globals.raw_set(
        "IsPossessBarVisible",
        lua.create_function(move |_, ()| {
            Ok(possess_state
                .inner
                .borrow()
                .possess_bar_visible
                .then_some(1_u8))
        })?,
    )?;
    globals.raw_set(
        "GetPossessInfo",
        lua.create_function(move |lua, value: Value| {
            let index = required_index(lua, value)?;
            let action = index.checked_sub(1).and_then(|index| {
                possess_info_state
                    .inner
                    .borrow()
                    .possess_actions
                    .get(index)
                    .cloned()
            });
            let Some(action) = action else {
                return Ok(MultiValue::from_vec(vec![Value::Nil; 3]));
            };
            Ok(MultiValue::from_vec(vec![
                Value::String(lua.create_string(action.texture)?),
                Value::String(lua.create_string(action.name)?),
                flag(action.enabled),
            ]))
        })?,
    )
}

fn required_index(lua: &Lua, value: Value) -> mlua::Result<usize> {
    let number = lua
        .coerce_number(value)?
        .ok_or_else(|| mlua::Error::runtime("Usage: shapeshift API requires index"))?;
    if !number.is_finite() || number < 1.0 || number > usize::MAX as f64 {
        return Ok(0);
    }
    Ok(number.round() as usize)
}

fn flag(value: bool) -> Value {
    if value {
        Value::Number(1.0)
    } else {
        Value::Nil
    }
}
