//! Native tutorial discovery, completion history, and server acknowledgements.

#[cfg(test)]
#[path = "../../tests/stock_seed/feature/tutorial.rs"]
mod tests;

use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use mlua::{Lua, MultiValue, Table, Value};

/// An authored tutorial action awaiting admission to the active session writer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiTutorialAction {
    /// Acknowledge one zero-based tutorial with `CMSG_TUTORIAL_FLAG`.
    Flag(u32),
    /// Complete all tutorial flags with `CMSG_TUTORIAL_CLEAR`.
    Clear,
    /// Reset tutorial flags with `CMSG_TUTORIAL_RESET`.
    Reset,
}

#[derive(Debug, Default)]
struct TutorialBank {
    bit_count: usize,
    words: Vec<u32>,
}

impl TutorialBank {
    fn replace(&mut self, bytes: &[u8]) {
        self.bit_count = bytes.len() * 8;
        let word_count = bytes.len().div_ceil(4);
        if self.words.len() < word_count {
            self.words.resize(word_count, 0);
        }
        // 530920 copies the byte count, preserving the unused bytes of an
        // existing partial word and the allocated tail after a shorter image.
        for (index, byte) in bytes.iter().enumerate() {
            let shift = (index % 4) * 8;
            self.words[index / 4] =
                (self.words[index / 4] & !(255 << shift)) | (u32::from(*byte) << shift);
        }
    }

    fn contains(&self, index: u32) -> bool {
        self.bit_count != 0
            && self
                .words
                .get(index as usize / 32)
                .is_some_and(|word| word & (1 << (index % 32)) != 0)
    }

    fn insert(&mut self, index: u32) -> bool {
        let Some(word) = self.words.get_mut(index as usize / 32) else {
            return false;
        };
        *word |= 1 << (index % 32);
        true
    }

    fn fill(&mut self, value: u32) {
        let length = self.bit_count.div_ceil(32);
        self.words[..length].fill(value);
    }
}

#[derive(Debug)]
struct TutorialState {
    seen: TutorialBank,
    completed: TutorialBank,
    history: [u32; 60],
    actions: VecDeque<UiTutorialAction>,
}

impl Default for TutorialState {
    fn default() -> Self {
        Self {
            seen: TutorialBank::default(),
            completed: TutorialBank::default(),
            history: [0; 60],
            actions: VecDeque::new(),
        }
    }
}

impl TutorialState {
    fn append_history(&mut self, index: u32) {
        let id = index.wrapping_add(1);
        for slot in &mut self.history {
            if *slot == id {
                return;
            }
            if *slot == 0 {
                *slot = id;
                return;
            }
        }
    }
}

/// Shared main-thread state behind stock's seven tutorial Lua functions.
#[derive(Clone, Debug, Default)]
pub struct UiTutorialState {
    inner: Rc<RefCell<TutorialState>>,
}

impl UiTutorialState {
    /// Replaces both native flag banks from all bytes of `SMSG_TUTORIAL_FLAGS`.
    /// Existing local completion history remains intact, as in `530920`.
    pub fn replace_flags(&self, bytes: &[u8]) {
        let mut state = self.inner.borrow_mut();
        state.seen.replace(bytes);
        state.completed.replace(bytes);
    }

    /// Admits a zero-based tutorial once its server flag bank exists.
    #[must_use]
    pub fn needs_trigger(&self, index: u32) -> bool {
        let state = self.inner.borrow();
        state.seen.bit_count != 0
            && (index as usize) < state.seen.bit_count
            && !state.seen.contains(index)
    }

    /// Marks discovery after `TUTORIAL_TRIGGER` has returned to its native owner.
    pub fn mark_triggered(&self, index: u32) {
        self.inner.borrow_mut().seen.insert(index);
    }

    /// Completes a zero-based tutorial, retaining its first completion order.
    pub fn flag(&self, index: u32) {
        let mut state = self.inner.borrow_mut();
        if state.completed.contains(index) || !state.completed.insert(index) {
            return;
        }
        state.seen.insert(index);
        state.append_history(index);
        state.actions.push_back(UiTutorialAction::Flag(index));
    }

    /// Marks both banks complete and queues the native clear command.
    pub fn clear(&self) {
        let mut state = self.inner.borrow_mut();
        state.seen.fill(u32::MAX);
        state.completed.fill(u32::MAX);
        for index in 0..60 {
            if !state.completed.contains(index) {
                state.append_history(index);
            }
        }
        state.actions.push_back(UiTutorialAction::Clear);
    }

    /// Clears both banks and completion history, retaining the server bit count.
    pub fn reset(&self) {
        let mut state = self.inner.borrow_mut();
        state.seen.fill(0);
        state.completed.fill(0);
        state.history.fill(0);
        state.actions.push_back(UiTutorialAction::Reset);
    }

