//! Player death's retained release timer, read synchronously by FrameXML.

#[cfg(test)]
#[path = "../../tests/stock_seed/world_player_release.rs"]
mod tests;

/// Native BD0848 deadline and BD084C no-timer flag.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UiPlayerReleaseTimer {
    deadline_ms: u32,
    no_timer: bool,
}

impl UiPlayerReleaseTimer {
    /// Initializes 6DC070 from private field 1197's low byte and PLAYER_FLAGS.
    #[must_use]
    pub fn on_death(player_bytes_2: u8, player_flags: u32, now_ms: u32) -> Self {
        let delay = if player_bytes_2 & 0x10 != 0 {
            Some(0)
        } else if player_bytes_2 & 8 != 0 && player_flags & 0x4000 == 0 {
            Some(360_000)
        } else {
            None
        };
        match delay {
            Some(delay) => Self {
                // 513A30 reserves zero as the inactive deadline.
                deadline_ms: now_ms.wrapping_add(delay).max(1),
                no_timer: false,
            },
            None => Self {
                deadline_ms: 0,
                no_timer: true,
            },
        }
    }

    /// Returns 516210's integer seconds, including its -1 no-timer sentinel.
    #[must_use]
    pub fn remaining(self, now_ms: u32) -> i32 {
        if self.no_timer {
            -1
        } else if self.deadline_ms == 0 {
            0
        } else {
            (self.deadline_ms.wrapping_sub(now_ms) as i32).max(0) / 1000
        }
    }
}

pub(crate) fn register_globals(
    lua: &mlua::Lua,
    globals: &mlua::Table,
    environment: &crate::UiScriptEnvironment,
) -> mlua::Result<()> {
    let cinematic = environment.world_state();
    globals.raw_set(
        "InCinematic",
        lua.create_function(move |_, ()| Ok(cinematic.in_cinematic().then_some(1)))?,
    )?;
    let world = environment.world_state();
    let clock = environment.client_clock();
    globals.raw_set(
        "GetReleaseTimeRemaining",
        lua.create_function(move |_, ()| {
            Ok(world.release_timer().remaining(clock.milliseconds()))
        })?,
    )?;
    let world = environment.world_state();
    globals.raw_set(
        "HasSoulstone",
        lua.create_function(move |_, ()| {
            Ok(world
                .player_vitals()
                .filter(|v| (v.health() as i32) <= 0)
                .and_then(|_| world.resurrection_state().self_resurrection_name))
        })?,
    )?;
    let world = environment.world_state();
    globals.raw_set(
        "CannotBeResurrected",
        lua.create_function(move |_, ()| {
            Ok((world.player().is_some() && world.resurrection_state().blocked).then_some(1))
        })?,
    )?;
    let world = environment.world_state();
    globals.raw_set(
        "IsOutOfBounds",
        lua.create_function(move |_, ()| {
            Ok((world.player().is_some() && world.resurrection_state().out_of_bounds).then_some(1))
        })?,
    )?;
    let world = environment.world_state();
    globals.raw_set(
        "IsFalling",
        lua.create_function(move |_, ()| {
            Ok((world.player().is_some() && world.is_falling()).then_some(1))
        })?,
    )?;
    let world = environment.world_state();
    globals.raw_set(
        "RepopMe",
        lua.create_function(move |_, ()| {
            // CGPlayer_C vtable A326C8 + 128 -> 6DAC10 includes ghosts.
            if world
                .player_vitals()
                .is_some_and(|v| (v.health() as i32) <= 0 || v.ghost())
                && world.resurrection_state().release_allowed()
            {
                world.queue_death_action(UiPlayerDeathAction::ReleaseSpirit);
            }
            Ok(())
        })?,
    )?;
    let world = environment.world_state();
    globals.raw_set(
        "UseSoulstone",
        lua.create_function(move |_, ()| {
            let state = world.resurrection_state();
            // 51ADD0's spell branch has no raw-health/ghost admission check.
            if world.player().is_some()
                && state.self_resurrection_spell != 0
                && state.release_allowed()
            {
                world.queue_death_action(UiPlayerDeathAction::SelfResurrect);
            }
            Ok(())
        })?,
    )
}

/// Inputs used by the stock death dialog and 727860 release admission.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UiPlayerResurrectionState {
    /// PLAYER_SELF_RES_SPELL (absolute field 1199), including unknown IDs.
    pub self_resurrection_spell: u32,
    /// Localized self-resurrection spell or usable inventory item name.
    pub self_resurrection_name: Option<String>,
    /// PLAYER_FLAGS bit 14.
    pub out_of_bounds: bool,
    /// Presence of aura type 314 in the unit's active effect set.
    pub blocked: bool,
    /// Self-resurrection spell attribute 0x08000000 bypasses the blocker.
    pub bypass_blocker: bool,
}

/// Ordered requests admitted by the death-dialog native Lua APIs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiPlayerDeathAction {
    /// Explicit Release Spirit button; opcode 15A with a zero byte.
    ReleaseSpirit,
    /// Use the replicated self-resurrection spell; opcode 2B3, empty body.
    SelfResurrect,
}

impl UiPlayerResurrectionState {
    /// Native release admission after the player's death/ghost check.
    #[must_use]
    pub const fn release_allowed(&self) -> bool {
        !self.blocked || self.bypass_blocker
    }
}
