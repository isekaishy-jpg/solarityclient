//! Native CSimpleStatusBar script contract and XML loading (0x00961720–0x00971A30).

use mlua::{AnyUserData, Lua, Table, UserData, Value, Variadic};

use super::*;
use crate::widget::{StatusBarConfig, StatusBarState, StatusBarXmlOperation};

static STATE: u8 = 0;
static CONFIG: u8 = 1;
static XML_TEXTURES: u8 = 2;
const PENDING: &str = "solarity.ui.status_bar_updates";

impl UserData for StatusBarState {}
impl UserData for StatusBarConfig {}

fn state(object: &Table) -> mlua::Result<AnyUserData> {
    object.raw_get(hidden_key(&STATE))
}

pub(in crate::script) fn snapshot(object: &Table) -> mlua::Result<(StatusBarState, Option<usize>)> {
    let state = *state(object)?.borrow::<StatusBarState>()?;
    let texture = object
        .raw_get::<Option<Table>>(status_bar_texture_key())?
        .map(|texture| texture.raw_get::<usize>(index_key()).map(|index| index - 1))
        .transpose()?;
    Ok((state, texture))
}

fn queue_update(lua: &Lua, object: &Table) -> mlua::Result<()> {
    let pending = match lua.named_registry_value::<Option<Table>>(PENDING)? {
        Some(pending) => pending,
        None => {
            let pending = lua.create_table()?;
            lua.set_named_registry_value(PENDING, pending.clone())?;
            pending
        }
    };
    pending.raw_set(object.raw_get::<usize>(index_key())?, object.clone())?;
    mark_object_state_changed(lua, object, DIRTY_WIDGET)
}

/// Native widget update runs after scripts, before the renderer consumes regions.
/// A zero fraction hides the texture and preserves its last positive geometry.
pub(super) fn flush_pending(lua: &Lua, ui_extent: (f64, f64)) -> mlua::Result<Vec<usize>> {
    let Some(pending) = lua.named_registry_value::<Option<Table>>(PENDING)? else {
        return Ok(Vec::new());
    };
    let objects = pending
        .clone()
        .pairs::<usize, Table>()
        .collect::<mlua::Result<Vec<_>>>()?;
    let mut changed = Vec::new();
    for (index, object) in objects {
        pending.raw_set(index, Value::Nil)?;
        let data = state(&object)?;
        let fraction = data.borrow::<StatusBarState>()?.fraction();
        if let Some(fraction) = fraction
            && let Some(texture) = object.raw_get::<Option<Table>>(status_bar_texture_key())?
        {
            if fraction > 0.0 {
                let mut state = data.borrow_mut::<StatusBarState>()?;
                state.fill_fraction = Some(fraction);
                state.fill_vertical = state.vertical;
                let vertical = state.vertical;
                drop(state);
                let (width, height) = live_region_dimensions(lua, &object, ui_extent)?;
                let anchors: Table = texture.raw_get(anchors_key())?;
                for point in [
                    UiPoint::TopLeft,
                    UiPoint::TopRight,
                    UiPoint::BottomLeft,
                    UiPoint::BottomRight,
                ] {
                    let offset = if vertical {
                        (
                            0.0,
                            if matches!(point, UiPoint::TopLeft | UiPoint::TopRight) {
                                -(1.0 - fraction) * height
                            } else {
                                0.0
                            },
                        )
                    } else {
                        (
                            if matches!(point, UiPoint::TopRight | UiPoint::BottomRight) {
                                -(1.0 - fraction) * width
                            } else {
                                0.0
                            },
                            0.0,
                        )
                    };
                    anchors.raw_set(
                        point.index() + 1,
                        create_anchor_record(lua, point, Some(index), point, offset)?,
                    )?;
                }
            }
            texture.raw_set(shown_key(), fraction > 0.0)?;
            changed.push(texture.raw_get::<usize>(index_key())? - 1);
        }
    }
    Ok(changed)
}

pub(super) fn initialize(lua: &Lua, object: &Table, config: StatusBarConfig) -> mlua::Result<()> {
    object.raw_set(
        hidden_key(&STATE),
        lua.create_userdata(StatusBarState::default())?,
    )?;
    object.raw_set(hidden_key(&CONFIG), lua.create_userdata(config)?)?;
    object.raw_set(hidden_key(&XML_TEXTURES), lua.create_table()?)?;
    object.raw_set(status_bar_texture_key(), Value::Nil)
}