    /// Returns whether any discovered tutorial permits the reset control.
    #[must_use]
    pub fn can_reset(&self) -> bool {
        let state = self.inner.borrow();
        state.seen.words[..state.seen.bit_count.div_ceil(32)]
            .iter()
            .any(|word| *word != 0)
    }

    /// Returns completion of one zero-based tutorial.
    #[must_use]
    pub fn is_flagged(&self, index: u32) -> bool {
        self.inner.borrow().completed.contains(index)
    }

    /// Returns the following completed one-based ID in original completion order.
    #[must_use]
    pub fn next_completed(&self, id: u32) -> Option<u32> {
        let state = self.inner.borrow();
        let mut found = false;
        for value in state.history {
            if found && value != 0 {
                return (value != 60).then_some(value);
            }
            if value == id {
                found = true;
            }
        }
        None
    }

    /// Returns the preceding completed one-based ID using `530190`'s inclusive scan.
    #[must_use]
    pub fn previous_completed(&self, id: u32) -> Option<u32> {
        let state = self.inner.borrow();
        // Native starts at history[60], which is the adjacent seen bit count.
        let mut found = state.seen.bit_count as u32 == id;
        for value in state.history.into_iter().rev() {
            if found && value != 0 {
                return Some(value);
            }
            if value == id {
                found = true;
            }
        }
        None
    }

    /// Peeks the next command so writer backpressure cannot discard it.
    #[must_use]
    pub fn pending_action(&self) -> Option<UiTutorialAction> {
        self.inner.borrow().actions.front().copied()
    }

    /// Removes the command after the active session writer has accepted it.
    pub fn accept_action(&self) {
        self.inner.borrow_mut().actions.pop_front();
    }
}

pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    state: UiTutorialState,
) -> mlua::Result<()> {
    let query = state.clone();
    globals.raw_set(
        "CanResetTutorials",
        lua.create_function(move |_, ()| Ok(query.can_reset().then_some(1)))?,
    )?;
    let query = state.clone();
    globals.raw_set(
        "IsTutorialFlagged",
        lua.create_function(move |lua, value: Value| {
            let index = argument(lua, value, "Trigger")?;
            if !(1..=60).contains(&index) {
                return Ok(MultiValue::new());
            }
            Ok(MultiValue::from_vec(vec![
                if query.is_flagged(index as u32 - 1) {
                    Value::Number(1.0)
                } else {
                    Value::Nil
                },
            ]))
        })?,
    )?;
    let flag = state.clone();
    globals.raw_set(
        "FlagTutorial",
        lua.create_function(move |lua, value: Value| {
            let index = argument(lua, value, "FlagTutorial")?;
            if (1..=60).contains(&index) {
                flag.flag(index as u32 - 1);
            }
            Ok(())
        })?,
    )?;
    let clear = state.clone();
    globals.raw_set(
        "ClearTutorials",
        lua.create_function(move |_, ()| {
            clear.clear();
            Ok(())
        })?,
    )?;
    let reset = state.clone();
    globals.raw_set(
        "ResetTutorials",
        lua.create_function(move |_, ()| {
            reset.reset();
            Ok(())
        })?,
    )?;
    let next = state.clone();
    globals.raw_set(
        "GetNextCompleatedTutorial",
        lua.create_function(move |lua, value: Value| {
            Ok(optional_result(next.next_completed(argument(
                lua,
                value,
                "GetNextCompleatedTutorial",
            )? as u32)))
        })?,
    )?;
    globals.raw_set(
        "GetPrevCompleatedTutorial",
        lua.create_function(move |lua, value: Value| {
            Ok(optional_result(state.previous_completed(argument(
                lua,
                value,
                "GetPrevCompleatedTutorial",
            )?
                as u32)))
        })?,
    )?;
    Ok(())
}

fn argument(lua: &Lua, value: Value, name: &str) -> mlua::Result<i32> {
    let number = lua
        .coerce_number(value)?
        .ok_or_else(|| mlua::Error::runtime(format!("Usage: {name}(\"tutorial\")")))?;
    // 88B9C0 returns x86's integer-indefinite lane on non-finite/overflow input.
    Ok(
        if !number.is_finite()
            || number.trunc() < f64::from(i32::MIN)
            || number.trunc() > f64::from(i32::MAX)
        {
            i32::MIN
        } else {
            number as i32
        },
    )
}

fn optional_result(value: Option<u32>) -> MultiValue {
    value.map_or_else(MultiValue::new, |value| {
        MultiValue::from_vec(vec![Value::Number(f64::from(value))])
    })
}
