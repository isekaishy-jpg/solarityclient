//! Lua bridge between shared minimap scene controls and saved console settings.

use mlua::{AnyUserData, Lua, Table, UserData};

use super::cvars::UiCVarRegistry;
use super::{DIRTY_WIDGET, hidden_key, mark_object_state_changed, xml_attribute};
use crate::UiMinimapState;
use crate::feature::MinimapWidgetState;

static WIDGET_STATE: u8 = 0;

impl UserData for MinimapWidgetState {}

fn widget(object: &Table) -> mlua::Result<AnyUserData> {
    object.raw_get(hidden_key(&WIDGET_STATE))
}

pub(in crate::script) fn snapshot(object: &Table) -> mlua::Result<MinimapWidgetState> {
    Ok(widget(object)?.borrow::<MinimapWidgetState>()?.clone())
}

pub(super) fn initialize(
    lua: &Lua,
    object: &Table,
    node: &crate::UiObjectNode<'_>,
) -> mlua::Result<()> {
    initialize_path(lua, object, xml_player_texture(node).as_deref())
}

pub(in crate::script) fn xml_player_texture(node: &crate::UiObjectNode<'_>) -> Option<String> {
    node.layers()
        .iter()
        .rev()
        .find_map(|layer| xml_attribute(layer.element(), "minimapPlayerTexture"))
        .map(str::to_owned)
}

pub(super) fn initialize_path(lua: &Lua, object: &Table, path: Option<&str>) -> mlua::Result<()> {
    let path = path.unwrap_or("Interface\\Minimap\\MinimapArrow.blp");
    object.raw_set(
        hidden_key(&WIDGET_STATE),
        lua.create_userdata(MinimapWidgetState {
            player_texture: texture_path(path)?,
            player_size: [0.0; 2],
            ping: None,
        })?,
    )
}

fn texture_path(path: &str) -> mlua::Result<solarity_asset::AssetPath> {
    crate::widget::canonical_texture_asset(path).map_err(mlua::Error::external)
}

pub(super) fn register(
    lua: &Lua,
    methods: &Table,
    state: UiMinimapState,
    cvars: UiCVarRegistry,
) -> mlua::Result<()> {
    for (name, axis) in [("SetPlayerTextureWidth", 0), ("SetPlayerTextureHeight", 1)] {
        methods.raw_set(
            name,
            lua.create_function(move |lua, (object, value): (Table, f32)| {
                if !value.is_finite() || value < 0.0 {
                    return Err(mlua::Error::runtime(format!("{name}(): invalid dimension")));
                }
                let data = widget(&object)?;
                let mut widget = data.borrow_mut::<MinimapWidgetState>()?;
                if widget.player_size[axis] != value {
                    widget.player_size[axis] = value;
                    drop(widget);
                    mark_object_state_changed(lua, &object, DIRTY_WIDGET)?;
                }
                Ok(())
            })?,
        )?;
    }
    methods.raw_set(
        "SetPlayerTexture",
        lua.create_function(|lua, (object, path): (Table, String)| {
            let path = texture_path(&path)?;
            let data = widget(&object)?;
            let mut widget = data.borrow_mut::<MinimapWidgetState>()?;
            if widget.player_texture != path {
                widget.player_texture = path;
                drop(widget);
                mark_object_state_changed(lua, &object, DIRTY_WIDGET)?;
            }
            Ok(())
        })?,
    )?;
    let mask_state = state.clone();
    methods.raw_set(
        "SetMaskTexture",
        lua.create_function(move |_, (_object, path): (Table, String)| {
            mask_state.set_mask(texture_path(&path)?);
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "PingLocation",
        lua.create_function(|_, (object, x, y): (Table, f64, f64)| {
            if !x.is_finite() || !y.is_finite() {
                return Err(mlua::Error::runtime("PingLocation(): invalid coordinates"));
            }
            widget(&object)?.borrow_mut::<MinimapWidgetState>()?.ping = Some([x, y]);
            Ok(())
        })?,
    )?;
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
