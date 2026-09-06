//! Build-12340 positional argument extension to Lua 5.1 string.format.

use mlua::{Function, Lua, MultiValue, Table, Value};

pub(super) fn register(lua: &Lua) -> mlua::Result<()> {
    let string: Table = lua.globals().raw_get("string")?;
    let original: Function = string.raw_get("format")?;
    string.raw_set(
        "format",
        lua.create_function(move |lua, arguments: MultiValue| {
            let Some(format) = arguments
                .front()
                .cloned()
                .map(|value| lua.coerce_string(value))
                .transpose()?
                .flatten()
            else {
                return original.call::<Value>(arguments);
            };
            let bytes = format.as_bytes();
            if !bytes.contains(&b'$') {
                return original.call::<Value>(arguments);
            }
            let mut rewritten = Vec::with_capacity(bytes.len());
            let mut selected = Vec::new();
            let mut cursor = 0;
            let mut argument = 1_usize;
            while cursor < bytes.len() {
                let byte = bytes[cursor];
                rewritten.push(byte);
                cursor += 1;
                if byte != b'%' || cursor == bytes.len() {
                    continue;
                }
                if bytes[cursor] == b'%' {
                    rewritten.push(b'%');
                    cursor += 1;
                    continue;
                }
                // 0x00853C50 accepts one or two digits followed by '$'. A numbered
                // conversion also resets the next sequential argument position.
                let digits = bytes[cursor..]
                    .iter()
                    .take(2)
                    .take_while(|byte| byte.is_ascii_digit())
                    .count();
                if digits != 0 && bytes.get(cursor + digits) == Some(&b'$') {
                    argument = bytes[cursor..cursor + digits]
                        .iter()
                        .fold(0, |index, byte| index * 10 + usize::from(*byte - b'0'));
                    cursor += digits + 1;
                }
                selected.push(arguments.get(argument).cloned().unwrap_or(Value::Nil));
                argument += 1;
                // Leave width, precision, conversion validation and formatting to
                // the retained Lua implementation; only argument selection changes.
                while cursor < bytes.len() {
                    let byte = bytes[cursor];
                    rewritten.push(byte);
                    cursor += 1;
                    if !matches!(byte, b'-' | b'+' | b' ' | b'#' | b'0'..=b'9' | b'.') {
                        break;
                    }
                }
            }
            let mut arguments = MultiValue::from_vec(selected);
            arguments.push_front(Value::String(lua.create_string(rewritten)?));
            original.call::<Value>(arguments)
        })?,
    )
}
