//! Learned mount and critter collections projected to the stock companion UI.

use std::cell::RefCell;
use std::rc::Rc;

use mlua::{Lua, MultiValue, Table, Value};

/// Stock companion collection selected by FrameXML's uppercase type token.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiCompanionType {
    /// Non-combat vanity pets.
    Critter,
    /// Learned character mounts.
    Mount,
}

/// One learned companion row in server-provided collection order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiCompanion {
    creature_id: u32,
    name: Option<String>,
    spell_id: u32,
    icon: Option<String>,
    active: bool,
}

impl UiCompanion {
    /// Creates one complete learned companion projection.
    #[must_use]
    pub fn new(
        creature_id: u32,
        name: Option<String>,
        spell_id: u32,
        icon: Option<String>,
        active: bool,
    ) -> Self {
        Self {
            creature_id,
            name,
            spell_id,
            icon,
            active,
        }
    }

    /// Returns the creature display identity used by the model frame.
    #[must_use]
    pub const fn creature_id(&self) -> u32 {
        self.creature_id
    }

    /// Returns the localized creature name when its record resolves.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Returns the learned summon spell identity.
    #[must_use]
    pub const fn spell_id(&self) -> u32 {
        self.spell_id
    }

    /// Returns the spell icon texture when its record resolves.
    #[must_use]
    pub fn icon(&self) -> Option<&str> {
        self.icon.as_deref()
    }

    /// Reports whether this row represents the currently summoned companion.
    #[must_use]
    pub const fn active(&self) -> bool {
        self.active
    }
}

#[derive(Debug, Default)]
struct UiCompanionInner {
    critters: Vec<UiCompanion>,
    mounts: Vec<UiCompanion>,
}

/// Shared learned-companion collection consumed by `PetPaperDollFrame.lua`.
#[derive(Clone, Debug, Default)]
pub struct UiCompanionState {
    inner: Rc<RefCell<UiCompanionInner>>,
}

impl UiCompanionState {
    /// Creates the pre-collection state with no synthesized companions.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces one complete ordered collection after a companion update.
    pub fn replace(&self, companion_type: UiCompanionType, companions: Vec<UiCompanion>) {
        let mut inner = self.inner.borrow_mut();
        *collection_mut(&mut inner, companion_type) = companions;
    }

    /// Returns the number of learned rows in one collection.
    #[must_use]
    pub fn len(&self, companion_type: UiCompanionType) -> usize {
        collection(&self.inner.borrow(), companion_type).len()
    }

    /// Returns one one-based learned row, if present.
    #[must_use]
    pub fn companion(&self, companion_type: UiCompanionType, index: usize) -> Option<UiCompanion> {
        index.checked_sub(1).and_then(|index| {
            collection(&self.inner.borrow(), companion_type)
                .get(index)
                .cloned()
        })
    }
}

fn collection(inner: &UiCompanionInner, companion_type: UiCompanionType) -> &Vec<UiCompanion> {
    match companion_type {
        UiCompanionType::Critter => &inner.critters,
        UiCompanionType::Mount => &inner.mounts,
    }
}

fn collection_mut(
    inner: &mut UiCompanionInner,
    companion_type: UiCompanionType,
) -> &mut Vec<UiCompanion> {
    match companion_type {
        UiCompanionType::Critter => &mut inner.critters,
        UiCompanionType::Mount => &mut inner.mounts,
    }
}

/// Registers the build-12340 companion query family.
pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    state: UiCompanionState,
) -> mlua::Result<()> {
    let info_state = state.clone();
    globals.raw_set(
        "GetNumCompanions",
        lua.create_function(move |_, companion_type: Value| {
            let companion_type = parse_companion_type(companion_type)
                .ok_or_else(|| mlua::Error::runtime("Usage: GetNumCompanions(type)"))?;
            Ok(state.len(companion_type))
        })?,
    )?;
    globals.raw_set(
        "GetCompanionInfo",
        lua.create_function(move |lua, (companion_type, index): (Value, Value)| {
            let companion_type = parse_companion_type(companion_type)
                .ok_or_else(|| mlua::Error::runtime("Usage: GetCompanionInfo(type, index)"))?;
            let index = companion_index(index)
                .ok_or_else(|| mlua::Error::runtime("Usage: GetCompanionInfo(type, index)"))?;
            let Some(companion) = info_state.companion(companion_type, index) else {
                return Ok(MultiValue::new());
            };
            Ok(MultiValue::from_vec(vec![
                Value::Integer(i64::from(companion.creature_id())),
                companion
                    .name()
                    .map(|name| lua.create_string(name).map(Value::String))
                    .transpose()?
                    .unwrap_or(Value::Nil),
                Value::Integer(i64::from(companion.spell_id())),
                companion
                    .icon()
                    .map(|icon| lua.create_string(icon).map(Value::String))
                    .transpose()?
                    .unwrap_or(Value::Nil),
                if companion.active() {
                    Value::Number(1.0)
                } else {
                    Value::Nil
                },
            ]))
        })?,
    )
}

fn parse_companion_type(value: Value) -> Option<UiCompanionType> {
    let Value::String(value) = value else {
        return None;
    };
    let value = value.to_string_lossy();
    if value.eq_ignore_ascii_case("CRITTER") {
        Some(UiCompanionType::Critter)
    } else if value.eq_ignore_ascii_case("MOUNT") {
        Some(UiCompanionType::Mount)
    } else {
        None
    }
}

fn companion_index(value: Value) -> Option<usize> {
    let number = match value {
        Value::Integer(number) => number as f64,
        Value::Number(number) => number,
        Value::String(number) => number.to_str().ok()?.parse::<f64>().ok()?,
        _ => return None,
    };
    if !number.is_finite() || number < 1.0 || number > usize::MAX as f64 {
        return Some(0);
    }
    Some(number.round() as usize)
}
