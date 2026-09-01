//! Ordered quest-log rows and client-owned presentation state.

use std::cell::RefCell;
use std::rc::Rc;

use mlua::{Lua, MultiValue, Table, Value};

/// One row before collapsed quest headers are projected.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiQuestLogEntry {
    /// A collapsible localized zone or category header.
    Header {
        /// Localized header text.
        title: String,
        /// Whether following quests are hidden.
        collapsed: bool,
    },
    /// One accepted quest.
    Quest(UiQuestLogQuest),
}

impl UiQuestLogEntry {
    /// Creates one expanded quest-log header.
    #[must_use]
    pub fn header(title: impl Into<String>) -> Self {
        Self::Header {
            title: title.into(),
            collapsed: false,
        }
    }
}

/// Script-visible title metadata for one accepted quest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiQuestLogQuest {
    quest_id: u32,
    title: String,
    level: u32,
    tag: Option<String>,
    suggested_group: u32,
    completion: Option<i8>,
    daily: bool,
    pushable: bool,
    watched: bool,
    abandonable: bool,
}

impl UiQuestLogQuest {
    /// Creates an ordinary incomplete quest with no optional title metadata.
    #[must_use]
    pub fn new(quest_id: u32, title: impl Into<String>, level: u32) -> Self {
        Self {
            quest_id,
            title: title.into(),
            level,
            tag: None,
            suggested_group: 0,
            completion: None,
            daily: false,
            pushable: false,
            watched: false,
            abandonable: true,
        }
    }

