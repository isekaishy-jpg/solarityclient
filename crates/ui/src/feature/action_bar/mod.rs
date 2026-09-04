//! Action-slot layout, input, cooldown, and presentation behavior evidenced by `ActionBarFrame.cpp`.

mod action_bar_frame;
mod state;

use mlua::{Lua, Table};

pub use state::{
    UI_ACTION_SLOT_COUNT, UiActionBarPageError, UiActionBarState, UiActionBarStateError,
};

/// Registers globals backed by client-owned action-bar state.
pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    state: UiActionBarState,
) -> mlua::Result<()> {
    let change_state = state.clone();
    let toggle_query = state.clone();
    let toggle_update = state.clone();
    let bonus_bar_state = state.clone();
    let has_action = state.clone();
    let action_texture = state.clone();
    let equipped_action = state.clone();
    let consumable_action = state.clone();
    let stackable_action = state.clone();
    let action_text = state.clone();
    let current_action = state.clone();
    let auto_repeat_action = state.clone();
    let attack_action = state.clone();
    let usable_action = state.clone();
    let action_range = state.clone();
    let action_count = state.clone();
    let action_cooldown = state.clone();
    globals.raw_set(
        "GetActionBarPage",
        lua.create_function(move |_, ()| Ok(state.page()))?,
    )?;
    globals.raw_set(
        "GetActionBarToggles",
        lua.create_function(move |_, ()| {
            let [bottom_left, bottom_right, right, right_two] = toggle_query.toggles();
            Ok((bottom_left, bottom_right, right, right_two))
        })?,
    )?;
    globals.raw_set(
        "SetActionBarToggles",
        lua.create_function(
            move |_, (bottom_left, bottom_right, right, right_two): (bool, bool, bool, bool)| {
                toggle_update.set_toggles([bottom_left, bottom_right, right, right_two]);
                Ok(())
            },
        )?,
    )?;
    // Build 12340 stores this native constant as the double 6.0. Multicast
    // buttons therefore address the seventh action-bar page region.
    globals.raw_set(
        "GetMultiCastBarOffset",
        lua.create_function(|_, ()| Ok(6_u8))?,
    )?;
    globals.raw_set(
        "GetBonusBarOffset",
        lua.create_function(move |_, ()| Ok(bonus_bar_state.bonus_bar_offset()))?,
    )?;
    globals.raw_set(
        "ChangeActionBarPage",
        lua.create_function(move |_, page: u8| {
            change_state.set_page(page).map_err(|_| {
                mlua::Error::runtime("ChangeActionBarPage() needs a page in the range 1 to 6")
            })
        })?,
    )?;
    globals.raw_set(
        "HasAction",
        lua.create_function(move |_, slot: i32| {
            packed(&has_action, slot, "HasAction").map(|value| value != 0)
        })?,
    )?;
    globals.raw_set(
        "GetActionTexture",
        lua.create_function(move |_, slot: i32| {
            require_available(&action_texture, slot, "GetActionTexture")?;
            Ok(None::<String>)
        })?,
    )?;
    register_empty_boolean(lua, globals, "IsEquippedAction", equipped_action)?;
    register_empty_boolean(lua, globals, "IsConsumableAction", consumable_action)?;
    register_empty_boolean(lua, globals, "IsStackableAction", stackable_action)?;
    globals.raw_set(
        "GetActionText",
        lua.create_function(move |_, slot: i32| {
            require_available(&action_text, slot, "GetActionText")?;
            Ok(None::<String>)
        })?,
    )?;
    register_empty_boolean(lua, globals, "IsCurrentAction", current_action)?;
    register_empty_boolean(lua, globals, "IsAutoRepeatAction", auto_repeat_action)?;
    register_empty_boolean(lua, globals, "IsAttackAction", attack_action)?;
    globals.raw_set(
        "IsUsableAction",
        lua.create_function(move |_, slot: i32| {
            let occupied = packed(&usable_action, slot, "IsUsableAction")? != 0;
            Ok((occupied, false))
        })?,
    )?;
    globals.raw_set(
        "IsActionInRange",
        lua.create_function(move |_, slot: i32| {
            require_available(&action_range, slot, "IsActionInRange")?;
            Ok(None::<u8>)
        })?,
    )?;
    globals.raw_set(
        "GetActionCount",
        lua.create_function(move |_, slot: i32| {
            require_available(&action_count, slot, "GetActionCount")?;
            Ok(0_u32)
        })?,
    )?;
    globals.raw_set(
        "GetActionCooldown",
        lua.create_function(move |_, slot: i32| {
            require_available(&action_cooldown, slot, "GetActionCooldown")?;
            Ok((0.0_f64, 0.0_f64, 0_u8))
        })?,
    )
}

fn register_empty_boolean(
    lua: &Lua,
    globals: &Table,
    name: &'static str,
    state: UiActionBarState,
) -> mlua::Result<()> {
    globals.raw_set(
        name,
        lua.create_function(move |_, slot: i32| {
            require_available(&state, slot, name)?;
            Ok(false)
        })?,
    )
}

fn packed(state: &UiActionBarState, slot: i32, api: &'static str) -> mlua::Result<u32> {
    state
        .packed_slot(slot)
        .map_err(|error| mlua::Error::runtime(format!("{api}: {error}")))
}

/// Validates server-image availability while permitting unresolved metadata.
///
/// SolCL's recovered action-bar controller and stock's nil-tolerant FrameXML
/// contract leave unavailable texture/text metadata empty for occupied slots;
/// slot presence alone must never abort world UI construction.
fn require_available(state: &UiActionBarState, slot: i32, api: &'static str) -> mlua::Result<()> {
    packed(state, slot, api).map(|_packed| ())
}
