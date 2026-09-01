//! Lua 5.1 object tables for stock animation groups and primitives.

use std::collections::BTreeMap;
use std::ffi::c_void;

use mlua::{Function, LightUserData, Lua, MultiValue, RegistryKey, Table, Value};

use super::c_simple_anim::{
    UiAnimationHandler, UiAnimationKind, UiAnimationPlan, UiAnimationValue,
};
use crate::script::OBJECT_REGISTRY;

static NAME_TOKEN: u8 = 1;
static TYPE_TOKEN: u8 = 2;
static PARENT_TOKEN: u8 = 3;
static PLAYING_TOKEN: u8 = 4;
static PAUSED_TOKEN: u8 = 5;
static DONE_TOKEN: u8 = 6;
static PROGRESS_TOKEN: u8 = 7;
static LOOPING_TOKEN: u8 = 8;
static ANIMATIONS_TOKEN: u8 = 9;
static DURATION_TOKEN: u8 = 10;
static START_DELAY_TOKEN: u8 = 11;
static END_DELAY_TOKEN: u8 = 12;
static ORDER_TOKEN: u8 = 13;
static SMOOTHING_TOKEN: u8 = 14;
static CHANGE_TOKEN: u8 = 15;
static OFFSET_X_TOKEN: u8 = 16;
static OFFSET_Y_TOKEN: u8 = 17;
static ON_FINISHED_TOKEN: u8 = 18;
static ON_LOAD_TOKEN: u8 = 19;

/// Lua metatables shared by all retained animation objects.
pub(crate) struct UiAnimationMetatables {
    group: RegistryKey,
    animation: RegistryKey,
    alpha: RegistryKey,
    translation: RegistryKey,
}

/// Builds the stock script method families once per Lua state.
pub(crate) fn create_animation_metatables(lua: &Lua) -> mlua::Result<UiAnimationMetatables> {
    Ok(UiAnimationMetatables {
        group: lua.create_registry_value(create_group_metatable(lua)?)?,
        animation: lua
            .create_registry_value(create_primitive_metatable(lua, UiAnimationKind::Animation)?)?,
        alpha: lua
            .create_registry_value(create_primitive_metatable(lua, UiAnimationKind::Alpha)?)?,
        translation: lua.create_registry_value(create_primitive_metatable(
            lua,
            UiAnimationKind::Translation,
        )?)?,
    })
}

/// Registers the groups owned by one frame before that frame's `OnLoad`.
pub(crate) fn register_owner_animations(
    lua: &Lua,
    plan: &UiAnimationPlan,
    owner: usize,
    metatables: &UiAnimationMetatables,
) -> mlua::Result<()> {
    let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
    let owner_table: Table = objects.raw_get(owner + 1)?;

    for group_index in plan.groups_for_owner(owner) {
        let group = plan
            .groups()
            .get(*group_index)
            .ok_or_else(|| mlua::Error::runtime("animation group index is outside the arena"))?;
        let group_table = lua.create_table()?;
        group_table.raw_set(name_key(), group.name())?;
        group_table.raw_set(type_key(), "AnimationGroup")?;
        group_table.raw_set(parent_key(), owner_table.clone())?;
        group_table.raw_set(playing_key(), false)?;
        group_table.raw_set(paused_key(), false)?;
        group_table.raw_set(done_key(), false)?;
        group_table.raw_set(progress_key(), 0.0)?;
        group_table.raw_set(looping_key(), group.looping().as_str())?;
        group_table.raw_set(animations_key(), lua.create_table()?)?;
        if let Some(handler) = group.on_load() {
            group_table.raw_set(on_load_key(), animation_handler(lua, handler)?)?;
        }
        if let Some(handler) = group.on_finished() {
            group_table.raw_set(on_finished_key(), animation_handler(lua, handler)?)?;
        }
        group_table.set_metatable(Some(lua.registry_value(&metatables.group)?))?;
        publish(
            lua,
            &owner_table,
            &group_table,
            group.name(),
            group.parent_key(),
        )?;

        let animation_tables: Table = group_table.raw_get(animations_key())?;
        for (sequence, animation_index) in group.animation_range().enumerate() {
            let animation = plan.animations().get(animation_index).ok_or_else(|| {
                mlua::Error::runtime("animation primitive index is outside the arena")
            })?;
            let animation_table = lua.create_table()?;
            animation_table.raw_set(name_key(), animation.name())?;
            animation_table.raw_set(type_key(), animation.kind().object_type())?;
            animation_table.raw_set(parent_key(), group_table.clone())?;
            animation_table.raw_set(playing_key(), false)?;
            animation_table.raw_set(paused_key(), false)?;
            animation_table.raw_set(done_key(), false)?;
            animation_table.raw_set(progress_key(), 0.0)?;
            animation_table.raw_set(duration_key(), animation.duration())?;
            animation_table.raw_set(start_delay_key(), animation.start_delay())?;
            animation_table.raw_set(end_delay_key(), animation.end_delay())?;
            animation_table.raw_set(order_key(), animation.order())?;
            animation_table.raw_set(smoothing_key(), animation.smoothing())?;
            match animation.value() {
                UiAnimationValue::Timing => {}
                UiAnimationValue::Alpha { change } => {
                    animation_table.raw_set(change_key(), change)?;
                }
                UiAnimationValue::Translation { offset } => {
                    animation_table.raw_set(offset_x_key(), offset.0)?;
                    animation_table.raw_set(offset_y_key(), offset.1)?;
                }
            }
            let metatable = match animation.kind() {
                UiAnimationKind::Animation => &metatables.animation,
                UiAnimationKind::Alpha => &metatables.alpha,
                UiAnimationKind::Translation => &metatables.translation,
            };
            animation_table.set_metatable(Some(lua.registry_value(metatable)?))?;
            animation_tables.raw_set(sequence + 1, animation_table.clone())?;
            publish(
                lua,
                &group_table,
                &animation_table,
                animation.name(),
                animation.parent_key(),
            )?;
        }
        call_handler(lua, &group_table, on_load_key())?;
    }
    Ok(())
}

