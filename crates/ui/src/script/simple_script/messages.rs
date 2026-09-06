//! ScrollingMessageFrame's native message ring and script argument contracts.

use mlua::{AnyUserData, Lua, Table, UserData, Value, Variadic};

use crate::widget::{Message, MessageConfig, MessageHistory};

use super::{DIRTY_TEXT, hidden_key, mark_object_state_changed, text_color_key, text_key};

static HISTORY_TOKEN: u8 = 0;

impl UserData for MessageHistory {}

pub(super) fn initialize(lua: &Lua, object: &Table, config: MessageConfig) -> mlua::Result<()> {
    object.raw_set(
        super::justify_v_key(),
        if config.insert_at_top {
            "TOP"
        } else {
            "BOTTOM"
        },
    )?;
    object.raw_set(
        hidden_key(&HISTORY_TOKEN),
        lua.create_userdata(MessageHistory::new(config))?,
    )?;
    object.raw_set(text_key(), "")
}

fn history(object: &Table) -> mlua::Result<AnyUserData> {
    object.raw_get(hidden_key(&HISTORY_TOKEN))
}

fn publish(lua: &Lua, object: &Table) -> mlua::Result<()> {
    let state = history(object)?;
    let text = state.borrow::<MessageHistory>()?.presentation_text();
    if object.raw_get::<Option<String>>(text_key())?.as_deref() != Some(text.as_str()) {
        object.raw_set(text_key(), text)?;
        mark_object_state_changed(lua, object, DIRTY_TEXT)?;
    }
    Ok(())
}

pub(super) fn register(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "SetHyperlinksEnabled",
        lua.create_function(|_, (object, arguments): (Table, Variadic<Value>)| {
            history(&object)?
                .borrow_mut::<MessageHistory>()?
                .hyperlinks_enabled = super::native_optional_bool(arguments.first(), true);
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "GetHyperlinksEnabled",
        lua.create_function(|_, object: Table| {
            Ok(history(&object)?
                .borrow::<MessageHistory>()?
                .hyperlinks_enabled
                .then_some(1_u8))
        })?,
    )?;
    methods.raw_set(
        "GetMaxLines",
        lua.create_function(|_, object: Table| {
            Ok(history(&object)?.borrow::<MessageHistory>()?.config.maximum)
        })?,
    )?;
    methods.raw_set(
        "SetMaxLines",
        lua.create_function(|lua, (object, maximum): (Table, Value)| {
            let maximum = lua
                .coerce_number(maximum)?
                .map(|maximum| maximum as i32)
                .filter(|maximum| *maximum > 0)
                .ok_or_else(|| {
                    mlua::Error::runtime(
                        "Usage: ScrollingMessageFrame:SetMaxLines(positiveLineCount)",
                    )
                })?;
            history(&object)?
                .borrow_mut::<MessageHistory>()?
                .set_maximum(maximum as usize);
            publish(lua, &object)
        })?,
    )?;
    methods.raw_set(
        "Clear",
        lua.create_function(|lua, object: Table| {
            history(&object)?.borrow_mut::<MessageHistory>()?.clear();
            publish(lua, &object)
        })?,
    )?;
    methods.raw_set(
        "GetCurrentLine",
        lua.create_function(|_, object: Table| {
            Ok(history(&object)?.borrow::<MessageHistory>()?.current_line)
        })?,
    )?;
    methods.raw_set(
        "GetNumMessages",
        lua.create_function(|lua, (object, access): (Table, Value)| {
            let access = lua.coerce_number(access)?.unwrap_or(0.0) as i32;
            Ok(history(&object)?.borrow::<MessageHistory>()?.count(access))
        })?,
    )?;
    methods.raw_set(
        "GetMessageInfo",
        lua.create_function(|lua, (object, index, access): (Table, Value, Value)| {
            let index = lua
                .coerce_number(index)?
                .map(|index| index as i32)
                .ok_or_else(|| {
                    mlua::Error::runtime(
                        "Usage: ScrollingMessageFrame:GetMessageInfo(index [, accessID])",
                    )
                })?;
            let access = lua.coerce_number(access)?.unwrap_or(0.0) as i32;
            let state = history(&object)?;
            let state = state.borrow::<MessageHistory>()?;
            let entry = index
                .checked_sub(1)
                .and_then(|index| usize::try_from(index).ok())
                .and_then(|index| state.get(index, access))
                .ok_or_else(|| {
                    mlua::Error::runtime(
                        "ScrollingMessageFrame:GetMessageInfo: invalid message index",
                    )
                })?;
            Ok((
                entry.text.clone(),
                entry.access_id,
                entry.color_id,
                entry.type_id,
            ))
        })?,
    )?;
    methods.raw_set("AddMessage", lua.create_function(|lua, (object, arguments): (Table, Variadic<Value>)| {
        let text = lua.coerce_string(arguments.first().cloned().unwrap_or(Value::Nil))?
            .ok_or_else(|| mlua::Error::runtime("Usage: ScrollingMessageFrame:AddMessage(text [, red, green, blue, id, addToTop, accessID, typeID])"))?;
        let text = text.to_str()?.split('\0').next().unwrap_or_default().to_owned();
        if text.is_empty() { return Ok(()); }
        let number = |index| lua.coerce_number(arguments.get(index).cloned().unwrap_or(Value::Nil));
        let (color, start) = match (number(1)?, number(2)?, number(3)?) {
            (Some(r), Some(g), Some(b)) => (super::clamped_color(r, g, b, None), 4),
            _ => {
                let color: Table = object.raw_get(text_color_key())?;
                ([color.raw_get(1)?, color.raw_get(2)?, color.raw_get(3)?, color.raw_get(4)?], 1)
            }
        };
        let message = Message { text, color, color_id: number(start)?.unwrap_or(0.0) as i32,
            access_id: number(start + 2)?.unwrap_or(0.0) as i32,
            type_id: number(start + 3)?.unwrap_or(0.0) as i32 };
        let at_top = matches!(arguments.get(start + 1), Some(Value::Boolean(true)));
        history(&object)?.borrow_mut::<MessageHistory>()?.append(message, at_top);
        publish(lua, &object)
    })?)?;
    methods.raw_set(
        "UpdateColorByID",
        lua.create_function(
            |lua, (object, id, red, green, blue): (Table, Value, Value, Value, Value)| {
                let (Some(id), Some(red), Some(green), Some(blue)) = (
                    lua.coerce_number(id)?,
                    lua.coerce_number(red)?,
                    lua.coerce_number(green)?,
                    lua.coerce_number(blue)?,
                ) else {
                    return Ok(());
                };
                let state = history(&object)?;
                for message in &mut state.borrow_mut::<MessageHistory>()?.entries {
                    if message.color_id == id as i32 {
                        message.color = super::clamped_color(red, green, blue, None);
                    }
                }
                publish(lua, &object)
            },
        )?,
    )?;
    Ok(())
}