    /// Sets the localized quest-type tag.
    #[must_use]
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
        self
    }

    /// Sets the suggested party size.
    #[must_use]
    pub const fn with_suggested_group(mut self, suggested_group: u32) -> Self {
        self.suggested_group = suggested_group;
        self
    }

    /// Sets the completion marker: negative for failed and positive for complete.
    #[must_use]
    pub const fn with_completion(mut self, completion: i8) -> Self {
        self.completion = Some(completion);
        self
    }

    /// Marks the quest as daily.
    #[must_use]
    pub const fn daily(mut self, daily: bool) -> Self {
        self.daily = daily;
        self
    }

    /// Marks the quest as eligible for party sharing.
    #[must_use]
    pub const fn pushable(mut self, pushable: bool) -> Self {
        self.pushable = pushable;
        self
    }

    /// Sets whether the watch frame tracks the quest.
    #[must_use]
    pub const fn watched(mut self, watched: bool) -> Self {
        self.watched = watched;
        self
    }

    /// Sets whether the player may abandon the quest.
    #[must_use]
    pub const fn abandonable(mut self, abandonable: bool) -> Self {
        self.abandonable = abandonable;
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UiQuestSelection {
    Header(usize),
    Quest(u32),
}

#[derive(Debug, Default)]
struct UiQuestLogInner {
    entries: Vec<UiQuestLogEntry>,
    selection: Option<UiQuestSelection>,
    abandon_quest_id: Option<u32>,
    daily_quests_completed: u32,
    maximum_daily_quests: u32,
}

/// Shared server quest log plus client-owned row state.
#[derive(Clone, Debug, Default)]
pub struct UiQuestLogState {
    inner: Rc<RefCell<UiQuestLogInner>>,
}

impl UiQuestLogState {
    /// Creates the pre-character empty quest log.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the complete ordered quest log after a server update.
    pub fn replace(&self, entries: Vec<UiQuestLogEntry>) {
        let mut inner = self.inner.borrow_mut();
        inner.entries = entries;
        inner.selection = None;
        inner.abandon_quest_id = None;
    }

    /// Replaces the daily completion count and realm daily cap.
    pub fn set_daily_quest_counts(&self, completed: u32, maximum: u32) {
        let mut inner = self.inner.borrow_mut();
        inner.daily_quests_completed = completed;
        inner.maximum_daily_quests = maximum;
    }

    fn visible_entries(&self) -> Vec<(usize, UiQuestLogEntry)> {
        let inner = self.inner.borrow();
        let mut visible = Vec::new();
        let mut collapsed = false;
        for (full_index, entry) in inner.entries.iter().enumerate() {
            match entry {
                UiQuestLogEntry::Header {
                    collapsed: header_collapsed,
                    ..
                } => {
                    collapsed = *header_collapsed;
                    visible.push((full_index, entry.clone()));
                }
                UiQuestLogEntry::Quest(_) if !collapsed => {
                    visible.push((full_index, entry.clone()));
                }
                UiQuestLogEntry::Quest(_) => {}
            }
        }
        visible
    }

    fn select(&self, index: usize) {
        let selection = index.checked_sub(1).and_then(|index| {
            self.visible_entries()
                .get(index)
                .map(|(full_index, entry)| match entry {
                    UiQuestLogEntry::Header { .. } => UiQuestSelection::Header(*full_index),
                    UiQuestLogEntry::Quest(quest) => UiQuestSelection::Quest(quest.quest_id),
                })
        });
        self.inner.borrow_mut().selection = selection;
    }

    fn selected_index(&self) -> usize {
        let selection = self.inner.borrow().selection;
        let Some(selection) = selection else { return 0 };
        self.visible_entries()
            .iter()
            .position(|(full_index, entry)| match (selection, entry) {
                (UiQuestSelection::Header(selected), UiQuestLogEntry::Header { .. }) => {
                    selected == *full_index
                }
                (UiQuestSelection::Quest(selected), UiQuestLogEntry::Quest(quest)) => {
                    selected == quest.quest_id
                }
                _ => false,
            })
            .map_or(0, |index| index + 1)
    }

    fn set_header_collapsed(&self, index: usize, collapsed: bool) {
        let full_index = index.checked_sub(1).and_then(|index| {
            self.visible_entries()
                .get(index)
                .map(|(full_index, _)| *full_index)
        });
        let Some(full_index) = full_index else { return };
        if let Some(UiQuestLogEntry::Header {
            collapsed: current, ..
        }) = self.inner.borrow_mut().entries.get_mut(full_index)
        {
            *current = collapsed;
        }
    }

    fn set_abandon_quest(&self) {
        let mut inner = self.inner.borrow_mut();
        inner.abandon_quest_id = match inner.selection {
            Some(UiQuestSelection::Quest(selected)) => inner.entries.iter().find_map(|entry| {
                let UiQuestLogEntry::Quest(quest) = entry else {
                    return None;
                };
                (quest.quest_id == selected && quest.abandonable).then_some(selected)
            }),
            _ => None,
        };
    }

    fn abandon_quest_name(&self) -> Option<String> {
        let inner = self.inner.borrow();
        let selected = inner.abandon_quest_id?;
        inner.entries.iter().find_map(|entry| {
            let UiQuestLogEntry::Quest(quest) = entry else {
                return None;
            };
            (quest.quest_id == selected).then(|| quest.title.clone())
        })
    }

    fn selected_quest(&self) -> Option<UiQuestLogQuest> {
        let inner = self.inner.borrow();
        let Some(UiQuestSelection::Quest(selected)) = inner.selection else {
            return None;
        };
        inner.entries.iter().find_map(|entry| {
            let UiQuestLogEntry::Quest(quest) = entry else {
                return None;
            };
            (quest.quest_id == selected).then(|| quest.clone())
        })
    }

    fn set_watched(&self, index: usize, watched: bool) {
        let full_index = index.checked_sub(1).and_then(|index| {
            self.visible_entries()
                .get(index)
                .map(|(full_index, _)| *full_index)
        });
        let Some(full_index) = full_index else { return };
        if let Some(UiQuestLogEntry::Quest(quest)) =
            self.inner.borrow_mut().entries.get_mut(full_index)
        {
            quest.watched = watched;
        }
    }
}

/// Registers the core build-12340 quest-log family.
pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    state: UiQuestLogState,
) -> mlua::Result<()> {
    let title_state = state.clone();
    let select_state = state.clone();
    let selection_state = state.clone();
    let abandon_state = state.clone();
    let abandon_name_state = state.clone();
    let pushable_state = state.clone();
    let collapse_state = state.clone();
    let expand_state = state.clone();
    let watched_state = state.clone();
    let add_watch_state = state.clone();
    let remove_watch_state = state.clone();
    let watch_count_state = state.clone();
    let daily_count_state = state.clone();
    let daily_max_state = state.clone();
    globals.raw_set(
        "GetNumQuestLogEntries",
        lua.create_function(move |_, ()| {
            let visible = state.visible_entries();
            let quests = visible
                .iter()
                .filter(|(_, entry)| matches!(entry, UiQuestLogEntry::Quest(_)))
                .count();
            Ok((visible.len(), quests))
        })?,
    )?;
    globals.raw_set(
        "GetQuestLogTitle",
        lua.create_function(move |lua, value: Value| {
            let index = required_index(lua, value, "Usage: GetQuestLogTitle(index)")?;
            let entry = index
                .checked_sub(1)
                .and_then(|index| title_state.visible_entries().get(index).cloned())
                .map(|(_, entry)| entry);
            quest_title_values(lua, entry)
        })?,
    )?;
    globals.raw_set(
        "SelectQuestLogEntry",
        lua.create_function(move |lua, value: Value| {
            select_state.select(required_index(
                lua,
                value,
                "Usage: SelectQuestLogEntry(index)",
            )?);
            Ok(())
        })?,
    )?;
    globals.raw_set(
        "GetQuestLogSelection",
        lua.create_function(move |_, ()| Ok(selection_state.selected_index()))?,
    )?;
    globals.raw_set(
        "SetAbandonQuest",
        lua.create_function(move |_, ()| {
            abandon_state.set_abandon_quest();
            Ok(())
        })?,
    )?;
    globals.raw_set(
        "GetAbandonQuestName",
        lua.create_function(move |lua, ()| {
            abandon_name_state
                .abandon_quest_name()
                .map(|name| lua.create_string(name))
                .transpose()
        })?,
    )?;
    globals.raw_set(
        "GetQuestLogPushable",
        lua.create_function(move |_, ()| {
            Ok(pushable_state
                .selected_quest()
                .filter(|quest| quest.pushable)
                .map(|_| 1_u8))
        })?,
    )?;
    register_index_mutator(
        lua,
        globals,
        "CollapseQuestHeader",
        "Usage: CollapseQuestHeader(index)",
        move |index| collapse_state.set_header_collapsed(index, true),
    )?;
    register_index_mutator(
        lua,
        globals,
        "ExpandQuestHeader",
        "Usage: ExpandQuestHeader(index)",
        move |index| expand_state.set_header_collapsed(index, false),
    )?;
    globals.raw_set(
        "IsQuestWatched",
        lua.create_function(move |lua, value: Value| {
            let index = required_index(lua, value, "Usage: IsQuestWatched(index)")?;
            Ok(index
                .checked_sub(1)
                .and_then(|index| watched_state.visible_entries().get(index).cloned())
                .and_then(|(_, entry)| match entry {
                    UiQuestLogEntry::Quest(quest) if quest.watched => Some(1_u8),
                    _ => None,
                }))
        })?,
    )?;
    register_index_mutator(
        lua,
        globals,
        "AddQuestWatch",
        "Usage: AddQuestWatch(index)",
        move |index| add_watch_state.set_watched(index, true),
    )?;
    register_index_mutator(
        lua,
        globals,
        "RemoveQuestWatch",
        "Usage: RemoveQuestWatch(index)",
        move |index| remove_watch_state.set_watched(index, false),
    )?;
    globals.raw_set(
        "GetNumQuestWatches",
        lua.create_function(move |_, ()| {
            Ok(watch_count_state
                .inner
                .borrow()
                .entries
                .iter()
                .filter(|entry| matches!(entry, UiQuestLogEntry::Quest(quest) if quest.watched))
                .count())
        })?,
    )?;
    globals.raw_set(
        "GetDailyQuestsCompleted",
        lua.create_function(move |_, ()| {
            Ok(daily_count_state.inner.borrow().daily_quests_completed)
        })?,
    )?;
    globals.raw_set(
        "GetMaxDailyQuests",
        lua.create_function(move |_, ()| Ok(daily_max_state.inner.borrow().maximum_daily_quests))?,
    )
}

