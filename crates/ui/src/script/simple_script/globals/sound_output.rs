//! Stock game-output enumeration and deferred restart requests.

use mlua::{Lua, Table};

use super::super::{UiGlueMediaAction, UiGlueMediaIntent, UiScriptEnvironment};

/// Resolves the synthetic default followed by the attached physical drivers.
/// 8783B0 localizes index zero and falls back only for absent/empty localization.
pub(super) fn driver_name(
    lua: &Lua,
    state: &UiGlueMediaIntent,
    index: i32,
) -> mlua::Result<Option<String>> {
    let Some(devices) = &state.output_devices else {
        return Ok(None);
    };
    if index <= 0 {
        let localized = lua.globals().raw_get::<Option<String>>("SYSTEM_DEFAULT")?;
        return Ok(Some(
            localized
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| "System Default".to_owned()),
        ));
    }
    // Native 8792B0 performs an unchecked array read here. Keep malformed Lua
    // indices outside that unsafe native boundary and return its empty buffer.
    Ok(devices.get(index as usize - 1).cloned())
}

/// Installs 985C70/985CA0 enumeration and the ordered 985D30 restart operation.
pub(super) fn register(
    lua: &Lua,
    globals: &Table,
    environment: &UiScriptEnvironment,
) -> mlua::Result<()> {
    let state = environment.media_intent();
    globals.raw_set(
        "Sound_GameSystem_GetNumOutputDrivers",
        lua.create_function(move |_, ()| {
            Ok(state
                .borrow()
                .output_devices
                .as_ref()
                .map_or(0, |devices| devices.len() + 1))
        })?,
    )?;
    let state = environment.media_intent();
    globals.raw_set(
        "Sound_GameSystem_GetOutputDriverNameByIndex",
        lua.create_function(move |lua, index: i32| {
            // 4C5AF0 clamps negative indices; 985CA0 returns its zeroed buffer when
            // there is no game output. Both are observable through stock options.
            Ok(driver_name(lua, &state.borrow(), index)?.unwrap_or_default())
        })?,
    )?;
    let state = environment.media_intent();
    globals.raw_set(
        "Sound_GameSystem_RestartSoundSystem",
        lua.create_function(move |_, ()| {
            state
                .borrow_mut()
                .actions
                .push_back(UiGlueMediaAction::RestartSoundSystem);
            Ok(())
        })?,
    )
}
