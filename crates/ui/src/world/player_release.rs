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
    /// Initializes 6DC070 from PLAYER_BYTES_2's low byte and PLAYER_FLAGS.
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
    let world = environment.world_state();
    let clock = environment.client_clock();
    globals.raw_set(
        "GetReleaseTimeRemaining",
        lua.create_function(move |_, ()| {
            Ok(world.release_timer().remaining(clock.milliseconds()))
        })?,
    )
}