fn publish(
    lua: &Lua,
    parent: &Table,
    object: &Table,
    name: Option<&str>,
    parent_field: Option<&str>,
) -> mlua::Result<()> {
    if let Some(name) = name {
        lua.globals().raw_set(name, object.clone())?;
    }
    if let Some(parent_field) = parent_field {
        parent.raw_set(parent_field, object.clone())?;
    }
    Ok(())
}

fn animation_handler(lua: &Lua, handler: &UiAnimationHandler) -> mlua::Result<Value> {
    match handler {
        UiAnimationHandler::Global(name) => Ok(Value::String(lua.create_string(name)?)),
        UiAnimationHandler::Inline(source) => {
            let wrapper = format!("return function(self)\n{source}\nend");
            lua.load(&wrapper).eval::<Function>().map(Value::Function)
        }
    }
}

fn create_group_metatable(lua: &Lua) -> mlua::Result<Table> {
    let methods = lua.create_table()?;
    add_identity_methods(lua, &methods)?;
    methods.raw_set(
        "Play",
        lua.create_function(|_, group: Table| {
            set_playback_state(&group, true, false, false, 0.0)?;
            for animation in animation_tables(&group)? {
                set_playback_state(&animation, true, false, false, 0.0)?;
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "Pause",
        lua.create_function(|_, group: Table| {
            let playing = group.raw_get::<bool>(playing_key())?;
            if playing {
                group.raw_set(playing_key(), false)?;
                group.raw_set(paused_key(), true)?;
                for animation in animation_tables(&group)? {
                    animation.raw_set(playing_key(), false)?;
                    animation.raw_set(paused_key(), true)?;
                }
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "Stop",
        lua.create_function(|_, group: Table| {
            set_playback_state(&group, false, false, false, 0.0)?;
            for animation in animation_tables(&group)? {
                set_playback_state(&animation, false, false, false, 0.0)?;
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "Finish",
        lua.create_function(|lua, group: Table| {
            set_playback_state(&group, false, false, true, 1.0)?;
            for animation in animation_tables(&group)? {
                set_playback_state(&animation, false, false, true, 1.0)?;
            }
            call_handler(lua, &group, on_finished_key())
        })?,
    )?;
    add_playback_queries(lua, &methods)?;
    methods.raw_set(
        "GetLooping",
        lua.create_function(|_, group: Table| group.raw_get::<String>(looping_key()))?,
    )?;
    methods.raw_set(
        "SetLooping",
        lua.create_function(|_, (group, looping): (Table, String)| {
            if !matches!(looping.as_str(), "NONE" | "REPEAT" | "BOUNCE") {
                return Err(mlua::Error::runtime(format!(
                    "invalid animation looping mode {looping}"
                )));
            }
            group.raw_set(looping_key(), looping)
        })?,
    )?;
    methods.raw_set(
        "GetAnimations",
        lua.create_function(|_, group: Table| {
            let values = animation_tables(&group)?
                .into_iter()
                .map(Value::Table)
                .collect::<Vec<_>>();
            Ok(MultiValue::from_vec(values))
        })?,
    )?;
    methods.raw_set(
        "GetDuration",
        lua.create_function(|_, group: Table| group_duration(&group))?,
    )?;
    let metatable = lua.create_table()?;
    metatable.raw_set("__index", methods)?;
    Ok(metatable)
}

fn create_primitive_metatable(lua: &Lua, kind: UiAnimationKind) -> mlua::Result<Table> {
    let methods = lua.create_table()?;
    add_identity_methods(lua, &methods)?;
    add_playback_methods(lua, &methods)?;
    add_playback_queries(lua, &methods)?;
    add_number_property(lua, &methods, "Duration", duration_key(), true)?;
    add_number_property(lua, &methods, "StartDelay", start_delay_key(), true)?;
    add_number_property(lua, &methods, "EndDelay", end_delay_key(), true)?;
    methods.raw_set(
        "GetOrder",
        lua.create_function(|_, animation: Table| animation.raw_get::<u32>(order_key()))?,
    )?;
    methods.raw_set(
        "SetOrder",
        lua.create_function(|_, (animation, order): (Table, u32)| {
            if order == 0 {
                return Err(mlua::Error::runtime("animation order must be positive"));
            }
            animation.raw_set(order_key(), order)
        })?,
    )?;
    methods.raw_set(
        "GetSmoothing",
        lua.create_function(|_, animation: Table| animation.raw_get::<String>(smoothing_key()))?,
    )?;
    methods.raw_set(
        "SetSmoothing",
        lua.create_function(|_, (animation, smoothing): (Table, String)| {
            if !matches!(smoothing.as_str(), "NONE" | "IN" | "OUT" | "IN_OUT") {
                return Err(mlua::Error::runtime(format!(
                    "invalid animation smoothing mode {smoothing}"
                )));
            }
            animation.raw_set(smoothing_key(), smoothing)
        })?,
    )?;
    methods.raw_set(
        "GetAnimationGroup",
        lua.create_function(|_, animation: Table| animation.raw_get::<Table>(parent_key()))?,
    )?;
    match kind {
        UiAnimationKind::Animation => {}
        UiAnimationKind::Alpha => {
            add_number_property(lua, &methods, "Change", change_key(), false)?;
        }
        UiAnimationKind::Translation => {
            methods.raw_set(
                "GetOffset",
                lua.create_function(|_, animation: Table| {
                    Ok((
                        animation.raw_get::<f64>(offset_x_key())?,
                        animation.raw_get::<f64>(offset_y_key())?,
                    ))
                })?,
            )?;
            methods.raw_set(
                "SetOffset",
                lua.create_function(|_, (animation, x, y): (Table, f64, f64)| {
                    require_finite("animation X offset", x, false)?;
                    require_finite("animation Y offset", y, false)?;
                    animation.raw_set(offset_x_key(), x)?;
                    animation.raw_set(offset_y_key(), y)
                })?,
            )?;
        }
    }
    let metatable = lua.create_table()?;
    metatable.raw_set("__index", methods)?;
    Ok(metatable)
}

fn add_identity_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "GetName",
        lua.create_function(|_, object: Table| object.raw_get::<Option<String>>(name_key()))?,
    )?;
    methods.raw_set(
        "GetObjectType",
        lua.create_function(|_, object: Table| object.raw_get::<String>(type_key()))?,
    )?;
    methods.raw_set(
        "IsObjectType",
        lua.create_function(|_, (object, candidate): (Table, String)| {
            Ok(object.raw_get::<String>(type_key())? == candidate)
        })?,
    )?;
    methods.raw_set(
        "GetParent",
        lua.create_function(|_, object: Table| object.raw_get::<Table>(parent_key()))?,
    )?;
    Ok(())
}

fn add_playback_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "Play",
        lua.create_function(|_, animation: Table| {
            set_playback_state(&animation, true, false, false, 0.0)
        })?,
    )?;
    methods.raw_set(
        "Pause",
        lua.create_function(|_, animation: Table| {
            if animation.raw_get::<bool>(playing_key())? {
                animation.raw_set(playing_key(), false)?;
                animation.raw_set(paused_key(), true)?;
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "Stop",
        lua.create_function(|_, animation: Table| {
            set_playback_state(&animation, false, false, false, 0.0)
        })?,
    )?;
    methods.raw_set(
        "Finish",
        lua.create_function(|_, animation: Table| {
            set_playback_state(&animation, false, false, true, 1.0)
        })?,
    )?;
    Ok(())
}

fn add_playback_queries(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "IsPlaying",
        lua.create_function(|_, object: Table| object.raw_get::<bool>(playing_key()))?,
    )?;
    methods.raw_set(
        "IsPaused",
        lua.create_function(|_, object: Table| object.raw_get::<bool>(paused_key()))?,
    )?;
    methods.raw_set(
        "IsDone",
        lua.create_function(|_, object: Table| object.raw_get::<bool>(done_key()))?,
    )?;
    methods.raw_set(
        "GetProgress",
        lua.create_function(|_, object: Table| object.raw_get::<f64>(progress_key()))?,
    )?;
    Ok(())
}

fn add_number_property(
    lua: &Lua,
    methods: &Table,
    suffix: &'static str,
    key: LightUserData,
    nonnegative: bool,
) -> mlua::Result<()> {
    methods.raw_set(
        format!("Get{suffix}"),
        lua.create_function(move |_, object: Table| object.raw_get::<f64>(key))?,
    )?;
    methods.raw_set(
        format!("Set{suffix}"),
        lua.create_function(move |_, (object, value): (Table, f64)| {
            require_finite(suffix, value, nonnegative)?;
            object.raw_set(key, value)
        })?,
    )?;
    Ok(())
}

fn set_playback_state(
    object: &Table,
    playing: bool,
    paused: bool,
    done: bool,
    progress: f64,
) -> mlua::Result<()> {
    object.raw_set(playing_key(), playing)?;
    object.raw_set(paused_key(), paused)?;
    object.raw_set(done_key(), done)?;
    object.raw_set(progress_key(), progress)
}

fn animation_tables(group: &Table) -> mlua::Result<Vec<Table>> {
    let animations: Table = group.raw_get(animations_key())?;
    animations.sequence_values::<Table>().collect()
}

fn group_duration(group: &Table) -> mlua::Result<f64> {
    let mut orders = BTreeMap::<u32, f64>::new();
    for animation in animation_tables(group)? {
        let order = animation.raw_get::<u32>(order_key())?;
        let duration = animation.raw_get::<f64>(start_delay_key())?
            + animation.raw_get::<f64>(duration_key())?
            + animation.raw_get::<f64>(end_delay_key())?;
        orders
            .entry(order)
            .and_modify(|longest| *longest = longest.max(duration))
            .or_insert(duration);
    }
    Ok(orders.values().sum())
}

fn call_handler(lua: &Lua, group: &Table, key: LightUserData) -> mlua::Result<()> {
    match group.raw_get::<Value>(key)? {
        Value::Nil => Ok(()),
        Value::Function(function) => function.call(group.clone()),
        Value::String(name) => {
            let function: Function = lua.globals().raw_get(name.to_str()?)?;
            function.call(group.clone())
        }
        _ => Err(mlua::Error::runtime(
            "animation OnFinished target has an invalid runtime type",
        )),
    }
}

fn require_finite(label: &str, value: f64, nonnegative: bool) -> mlua::Result<()> {
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

token_key!(name_key, NAME_TOKEN);
token_key!(type_key, TYPE_TOKEN);
token_key!(parent_key, PARENT_TOKEN);
token_key!(playing_key, PLAYING_TOKEN);
token_key!(paused_key, PAUSED_TOKEN);
token_key!(done_key, DONE_TOKEN);
token_key!(progress_key, PROGRESS_TOKEN);
token_key!(looping_key, LOOPING_TOKEN);
token_key!(animations_key, ANIMATIONS_TOKEN);
token_key!(duration_key, DURATION_TOKEN);
token_key!(start_delay_key, START_DELAY_TOKEN);
token_key!(end_delay_key, END_DELAY_TOKEN);
token_key!(order_key, ORDER_TOKEN);
token_key!(smoothing_key, SMOOTHING_TOKEN);
token_key!(change_key, CHANGE_TOKEN);
token_key!(offset_x_key, OFFSET_X_TOKEN);
token_key!(offset_y_key, OFFSET_Y_TOKEN);
token_key!(on_finished_key, ON_FINISHED_TOKEN);
token_key!(on_load_key, ON_LOAD_TOKEN);
