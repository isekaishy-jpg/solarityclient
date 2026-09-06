//! Lua bridge between shared minimap scene controls and saved console settings.

use mlua::{Lua, Table};

use super::cvars::UiCVarRegistry;
use crate::UiMinimapState;

pub(super) fn register(
    lua: &Lua,
    methods: &Table,
    state: UiMinimapState,
    cvars: UiCVarRegistry,
) -> mlua::Result<()> {
    let zoom_state = state.clone();
    methods.raw_set(
        "GetZoom",
        lua.create_function(move |_, _minimap: Table| Ok(zoom_state.zoom()))?,
    )?;
    methods.raw_set(
        "GetZoomLevels",
        lua.create_function(|_, _minimap: Table| Ok(UiMinimapState::ZOOM_LEVELS))?,
    )?;
    methods.raw_set(
        "SetZoom",
        lua.create_function(move |_, (_minimap, zoom): (Table, f64)| {
            let zoom = native_zoom_index(zoom);
            if state.zoom() != zoom {
                cvars
                    .set(state.zoom_cvar(), zoom.to_string())
                    .map_err(|_| {
                        mlua::Error::runtime("minimap scene is missing its native zoom CVar")
                    })?;
                state.set_zoom(zoom);
            }
            Ok(())
        })?,
    )
}

/// FUN_0057BFD0 uses truncating x87 FISTP to an i64, passes its low unsigned
/// word, and FUN_007F3AE0 clamps to five. Invalid FISTP input produces the
/// integer-indefinite value i64::MIN, whose low word is zero.
pub(super) fn native_zoom_index(value: f64) -> u32 {
    let integer = if (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&value) {
        value.trunc() as i64
    } else {
        i64::MIN
    };
    (integer as u32).min(UiMinimapState::ZOOM_LEVELS - 1)
}