fn register_index_mutator(
    lua: &Lua,
    globals: &Table,
    name: &'static str,
    usage: &'static str,
    mutator: impl Fn(usize) + 'static,
) -> mlua::Result<()> {
    globals.raw_set(
        name,
        lua.create_function(move |lua, value: Value| {
            mutator(required_index(lua, value, usage)?);
            Ok(())
        })?,
    )
}

fn required_index(lua: &Lua, value: Value, usage: &'static str) -> mlua::Result<usize> {
    let number = lua
        .coerce_number(value)?
        .ok_or_else(|| mlua::Error::runtime(usage))?;
    if !number.is_finite() || number < 1.0 || number > usize::MAX as f64 {
        return Ok(0);
    }
    Ok(number.round() as usize)
}

fn quest_title_values(lua: &Lua, entry: Option<UiQuestLogEntry>) -> mlua::Result<MultiValue> {
    let values = match entry {
        Some(UiQuestLogEntry::Header { title, collapsed }) => vec![
            Value::String(lua.create_string(title)?),
            Value::Integer(0),
            Value::Nil,
            Value::Integer(0),
            Value::Number(1.0),
            numeric_flag(collapsed),
            Value::Nil,
            Value::Nil,
            Value::Integer(0),
        ],
        Some(UiQuestLogEntry::Quest(quest)) => vec![
            Value::String(lua.create_string(quest.title)?),
            Value::Integer(i64::from(quest.level)),
            quest
                .tag
                .map(|tag| lua.create_string(tag).map(Value::String))
                .transpose()?
                .unwrap_or(Value::Nil),
            Value::Integer(i64::from(quest.suggested_group)),
            Value::Nil,
            Value::Nil,
            quest
                .completion
                .map_or(Value::Nil, |value| Value::Integer(i64::from(value))),
            numeric_flag(quest.daily),
            Value::Integer(i64::from(quest.quest_id)),
        ],
        None => vec![
            Value::Nil,
            Value::Integer(0),
            Value::Nil,
            Value::Integer(0),
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Integer(0),
        ],
    };
    Ok(MultiValue::from_vec(values))
}

fn numeric_flag(value: bool) -> Value {
    if value {
        Value::Number(1.0)
    } else {
        Value::Nil
    }
}