pub(super) fn collect_xml_texture(owner: &Table, texture: &Table) -> mlua::Result<()> {
    let textures: Table = owner.raw_get(hidden_key(&XML_TEXTURES))?;
    textures.raw_push(texture.clone())
}

pub(super) fn load_xml(lua: &Lua, object: &Table) -> mlua::Result<()> {
    let config: AnyUserData = object.raw_get(hidden_key(&CONFIG))?;
    let operations = config.borrow::<StatusBarConfig>()?.0.clone();
    let textures: Table = object.raw_get(hidden_key(&XML_TEXTURES))?;
    let mut next_texture = 1;
    for operation in operations {
        match operation {
            StatusBarXmlOperation::Texture(layer) => {
                let texture = textures.raw_get::<Table>(next_texture)?;
                next_texture += 1;
                set_texture(lua, object, Some(texture), &layer)?;
            }
            StatusBarXmlOperation::Color(color) => set_color(lua, object, color)?,
            StatusBarXmlOperation::Range(min, max) => set_range(lua, object, min, max)?,
            StatusBarXmlOperation::Value(value) => set_value(lua, object, value)?,
            StatusBarXmlOperation::Orientation(vertical) => set_orientation(lua, object, vertical)?,
            StatusBarXmlOperation::RotatesTexture(rotates) => {
                set_rotates_texture(lua, object, rotates)?
            }
        }
    }
    object.raw_set(hidden_key(&CONFIG), Value::Nil)?;
    object.raw_set(hidden_key(&XML_TEXTURES), Value::Nil)
}

fn set_value(lua: &Lua, object: &Table, value: f32) -> mlua::Result<()> {
    let data = state(object)?;
    {
        let mut state = data.borrow_mut::<StatusBarState>()?;
        if !state.range_valid {
            return Ok(());
        }
        let value = value.clamp(state.minimum, state.maximum);
        if state.value_valid && state.value == value {
            return Ok(());
        }
        state.value = value;
        state.value_valid = true;
    }
    queue_update(lua, object)?;
    if let Some(function) = object_script_function(lua, object, UiScriptHandler::ValueChanged)? {
        let value = data.borrow::<StatusBarState>()?.value;
        call_number_object_handler(lua, &function, object.clone(), f64::from(value))?;
    }
    Ok(())
}

fn set_range(lua: &Lua, object: &Table, minimum: f32, maximum: f32) -> mlua::Result<()> {
    let minimum = minimum.min(maximum);
    let data = state(object)?;
    {
        let mut state = data.borrow_mut::<StatusBarState>()?;
        if state.range_valid && state.minimum == minimum && state.maximum == maximum {
            return Ok(());
        }
        state.minimum = minimum;
        state.maximum = maximum;
        state.range_valid = true;
    }
    queue_update(lua, object)?;
    if let Some(function) = object_script_function(lua, object, UiScriptHandler::MinMaxChanged)? {
        call_two_number_object_handler(
            lua,
            &function,
            object.clone(),
            f64::from(minimum),
            f64::from(maximum),
        )?;
    }
    // Read again after the callback: scripts may replace both range and value.
    let current = *data.borrow::<StatusBarState>()?;
    if current.value_valid {
        set_value(lua, object, current.value)?;
    }
    Ok(())
}

fn set_orientation(lua: &Lua, object: &Table, vertical: bool) -> mlua::Result<()> {
    state(object)?.borrow_mut::<StatusBarState>()?.vertical = vertical;
    queue_update(lua, object)
}

fn packed_color_component(value: f64) -> f64 {
    let packed = (f64::from(value.clamp(0.0, 1.0) as f32) * 255.0 + 0.5) as u8;
    f64::from(f32::from(packed) * (1.0_f32 / 255.0))
}

fn set_color(lua: &Lua, object: &Table, color: [f64; 4]) -> mlua::Result<()> {
    if let Some(texture) = object.raw_get::<Option<Table>>(status_bar_texture_key())? {
        let color = color.map(packed_color_component);
        texture.raw_set(
            texture_color_key(),
            lua.create_sequence_from(color.into_iter().cycle().take(16))?,
        )?;
        mark_object_state_changed(lua, &texture, DIRTY_TEXTURE_VERTEX_COLOR)?;
    }
    Ok(())
}

