//! Retained `Minimap` widget properties and Lua methods.

use std::ffi::c_void;

use mlua::{LightUserData, Lua, Table};

static PLAYER_TEXTURE_WIDTH_TOKEN: u8 = 1;
static PLAYER_TEXTURE_HEIGHT_TOKEN: u8 = 2;
static PING_X_TOKEN: u8 = 5;
static PING_Y_TOKEN: u8 = 6;

/// Initializes native minimap presentation state before XML `OnLoad`.
pub(crate) fn initialize_state(minimap: &Table) -> mlua::Result<()> {
    minimap.raw_set(player_texture_width_key(), 0.0)?;
    minimap.raw_set(player_texture_height_key(), 0.0)?;
    minimap.raw_set(ping_x_key(), Option::<f64>::None)?;
    minimap.raw_set(ping_y_key(), Option::<f64>::None)
}

/// Adds the build-12340 minimap-specific script method family.
pub(crate) fn register_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    register_dimension_setter(
        lua,
        methods,
        "PlayerTextureWidth",
        player_texture_width_key(),
    )?;
    register_dimension_setter(
        lua,
        methods,
        "PlayerTextureHeight",
        player_texture_height_key(),
    )?;
    methods.raw_set(
        "PingLocation",
        lua.create_function(|_, (minimap, x, y): (Table, f64, f64)| {
            require_nonnegative_or_signed_finite("minimap ping X", x, false)?;
            require_nonnegative_or_signed_finite("minimap ping Y", y, false)?;
            minimap.raw_set(ping_x_key(), x)?;
            minimap.raw_set(ping_y_key(), y)
        })?,
    )?;
    Ok(())
}

fn register_dimension_setter(
    lua: &Lua,
    methods: &Table,
    suffix: &'static str,
    key: LightUserData,
) -> mlua::Result<()> {
    methods.raw_set(
        format!("Set{suffix}"),
        lua.create_function(move |_, (minimap, value): (Table, f64)| {
            require_nonnegative_or_signed_finite(suffix, value, true)?;
            minimap.raw_set(key, value)
        })?,
    )
}

fn require_nonnegative_or_signed_finite(
    label: &str,
    value: f64,
    nonnegative: bool,
) -> mlua::Result<()> {
    if !value.is_finite() || nonnegative && value < 0.0 {
        return Err(mlua::Error::runtime(format!(
            "invalid {label} value {value}"
        )));
    }
    Ok(())
}

macro_rules! token_key {
    ($name:ident, $token:ident) => {
        fn $name() -> LightUserData {
            LightUserData(std::ptr::from_ref(&$token).cast::<c_void>().cast_mut())
        }
    };
}

token_key!(player_texture_width_key, PLAYER_TEXTURE_WIDTH_TOKEN);
token_key!(player_texture_height_key, PLAYER_TEXTURE_HEIGHT_TOKEN);
token_key!(ping_x_key, PING_X_TOKEN);
token_key!(ping_y_key, PING_Y_TOKEN);
