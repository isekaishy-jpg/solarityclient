//! Fixed death-knight rune slots projected from controlled-player state.

use std::cell::RefCell;
use std::rc::Rc;

use mlua::{Lua, Table, Value};

/// Number of rune slots authored by build-12340 FrameXML.
pub const UI_RUNE_SLOT_COUNT: usize = 6;

/// Script-visible rune texture classifications.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiRuneType {
    /// Blood rune.
    Blood,
    /// Unholy rune.
    Unholy,
    /// Frost rune.
    Frost,
    /// Temporary death rune.
    Death,
}

impl UiRuneType {
    const fn id(self) -> u8 {
        match self {
            Self::Blood => 1,
            Self::Unholy => 2,
            Self::Frost => 3,
            Self::Death => 4,
        }
    }
}

/// One occupied rune slot and its recharge state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiRune {
    rune_type: UiRuneType,
    cooldown_start: f64,
    cooldown_duration: f64,
    ready: bool,
}

impl UiRune {
    /// Creates one ready rune of the supplied type.
    #[must_use]
    pub const fn ready(rune_type: UiRuneType) -> Self {
        Self {
            rune_type,
            cooldown_start: 0.0,
            cooldown_duration: 0.0,
            ready: true,
        }
    }

    /// Creates one recharging rune using client-clock seconds.
    #[must_use]
    pub const fn recharging(rune_type: UiRuneType, start: f64, duration: f64) -> Self {
        Self {
            rune_type,
            cooldown_start: start,
            cooldown_duration: duration,
            ready: false,
        }
    }
}

/// Shared fixed rune-slot image.
#[derive(Clone, Debug, Default)]
pub struct UiRuneState {
    slots: Rc<RefCell<[Option<UiRune>; UI_RUNE_SLOT_COUNT]>>,
}

impl UiRuneState {
    /// Creates the non-death-knight state with no occupied rune slots.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces all six rune slots atomically.
    pub fn replace(&self, slots: [Option<UiRune>; UI_RUNE_SLOT_COUNT]) {
        *self.slots.borrow_mut() = slots;
    }

    fn rune(&self, index: usize) -> Option<UiRune> {
        index
            .checked_sub(1)
            .and_then(|index| self.slots.borrow().get(index).copied())
            .flatten()
    }
}

/// Registers the complete rune query family used by `RuneFrame.lua`.
pub(crate) fn register_globals(lua: &Lua, globals: &Table, state: UiRuneState) -> mlua::Result<()> {
    let cooldown_state = state.clone();
    globals.raw_set(
        "GetRuneType",
        lua.create_function(move |_, index: usize| {
            Ok(state.rune(index).map(|rune| rune.rune_type.id()))
        })?,
    )?;
    globals.raw_set(
        "GetRuneCooldown",
        lua.create_function(move |_, index: usize| {
            Ok(cooldown_state
                .rune(index)
                .map_or((0.0, 0.0, Value::Boolean(true)), |rune| {
                    (
                        rune.cooldown_start,
                        rune.cooldown_duration,
                        Value::Boolean(rune.ready),
                    )
                }))
        })?,
    )
}