fn set_rotates_texture(lua: &Lua, object: &Table, rotates: bool) -> mlua::Result<()> {
    state(object)?
        .borrow_mut::<StatusBarState>()?
        .rotates_texture = rotates;
    if let Some(texture) = object.raw_get::<Option<Table>>(status_bar_texture_key())? {
        let coords = if rotates {
            [0., 1., 1., 1., 0., 0., 1., 0.]
        } else {
            [0., 0., 0., 1., 1., 0., 1., 1.]
        };
        texture.raw_set(tex_coord_key(), lua.create_sequence_from(coords)?)?;
        mark_object_state_changed(lua, &texture, DIRTY_TEXTURE)?;
    }
    queue_update(lua, object)
}

fn draw_layer(value: &str) -> &'static str {
    let value = value.split('\0').next().unwrap_or_default();
    ["BACKGROUND", "BORDER", "ARTWORK", "OVERLAY", "HIGHLIGHT"]
        .into_iter()
        .find(|layer| layer.eq_ignore_ascii_case(value))
        .unwrap_or("ARTWORK")
}

fn set_texture(lua: &Lua, owner: &Table, texture: Option<Table>, layer: &str) -> mlua::Result<()> {
    let previous = owner.raw_get::<Option<Table>>(status_bar_texture_key())?;
    if previous == texture {
        return Ok(());
    }
    if let Some(previous) = previous {
        // Keep arena indices stable while retiring the replaced native region.
        let index = previous.raw_get::<usize>(index_key())?;
        let parent = previous.raw_get::<Option<usize>>(parent_key())?;
        previous.raw_set(shown_key(), false)?;
        previous.raw_set(texture_file_key(), Value::Nil)?;
        previous.raw_set(texture_solid_color_key(), Value::Nil)?;
        previous.raw_set(parent_key(), Value::Nil)?;
        previous.raw_set(anchors_key(), lua.create_table()?)?;
        previous.raw_set(role_key(), "object")?;
        move_child_relation(lua, index, parent, None)?;
        if let Some(name) = previous.raw_get::<Option<String>>(name_key())?
            && lua.globals().raw_get::<Value>(name.as_str())? == Value::Table(previous.clone())
        {
            lua.globals().raw_set(name.as_str(), Value::Nil)?;
        }
    }
    if let Some(texture) = texture.as_ref() {
        let owner_index = owner.raw_get::<usize>(index_key())?;
        let texture_index = texture.raw_get::<usize>(index_key())?;
        let previous_parent = texture.raw_get::<Option<usize>>(parent_key())?;
        texture.raw_set(parent_key(), owner_index)?;
        move_child_relation(lua, texture_index, previous_parent, Some(owner_index))?;
        texture.raw_set(role_key(), "bar_texture")?;
        texture.raw_set(draw_layer_key(), draw_layer(layer))?;
        let anchors = lua.create_table()?;
        for point in [
            UiPoint::TopLeft,
            UiPoint::TopRight,
            UiPoint::BottomLeft,
            UiPoint::BottomRight,
        ] {
            anchors.raw_set(
                point.index() + 1,
                create_anchor_record(lua, point, Some(owner_index), point, (0., 0.))?,
            )?;
        }
        texture.raw_set(anchors_key(), anchors)?;
    }
    owner.raw_set(status_bar_texture_key(), texture)?;
    state(owner)?.borrow_mut::<StatusBarState>()?.fill_fraction = None;
    queue_update(lua, owner)?;
    mark_live_state_changed(lua)
}

