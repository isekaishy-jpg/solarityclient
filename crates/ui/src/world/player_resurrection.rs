//! Offered resurrection and corpse recovery state consumed by stock FrameXML.

#[cfg(test)]
#[path = "../../tests/stock_seed/world_player_resurrection.rs"]
mod tests;

/// C9EAB8/C9EAC0/C9EAC4, retained even when an offer cannot show a dialog.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UiPlayerResurrectionOffer {
    /// Full source GUID, consumed immediately by an admitted Lua response.
    pub guid: u64,
    /// Whether accepting causes resurrection sickness.
    pub sickness: u8,
    /// Whether the dialog uses the corpse recovery deadline outside arenas.
    pub timer: u8,
}

/// BD0828 corpse GUID and the independent BD0850 recovery deadline.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UiPlayerCorpseState {
    /// Last resident non-bones corpse owned by the local player.
    pub guid: u64,
    /// Corpse's original map and display map from CORPSE_QUERY.
    pub maps: (u32, u32),
    /// Current native range latch, used when restarting the deadline.
    pub in_range: bool,
    deadline_ms: u32,
}

impl UiPlayerCorpseState {
    /// Sets 513A80's deadline, preserving zero as an inactive sentinel.
    pub fn set_delay(&mut self, delay_ms: u32, now_ms: u32) {
        self.deadline_ms = now_ms.wrapping_add(delay_ms).max(1);
    }

    /// Native integer seconds, with signed wrapping expiry and no player gate.
    #[must_use]
    pub fn remaining(self, now_ms: u32) -> u32 {
        if self.deadline_ms == 0 {
            0
        } else {
            (self.deadline_ms.wrapping_sub(now_ms) as i32).max(0) as u32 / 1000
        }
    }

    /// Range event repeated by 513A80 when a recovery delay is received.
    #[must_use]
    pub fn range_event(self) -> Option<&'static str> {
        self.in_range.then_some(if self.maps.0 == self.maps.1 {
            "CORPSE_IN_RANGE"
        } else {
            "CORPSE_IN_INSTANCE"
        })
    }
}

pub(crate) fn register_globals(
    lua: &mlua::Lua,
    globals: &mlua::Table,
    environment: &crate::UiScriptEnvironment,
) -> mlua::Result<()> {
    let world = environment.world_state();
    globals.raw_set(
        "ResurrectHasSickness",
        lua.create_function(move |_, ()| {
            Ok((world.resurrection_offer().sickness != 0).then_some(1))
        })?,
    )?;
    let world = environment.world_state();
    let battlefield = environment.battlefield_state();
    globals.raw_set(
        "ResurrectHasTimer",
        lua.create_function(move |_, ()| {
            Ok(
                (!battlefield.active_arena().0 && world.resurrection_offer().timer != 0)
                    .then_some(1),
            )
        })?,
    )?;
    let world = environment.world_state();
    let clock = environment.client_clock();
    globals.raw_set(
        "GetCorpseRecoveryDelay",
        lua.create_function(move |_, ()| Ok(world.corpse_state().remaining(clock.milliseconds())))?,
    )?;
    for (name, accept) in [("AcceptResurrect", true), ("DeclineResurrect", false)] {
        let world = environment.world_state();
        globals.raw_set(
            name,
            lua.create_function(move |_, ()| {
                world.respond_to_resurrection(accept);
                Ok(())
            })?,
        )?;
    }
    let world = environment.world_state();
    globals.raw_set(
        "RetrieveCorpse",
        lua.create_function(move |_, ()| {
            world.respond_to_resurrection(false);
            world.queue_death_action(super::UiPlayerDeathAction::ReclaimCorpse {
                guid: world.corpse_state().guid,
            });
            Ok(())
        })?,
    )?;
    Ok(())
}
