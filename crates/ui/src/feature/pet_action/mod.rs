//! Ten-slot pet action bar projected from controlled-unit state.

use std::cell::RefCell;
use std::rc::Rc;

use mlua::{Lua, MultiValue, Table, Value};

/// Number of pet action slots in build 12340.
pub const UI_PET_ACTION_SLOT_COUNT: usize = 10;

/// One occupied pet action slot.
#[derive(Clone, Debug, PartialEq)]
pub struct UiPetAction {
    name: String,
    subtext: Option<String>,
    texture: String,
    token: bool,
    active: bool,
    auto_cast_allowed: bool,
    auto_cast_enabled: bool,
    usable: bool,
    attack: bool,
    cooldown_start: f64,
    cooldown_duration: f64,
    cooldown_enabled: bool,
}

impl UiPetAction {
    /// Creates one ordinary usable pet spell without autocast or cooldown.
    #[must_use]
    pub fn new(name: impl Into<String>, texture: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            subtext: None,
            texture: texture.into(),
            token: false,
            active: false,
            auto_cast_allowed: false,
            auto_cast_enabled: false,
            usable: true,
            attack: false,
            cooldown_start: 0.0,
            cooldown_duration: 0.0,
            cooldown_enabled: false,
        }
    }
}

#[derive(Debug)]
struct UiPetActionInner {
    has_action_bar: bool,
    slots: [Option<UiPetAction>; UI_PET_ACTION_SLOT_COUNT],
}

impl Default for UiPetActionInner {
    fn default() -> Self {
        Self {
            has_action_bar: false,
            slots: std::array::from_fn(|_| None),
        }
    }
}

/// Shared controlled-unit pet action-bar image.
#[derive(Clone, Debug, Default)]
pub struct UiPetActionState {
    inner: Rc<RefCell<UiPetActionInner>>,
}

impl UiPetActionState {
    /// Creates the no-pet action-bar image.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the complete pet bar and whether the controlled unit exposes it.
    pub fn replace(
        &self,
        has_action_bar: bool,
        slots: [Option<UiPetAction>; UI_PET_ACTION_SLOT_COUNT],
    ) {
        let mut inner = self.inner.borrow_mut();
        inner.has_action_bar = has_action_bar;
        inner.slots = slots;
    }

    fn action(&self, index: usize) -> Option<UiPetAction> {
        index
            .checked_sub(1)
            .and_then(|index| self.inner.borrow().slots.get(index).cloned())
            .flatten()
    }
}

/// Registers pet-action queries consumed during action-bar initialization.
pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    state: UiPetActionState,
) -> mlua::Result<()> {
    let info_state = state.clone();
    let usable_state = state.clone();
    let attack_state = state.clone();
    let cooldown_state = state.clone();
    globals.raw_set(
        "PetHasActionBar",
        lua.create_function(move |_, ()| {
            Ok(state
                .inner
                .borrow()
                .has_action_bar
                .then_some(Value::Number(1.0)))
        })?,
    )?;
    globals.raw_set(
        "GetPetActionInfo",
        lua.create_function(move |lua, value: Value| {
            pet_action_values(lua, info_state.action(required_slot(lua, value)?))
        })?,
    )?;
    globals.raw_set(
        "GetPetActionSlotUsable",
        lua.create_function(move |lua, value: Value| {
            Ok(usable_state
                .action(required_slot(lua, value)?)
                .filter(|action| action.usable)
                .map(|_| 1_u8))
        })?,
    )?;
    globals.raw_set(
        "IsPetAttackAction",
        lua.create_function(move |lua, value: Value| {
            Ok(attack_state
                .action(required_slot(lua, value)?)
                .filter(|action| action.attack)
                .map(|_| 1_u8))
        })?,
    )?;
    globals.raw_set(
        "GetPetActionCooldown",
        lua.create_function(move |lua, value: Value| {
            let action = cooldown_state.action(required_slot(lua, value)?);
            Ok(action.map_or((0.0, 0.0, 0_u8), |action| {
                (
                    action.cooldown_start,
                    action.cooldown_duration,
                    u8::from(action.cooldown_enabled),
                )
            }))
        })?,
    )
}

fn required_slot(lua: &Lua, value: Value) -> mlua::Result<usize> {
    let number = lua
        .coerce_number(value)?
        .ok_or_else(|| mlua::Error::runtime("Usage: pet action API requires index"))?;
    if !number.is_finite() || number < 1.0 || number > usize::MAX as f64 {
        return Ok(0);
    }
    Ok(number.round() as usize)
}

fn pet_action_values(lua: &Lua, action: Option<UiPetAction>) -> mlua::Result<MultiValue> {
    let Some(action) = action else {
        return Ok(MultiValue::from_vec(vec![Value::Nil; 7]));
    };
    Ok(MultiValue::from_vec(vec![
        Value::String(lua.create_string(action.name)?),
        action
            .subtext
            .map(|subtext| lua.create_string(subtext).map(Value::String))
            .transpose()?
            .unwrap_or(Value::Nil),
        Value::String(lua.create_string(action.texture)?),
        flag(action.token),
        flag(action.active),
        flag(action.auto_cast_allowed),
        flag(action.auto_cast_enabled),
    ]))
}

fn flag(value: bool) -> Value {
    if value {
        Value::Number(1.0)
    } else {
        Value::Nil
    }
}
