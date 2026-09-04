//! Lua 5.1 object tables for stock animation groups and primitives.

use std::collections::BTreeMap;
use std::ffi::c_void;

use mlua::{Function, LightUserData, Lua, MultiValue, RegistryKey, Table, Value};

use super::c_simple_anim::{
    UiAnimationHandler, UiAnimationKind, UiAnimationPlan, UiAnimationValue,
};
use crate::script::{OBJECT_REGISTRY, mark_live_state_changed};

const ANIMATION_GROUP_REGISTRY: &str = "solarity.ui.animation_groups";

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
static OWNER_INDEX_TOKEN: u8 = 20;
static ELAPSED_TOKEN: u8 = 21;
static PROGRESS_WITH_DELAY_TOKEN: u8 = 22;
static ALPHA_DELTA_TOKEN: u8 = 23;
static TRANSLATION_X_TOKEN: u8 = 24;
static TRANSLATION_Y_TOKEN: u8 = 25;

/// Lua metatables shared by all retained animation objects.
pub(crate) struct UiAnimationMetatables {
    group: RegistryKey,
    animation: RegistryKey,
    alpha: RegistryKey,
    translation: RegistryKey,
}

/// Builds the stock script method families once per Lua state.
pub(crate) fn create_animation_metatables(lua: &Lua) -> mlua::Result<UiAnimationMetatables> {
    lua.set_named_registry_value(ANIMATION_GROUP_REGISTRY, lua.create_table()?)?;
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
        group_table.raw_set(owner_index_key(), owner)?;
        group_table.raw_set(elapsed_key(), 0.0)?;
        clear_group_contribution(&group_table)?;
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
            animation_table.raw_set(progress_with_delay_key(), 0.0)?;
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
        let groups: Table = lua.named_registry_value(ANIMATION_GROUP_REGISTRY)?;
        groups.raw_set(groups.raw_len() + 1, group_table.clone())?;
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
        lua.create_function(|lua, group: Table| {
            if group.raw_get::<bool>(paused_key())? {
                group.raw_set(playing_key(), true)?;
                group.raw_set(paused_key(), false)?;
                for animation in animation_tables(&group)? {
                    if animation.raw_get::<bool>(paused_key())? {
                        animation.raw_set(playing_key(), true)?;
                        animation.raw_set(paused_key(), false)?;
                    }
                }
                return mark_live_state_changed(lua);
            }
            set_playback_state(&group, true, false, false, 0.0)?;
            group.raw_set(elapsed_key(), 0.0)?;
            reset_group_animations(&group, true)?;
            clear_group_contribution(&group)?;
            mark_live_state_changed(lua)
        })?,
    )?;
    methods.raw_set(
        "Pause",
        lua.create_function(|lua, group: Table| {
            let playing = group.raw_get::<bool>(playing_key())?;
            if playing {
                group.raw_set(playing_key(), false)?;
                group.raw_set(paused_key(), true)?;
                for animation in animation_tables(&group)? {
                    animation.raw_set(playing_key(), false)?;
                    animation.raw_set(paused_key(), true)?;
                }
                mark_live_state_changed(lua)?;
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "Stop",
        lua.create_function(|lua, group: Table| {
            set_playback_state(&group, false, false, false, 0.0)?;
            group.raw_set(elapsed_key(), 0.0)?;
            reset_group_animations(&group, false)?;
            clear_group_contribution(&group)?;
            mark_live_state_changed(lua)
        })?,
    )?;
    methods.raw_set(
        "Finish",
        lua.create_function(|lua, group: Table| {
            set_playback_state(&group, false, false, true, 1.0)?;
            for animation in animation_tables(&group)? {
                set_playback_state(&animation, false, false, true, 1.0)?;
                animation.raw_set(progress_with_delay_key(), 1.0)?;
            }
            call_handler(lua, &group, on_finished_key())?;
            clear_group_contribution(&group)?;
            mark_live_state_changed(lua)
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
        lua.create_function(|lua, animation: Table| {
            if animation.raw_get::<bool>(paused_key())? {
                animation.raw_set(playing_key(), true)?;
                animation.raw_set(paused_key(), false)?;
                return mark_live_state_changed(lua);
            }
            set_playback_state(&animation, true, false, false, 0.0)?;
            animation.raw_set(progress_with_delay_key(), 0.0)?;
            mark_live_state_changed(lua)
        })?,
    )?;
    methods.raw_set(
        "Pause",
        lua.create_function(|lua, animation: Table| {
            if animation.raw_get::<bool>(playing_key())? {
                animation.raw_set(playing_key(), false)?;
                animation.raw_set(paused_key(), true)?;
                mark_live_state_changed(lua)?;
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "Stop",
        lua.create_function(|lua, animation: Table| {
            set_playback_state(&animation, false, false, false, 0.0)?;
            animation.raw_set(progress_with_delay_key(), 0.0)?;
            mark_live_state_changed(lua)
        })?,
    )?;
    methods.raw_set(
        "Finish",
        lua.create_function(|lua, animation: Table| {
            set_playback_state(&animation, false, false, true, 1.0)?;
            animation.raw_set(progress_with_delay_key(), 1.0)?;
            mark_live_state_changed(lua)
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
    methods.raw_set(
        "GetProgressWithDelay",
        lua.create_function(|_, object: Table| {
            if let Some(progress) = object.raw_get::<Option<f64>>(progress_with_delay_key())? {
                Ok(progress)
            } else {
                object.raw_get(progress_key())
            }
        })?,
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

/// Advances every retained group from the rendered-frame clock.
///
/// Build 12340's `CSimpleAnimGroup` tick at `0x0049C350` advances one order
/// band at a time, while `CSimpleAnim` at `0x004985F0` swaps delays in reverse
/// playback and applies smoothing only to the active interval.
pub(crate) fn advance_animations(
    lua: &Lua,
    elapsed_seconds: f64,
) -> mlua::Result<Vec<(usize, UiAnimationTransform)>> {
    if elapsed_seconds == 0.0 {
        return Ok(Vec::new());
    }
    let groups: Table = lua.named_registry_value(ANIMATION_GROUP_REGISTRY)?;
    let retained = groups
        .sequence_values::<Table>()
        .collect::<mlua::Result<Vec<_>>>()?;
    let mut changed_owners = Vec::new();
    for group in &retained {
        if !group.raw_get::<bool>(playing_key())? {
            continue;
        }
        changed_owners.push(group.raw_get::<usize>(owner_index_key())?);
        let total = group_duration(group)?;
        if total <= f64::EPSILON {
            finish_naturally(lua, group)?;
            continue;
        }

        let elapsed = group.raw_get::<f64>(elapsed_key())? + elapsed_seconds;
        let looping = group.raw_get::<String>(looping_key())?;
        let (timeline_time, group_progress, finished) = match looping.as_str() {
            "NONE" if elapsed >= total => (total, 1.0, true),
            "NONE" => (elapsed, elapsed / total, false),
            "REPEAT" => {
                let cycle = elapsed.rem_euclid(total);
                (cycle, cycle / total, false)
            }
            "BOUNCE" => {
                let cycle = elapsed.rem_euclid(total * 2.0);
                if cycle <= total {
                    (cycle, cycle / total, false)
                } else {
                    (total * 2.0 - cycle, (cycle - total) / total, false)
                }
            }
            value => {
                return Err(mlua::Error::runtime(format!(
                    "invalid animation looping mode {value}"
                )));
            }
        };
        group.raw_set(elapsed_key(), elapsed)?;
        group.raw_set(progress_key(), group_progress.clamp(0.0, 1.0))?;
        update_group_contribution(group, timeline_time)?;
        if finished {
            finish_naturally(lua, group)?;
        }
    }
    changed_owners.sort_unstable();
    changed_owners.dedup();
    if changed_owners.is_empty() {
        return Ok(Vec::new());
    }
    let mut transforms = changed_owners
        .iter()
        .copied()
        .map(|owner| (owner, UiAnimationTransform::default()))
        .collect::<Vec<_>>();
    for group in retained {
        let owner = group.raw_get::<usize>(owner_index_key())?;
        let Ok(slot) = changed_owners.binary_search(&owner) else {
            continue;
        };
        let transform = &mut transforms[slot].1;
        transform.active |= group.raw_get::<bool>(playing_key())?;
        transform.alpha_delta += group.raw_get::<f64>(alpha_delta_key())?;
        transform.offset.0 += group.raw_get::<f64>(translation_x_key())?;
        transform.offset.1 += group.raw_get::<f64>(translation_y_key())?;
    }
    Ok(transforms)
}

/// Returns the temporary alpha delta and translation for one owner.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct UiAnimationTransform {
    pub(crate) alpha_delta: f64,
    pub(crate) offset: (f64, f64),
    pub(crate) active: bool,
}

/// Snapshots every owner's temporary animation contribution in one registry pass.
///
/// The object arena is substantially larger than the animation-group arena in
/// stock GlueXML. Indexing contributions once avoids rescanning every Lua group
/// for every object whenever a fade advances.
pub(crate) fn owner_animation_transforms(
    lua: &Lua,
    owner_count: usize,
) -> mlua::Result<Vec<UiAnimationTransform>> {
    let groups: Table = lua.named_registry_value(ANIMATION_GROUP_REGISTRY)?;
    let mut transforms = vec![UiAnimationTransform::default(); owner_count];
    for group in groups.sequence_values::<Table>() {
        let group = group?;
        let owner_index = group.raw_get::<usize>(owner_index_key())?;
        let Some(transform) = transforms.get_mut(owner_index) else {
            continue;
        };
        transform.active |= group.raw_get::<bool>(playing_key())?;
        transform.alpha_delta += group.raw_get::<f64>(alpha_delta_key())?;
        transform.offset.0 += group.raw_get::<f64>(translation_x_key())?;
        transform.offset.1 += group.raw_get::<f64>(translation_y_key())?;
    }
    Ok(transforms)
}

fn update_group_contribution(group: &Table, timeline_time: f64) -> mlua::Result<()> {
    let animations = animation_tables(group)?;
    let mut order_durations = BTreeMap::<u32, f64>::new();
    for animation in &animations {
        let order = animation.raw_get::<u32>(order_key())?;
        let duration = animation.raw_get::<f64>(start_delay_key())?
            + animation.raw_get::<f64>(duration_key())?
            + animation.raw_get::<f64>(end_delay_key())?;
        order_durations
            .entry(order)
            .and_modify(|longest| *longest = longest.max(duration))
            .or_insert(duration);
    }
    let mut order_starts = BTreeMap::new();
    let mut cursor = 0.0;
    for (order, duration) in order_durations {
        order_starts.insert(order, cursor);
        cursor += duration;
    }

    let mut alpha_delta = 0.0;
    let mut translation = (0.0, 0.0);
    for animation in animations {
        let order = animation.raw_get::<u32>(order_key())?;
        let order_start = order_starts[&order];
        let start_delay = animation.raw_get::<f64>(start_delay_key())?;
        let duration = animation.raw_get::<f64>(duration_key())?;
        let end_delay = animation.raw_get::<f64>(end_delay_key())?;
        let complete_duration = start_delay + duration + end_delay;
        let local_time = timeline_time - order_start;
        let progress_with_delay = if complete_duration <= f64::EPSILON {
            f64::from(local_time >= 0.0)
        } else {
            (local_time / complete_duration).clamp(0.0, 1.0)
        };
        let linear_progress = if duration <= f64::EPSILON {
            f64::from(local_time >= start_delay)
        } else {
            ((local_time - start_delay) / duration).clamp(0.0, 1.0)
        };
        let progress = smooth_progress(
            animation.raw_get::<String>(smoothing_key())?.as_str(),
            linear_progress,
        )?;
        let active = local_time >= 0.0 && local_time < complete_duration;
        set_playback_state(
            &animation,
            active,
            false,
            local_time >= complete_duration,
            progress,
        )?;
        animation.raw_set(progress_with_delay_key(), progress_with_delay)?;

        match animation.raw_get::<String>(type_key())?.as_str() {
            "Animation" => {}
            "Alpha" => alpha_delta += animation.raw_get::<f64>(change_key())? * progress,
            "Translation" => {
                translation.0 += animation.raw_get::<f64>(offset_x_key())? * progress;
                translation.1 += animation.raw_get::<f64>(offset_y_key())? * progress;
            }
            kind => {
                return Err(mlua::Error::runtime(format!(
                    "unsupported animation primitive {kind}"
                )));
            }
        }
    }
    group.raw_set(alpha_delta_key(), alpha_delta)?;
    group.raw_set(translation_x_key(), translation.0)?;
    group.raw_set(translation_y_key(), translation.1)
}

fn smooth_progress(smoothing: &str, progress: f64) -> mlua::Result<f64> {
    let half_pi = std::f64::consts::FRAC_PI_2;
    match smoothing {
        "NONE" => Ok(progress),
        "IN" => Ok(1.0 - (progress * half_pi).cos()),
        "OUT" => Ok((progress * half_pi).sin()),
        "IN_OUT" => Ok(0.5 - 0.5 * (progress * std::f64::consts::PI).cos()),
        value => Err(mlua::Error::runtime(format!(
            "invalid animation smoothing mode {value}"
        ))),
    }
}

fn finish_naturally(lua: &Lua, group: &Table) -> mlua::Result<()> {
    set_playback_state(group, false, false, true, 1.0)?;
    for animation in animation_tables(group)? {
        set_playback_state(&animation, false, false, true, 1.0)?;
        animation.raw_set(progress_with_delay_key(), 1.0)?;
    }
    call_handler(lua, group, on_finished_key())?;
    clear_group_contribution(group)
}

fn reset_group_animations(group: &Table, play_first_order: bool) -> mlua::Result<()> {
    let animations = animation_tables(group)?;
    let first_order = animations
        .iter()
        .map(|animation| animation.raw_get::<u32>(order_key()))
        .collect::<mlua::Result<Vec<_>>>()?
        .into_iter()
        .min();
    for animation in animations {
        let order = animation.raw_get::<u32>(order_key())?;
        let playing = play_first_order && first_order == Some(order);
        set_playback_state(&animation, playing, false, false, 0.0)?;
        animation.raw_set(progress_with_delay_key(), 0.0)?;
    }
    Ok(())
}

fn clear_group_contribution(group: &Table) -> mlua::Result<()> {
    group.raw_set(alpha_delta_key(), 0.0)?;
    group.raw_set(translation_x_key(), 0.0)?;
    group.raw_set(translation_y_key(), 0.0)
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
token_key!(owner_index_key, OWNER_INDEX_TOKEN);
token_key!(elapsed_key, ELAPSED_TOKEN);
token_key!(progress_with_delay_key, PROGRESS_WITH_DELAY_TOKEN);
token_key!(alpha_delta_key, ALPHA_DELTA_TOKEN);
token_key!(translation_x_key, TRANSLATION_X_TOKEN);
token_key!(translation_y_key, TRANSLATION_Y_TOKEN);
