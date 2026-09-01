//! Spell-book navigation and spell presentation behavior evidenced by `SpellBookFrame.cpp`.

mod spell_book_frame;

use std::cell::RefCell;
use std::rc::Rc;

use mlua::{Lua, MultiValue, Table, Value};

/// One class-tab projection in the player's ordered spell book.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiSpellBookTab {
    name: String,
    texture: String,
    offset: u32,
    spell_count: u32,
    highest_rank_offset: u32,
    highest_rank_spell_count: u32,
}

impl UiSpellBookTab {
    /// Creates one tab from its localized SkillLine and learned-spell range.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        texture: impl Into<String>,
        offset: u32,
        spell_count: u32,
    ) -> Self {
        Self {
            name: name.into(),
            texture: texture.into(),
            offset,
            spell_count,
            highest_rank_offset: offset,
            highest_rank_spell_count: spell_count,
        }
    }

    /// Replaces the filtered range used when lower spell ranks are hidden.
    #[must_use]
    pub const fn with_highest_rank_range(mut self, offset: u32, spell_count: u32) -> Self {
        self.highest_rank_offset = offset;
        self.highest_rank_spell_count = spell_count;
        self
    }

    /// Returns the localized spell-book tab label.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the tab icon's stock texture identity.
    #[must_use]
    pub fn texture(&self) -> &str {
        &self.texture
    }

    /// Returns the zero-based offset into the ordered spell sequence.
    #[must_use]
    pub const fn offset(&self) -> u32 {
        self.offset
    }

    /// Returns the number of spells assigned to this tab.
    #[must_use]
    pub const fn spell_count(&self) -> u32 {
        self.spell_count
    }

    /// Returns the zero-based filtered offset when lower ranks are hidden.
    #[must_use]
    pub const fn highest_rank_offset(&self) -> u32 {
        self.highest_rank_offset
    }

    /// Returns the filtered spell count when lower ranks are hidden.
    #[must_use]
    pub const fn highest_rank_spell_count(&self) -> u32 {
        self.highest_rank_spell_count
    }
}

/// Shared ordered spell-book tabs projected from learned spells and SkillLine data.
#[derive(Clone, Debug, Default)]
pub struct UiSpellBookState {
    inner: Rc<RefCell<UiSpellBookInner>>,
}

#[derive(Debug, Default)]
struct UiSpellBookInner {
    tabs: Vec<UiSpellBookTab>,
    pet_spells: Option<(u32, String)>,
}

impl UiSpellBookState {
    /// Creates the pre-character spell book without synthesized tabs.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces all class tabs after the learned-spell image changes.
    pub fn set_tabs(&self, tabs: Vec<UiSpellBookTab>) {
        self.inner.borrow_mut().tabs = tabs;
    }

    /// Sets the current pet spell count and localized class pet-name token.
    pub fn set_pet_spells(&self, spell_count: u32, pet_name_token: impl Into<String>) {
        let mut inner = self.inner.borrow_mut();
        inner.pet_spells = (spell_count != 0).then(|| (spell_count, pet_name_token.into()));
    }

    /// Clears the pet spell book when the player controls no applicable pet.
    pub fn clear_pet_spells(&self) {
        self.inner.borrow_mut().pet_spells = None;
    }

    /// Returns the number of script-visible tabs.
    #[must_use]
    pub fn tab_count(&self) -> usize {
        self.inner.borrow().tabs.len()
    }

    /// Returns one one-based tab, if present.
    #[must_use]
    pub fn tab(&self, index: usize) -> Option<UiSpellBookTab> {
        index
            .checked_sub(1)
            .and_then(|index| self.inner.borrow().tabs.get(index).cloned())
    }

    /// Returns the pet spell count and localized class pet-name token.
    #[must_use]
    pub fn pet_spell_info(&self) -> Option<(u32, String)> {
        self.inner.borrow().pet_spells.clone()
    }
}

/// Registers the build-12340 spell-book tab query family.
pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    state: UiSpellBookState,
) -> mlua::Result<()> {
    let info_state = state.clone();
    let pet_state = state.clone();
    globals.raw_set(
        "GetNumSpellTabs",
        lua.create_function(move |_, ()| Ok(state.tab_count()))?,
    )?;
    globals.raw_set(
        "GetSpellTabInfo",
        lua.create_function(move |lua, value: Value| {
            let Some(index) = spell_tab_index(value)? else {
                return Ok(MultiValue::new());
            };
            let Some(tab) = info_state.tab(index) else {
                return Ok(MultiValue::new());
            };
            Ok(MultiValue::from_vec(vec![
                Value::String(lua.create_string(tab.name())?),
                Value::String(lua.create_string(tab.texture())?),
                Value::Integer(i64::from(tab.offset())),
                Value::Integer(i64::from(tab.spell_count())),
                Value::Integer(i64::from(tab.highest_rank_offset())),
                Value::Integer(i64::from(tab.highest_rank_spell_count())),
            ]))
        })?,
    )?;
    globals.raw_set(
        "HasPetSpells",
        lua.create_function(move |lua, ()| {
            let Some((spell_count, pet_name_token)) = pet_state.pet_spell_info() else {
                return Ok(MultiValue::from_vec(vec![Value::Nil, Value::Nil]));
            };
            Ok(MultiValue::from_vec(vec![
                Value::Integer(i64::from(spell_count)),
                Value::String(lua.create_string(pet_name_token)?),
            ]))
        })?,
    )
}

fn spell_tab_index(value: Value) -> mlua::Result<Option<usize>> {
    let number = match value {
        Value::Integer(number) => number as f64,
        Value::Number(number) => number,
        Value::String(number) => match number.to_str()?.parse::<f64>() {
            Ok(number) => number,
            Err(_) => return Ok(None),
        },
        _ => return Ok(None),
    };
    if !number.is_finite() || number < 1.0 || number > usize::MAX as f64 {
        return Ok(None);
    }
    Ok(Some(number.round() as usize))
}