pub(super) fn register(
    lua: &Lua,
    methods: &Table,
    dynamic_arena: DynamicArenaState,
) -> mlua::Result<()> {
    methods.raw_set(
        "GetValue",
        lua.create_function(|_, object: Table| {
            Ok(state(&object)?.borrow::<StatusBarState>()?.value)
        })?,
    )?;
    methods.raw_set(
        "SetValue",
        lua.create_function(|lua, (object, value): (Table, f32)| set_value(lua, &object, value))?,
    )?;
    methods.raw_set(
        "GetMinMaxValues",
        lua.create_function(|_, object: Table| {
            let data = state(&object)?;
            let state = data.borrow::<StatusBarState>()?;
            Ok((state.minimum, state.maximum))
        })?,
    )?;
    methods.raw_set(
        "SetMinMaxValues",
        lua.create_function(|lua, (object, minimum, maximum): (Table, f32, f64)| {
            if !StatusBarState::valid_range(minimum, maximum) {
                return Err(mlua::Error::runtime("SetMinMaxValues: range out of bounds"));
            }
            set_range(lua, &object, minimum, maximum as f32)
        })?,
    )?;
    methods.raw_set(
        "GetOrientation",
        lua.create_function(|_, object: Table| {
            Ok(if state(&object)?.borrow::<StatusBarState>()?.vertical {
                "VERTICAL"
            } else {
                "HORIZONTAL"
            })
        })?,
    )?;
    methods.raw_set(
        "SetOrientation",
        lua.create_function(|lua, (object, axis): (Table, String)| {
            if axis.eq_ignore_ascii_case("VERTICAL") {
                set_orientation(lua, &object, true)
            } else if axis.eq_ignore_ascii_case("HORIZONTAL") {
                set_orientation(lua, &object, false)
            } else {
                Err(mlua::Error::runtime(
                    "SetOrientation expects HORIZONTAL or VERTICAL",
                ))
            }
        })?,
    )?;
    methods.raw_set(
        "SetStatusBarColor",
        lua.create_function(|lua, (object, args): (Table, Variadic<Value>)| {
            let mut color = [0.0, 0.0, 0.0, 1.0];
            for (index, component) in color.iter_mut().enumerate() {
                if let Some(value) = args.get(index) {
                    *component = lua.coerce_number(value.clone())?.unwrap_or(*component);
                }
            }
            set_color(lua, &object, color)
        })?,
    )?;
    methods.raw_set(
        "GetStatusBarColor",
        lua.create_function(|_, object: Table| {
            if let Some(texture) = object.raw_get::<Option<Table>>(status_bar_texture_key())? {
                let color: Table = texture.raw_get(texture_color_key())?;
                Ok((
                    packed_color_component(color.raw_get::<f64>(1)?),
                    packed_color_component(color.raw_get::<f64>(2)?),
                    packed_color_component(color.raw_get::<f64>(3)?),
                    packed_color_component(color.raw_get::<f64>(4)?),
                ))
            } else {
                Ok((1., 1., 1., 1.))
            }
        })?,
    )?;
    methods.raw_set(
        "GetRotatesTexture",
        lua.create_function(|_, object: Table| {
            Ok(state(&object)?
                .borrow::<StatusBarState>()?
                .rotates_texture
                .then_some(1_u8))
        })?,
    )?;
    methods.raw_set(
        "SetRotatesTexture",
        lua.create_function(|lua, (object, args): (Table, Variadic<Value>)| {
            set_rotates_texture(lua, &object, native_optional_bool(args.first(), false))
        })?,
    )?;
    methods.raw_set(
        "GetStatusBarTexture",
        lua.create_function(|_, object: Table| {
            object.raw_get::<Option<Table>>(status_bar_texture_key())
        })?,
    )?;
    methods.raw_set(
        "SetStatusBarTexture",
        lua.create_function(move |lua, (object, value, layer): (Table, Value, Value)| {
            let layer = lua.coerce_string(layer)?;
            let layer = layer
                .as_ref()
                .map(|value| value.to_string_lossy())
                .unwrap_or_default();
            let layer = draw_layer(&layer);
            let texture = match value {
                Value::Nil => None,
                Value::Table(texture) if texture.raw_get::<String>(type_key())? == "Texture" => {
                    Some(texture)
                }
                value => {
                    let path = lua
                        .coerce_string(value)?
                        .ok_or_else(|| {
                            mlua::Error::runtime("SetStatusBarTexture expects filename or Texture")
                        })?
                        .to_string_lossy();
                    let texture = match object.raw_get::<Option<Table>>(status_bar_texture_key())? {
                        Some(texture) => texture,
                        None => create_dynamic_region(
                            lua,
                            "Texture",
                            object.clone(),
                            None,
                            Some(layer),
                            None,
                            None,
                            &dynamic_arena.counters(),
                        )?,
                    };
                    texture.raw_set(texture_file_key(), (!path.is_empty()).then_some(path))?;
                    texture.raw_set(texture_solid_color_key(), Value::Nil)?;
                    mark_object_state_changed(lua, &texture, DIRTY_TEXTURE)?;
                    Some(texture)
                }
            };
            set_texture(lua, &object, texture, layer)
        })?,
    )?;
    Ok(())
}
