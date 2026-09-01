//! Joined chat-channel display rows and member rosters.

use std::cell::RefCell;
use std::rc::Rc;

use mlua::{Lua, MultiValue, Table, Value};

/// Stock channel-category token used by ChannelFrame branches.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiChannelCategory {
    /// Built-in zone-wide channel.
    World,
    /// Party, raid, or battleground channel.
    Group,
    /// Player-created custom channel.
    Custom,
}

impl UiChannelCategory {
    const fn as_str(self) -> &'static str {
        match self {
            Self::World => "CHANNEL_CATEGORY_WORLD",
            Self::Group => "CHANNEL_CATEGORY_GROUP",
            Self::Custom => "CHANNEL_CATEGORY_CUSTOM",
        }
    }
}

/// One channel roster member and their text/voice permissions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiChannelMember {
    name: String,
    owner: bool,
    moderator: bool,
    muted: bool,
    voice_active: bool,
    voice_enabled: bool,
}

impl UiChannelMember {
    /// Creates one ordinary unmuted text-only member.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            owner: false,
            moderator: false,
            muted: false,
            voice_active: false,
            voice_enabled: false,
        }
    }
}

/// One row in the stock channel display list.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiChannelDisplay {
    /// A collapsible category row.
    Header {
        /// Localized category label.
        name: String,
        /// Whether child rows are hidden.
        collapsed: bool,
        /// Number of child rows, if nonzero.
        child_count: u32,
    },
    /// One joined or joinable channel row.
    Channel {
        /// Localized or custom channel name.
        name: String,
        /// Chat message channel number when joined.
        channel_number: Option<u32>,
        /// Stock channel category.
        category: UiChannelCategory,
        /// Whether the character is currently joined.
        active: bool,
        /// Whether voice is enabled for this channel.
        voice_enabled: bool,
        /// Whether this is the active voice channel.
        voice_active: bool,
        /// Ordered joined-member roster.
        members: Vec<UiChannelMember>,
    },
}

impl UiChannelDisplay {
    /// Creates one category header.
    #[must_use]
    pub fn header(name: impl Into<String>, collapsed: bool, child_count: u32) -> Self {
        Self::Header {
            name: name.into(),
            collapsed,
            child_count,
        }
    }

    /// Creates one text-only channel row.
    #[must_use]
    pub fn channel(
        name: impl Into<String>,
        channel_number: Option<u32>,
        category: UiChannelCategory,
        active: bool,
        members: Vec<UiChannelMember>,
    ) -> Self {
        Self::Channel {
            name: name.into(),
            channel_number,
            category,
            active,
            voice_enabled: false,
            voice_active: false,
            members,
        }
    }
}

#[derive(Debug, Default)]
struct UiChannelInner {
    displays: Vec<UiChannelDisplay>,
    selected_display: Option<usize>,
}

/// Shared joined-channel list and client-owned display selection.
#[derive(Clone, Debug, Default)]
pub struct UiChannelState {
    inner: Rc<RefCell<UiChannelInner>>,
}

impl UiChannelState {
    /// Creates the pre-world empty channel list.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces all visible channel rows after a channel service update.
    pub fn replace(&self, displays: Vec<UiChannelDisplay>) {
        let mut inner = self.inner.borrow_mut();
        inner.displays = displays;
        if inner
            .selected_display
            .is_some_and(|selection| selection > inner.displays.len())
        {
            inner.selected_display = None;
        }
    }

    /// Returns the selected one-based display row.
    #[must_use]
    pub fn selected_display(&self) -> Option<usize> {
        self.inner.borrow().selected_display
    }
}

/// Registers channel-list and channel-roster queries.
pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    state: UiChannelState,
) -> mlua::Result<()> {
    let selected_state = state.clone();
    let set_selected_state = state.clone();
    let display_state = state.clone();
    let roster_state = state.clone();
    globals.raw_set(
        "GetNumDisplayChannels",
        lua.create_function(move |_, ()| Ok(state.inner.borrow().displays.len()))?,
    )?;
    globals.raw_set(
        "GetSelectedDisplayChannel",
        lua.create_function(move |_, ()| Ok(selected_state.selected_display()))?,
    )?;
    globals.raw_set(
        "SetSelectedDisplayChannel",
        lua.create_function(move |lua, value: Value| {
            let number = lua.coerce_number(value)?.unwrap_or(0.0);
            let index = if number.is_finite() && number >= 1.0 && number <= usize::MAX as f64 {
                number.round() as usize
            } else {
                0
            };
            let mut inner = set_selected_state.inner.borrow_mut();
            inner.selected_display = (index <= inner.displays.len()).then_some(index);
            Ok(())
        })?,
    )?;
    globals.raw_set(
        "GetChannelDisplayInfo",
        lua.create_function(move |lua, value: Value| {
            let index = lua.coerce_number(value)?.unwrap_or(0.0).round() as usize;
            let display = index
                .checked_sub(1)
                .and_then(|index| display_state.inner.borrow().displays.get(index).cloned());
            channel_display_values(lua, display)
        })?,
    )?;
    globals.raw_set(
        "GetChannelRosterInfo",
        lua.create_function(move |lua, (display, member): (Value, Value)| {
            let display = lua.coerce_number(display)?.unwrap_or(0.0).round() as usize;
            let member = lua.coerce_number(member)?.unwrap_or(0.0).round() as usize;
            let value = display.checked_sub(1).and_then(|display| {
                member.checked_sub(1).and_then(|member| {
                    let inner = roster_state.inner.borrow();
                    let UiChannelDisplay::Channel { members, .. } = inner.displays.get(display)?
                    else {
                        return None;
                    };
                    members.get(member).cloned()
                })
            });
            channel_member_values(lua, value)
        })?,
    )
}

fn channel_display_values(
    lua: &Lua,
    display: Option<UiChannelDisplay>,
) -> mlua::Result<MultiValue> {
    let Some(display) = display else {
        return Ok(MultiValue::new());
    };
    let values = match display {
        UiChannelDisplay::Header {
            name,
            collapsed,
            child_count,
        } => vec![
            Value::String(lua.create_string(name)?),
            flag(true),
            flag(collapsed),
            Value::Nil,
            if child_count > 0 {
                Value::Integer(i64::from(child_count))
            } else {
                Value::Nil
            },
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Nil,
        ],
        UiChannelDisplay::Channel {
            name,
            channel_number,
            category,
            active,
            voice_enabled,
            voice_active,
            members,
        } => vec![
            Value::String(lua.create_string(name)?),
            Value::Nil,
            Value::Nil,
            channel_number.map_or(Value::Nil, |number| Value::Integer(i64::from(number))),
            Value::Integer(members.len() as i64),
            flag(active),
            Value::String(lua.create_string(category.as_str())?),
            flag(voice_enabled),
            flag(voice_active),
        ],
    };
    Ok(MultiValue::from_vec(values))
}

fn channel_member_values(lua: &Lua, member: Option<UiChannelMember>) -> mlua::Result<MultiValue> {
    let Some(member) = member else {
        return Ok(MultiValue::new());
    };
    Ok(MultiValue::from_vec(vec![
        Value::String(lua.create_string(member.name)?),
        flag(member.owner),
        flag(member.moderator),
        flag(member.muted),
        flag(member.voice_active),
        flag(member.voice_enabled),
    ]))
}

fn flag(value: bool) -> Value {
    if value {
        Value::Number(1.0)
    } else {
        Value::Nil
    }
}
