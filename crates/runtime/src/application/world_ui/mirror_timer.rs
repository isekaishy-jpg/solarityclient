//! Native mirror-timer event ordering and selected-locale labels.

#[cfg(test)]
#[path = "../../../tests/application/mirror_timer.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/application/water_tutorial.rs"]
mod tutorial_tests;

use solarity_network::WorldMirrorTimerUpdate;
use solarity_ui::{UiEventArgument, UiEventPayload, UiMirrorTimer};

use super::RuntimeWorldUi;
use crate::application::{
    ApplicationError, gameplay_coordinator::player_ui::TimedMirrorTimerUpdate,
};

impl RuntimeWorldUi {
    pub(in crate::application) fn player_ui_notification(
        &mut self,
        notification: crate::application::gameplay_coordinator::player_ui::RuntimePlayerUiNotification,
    ) -> Result<(), ApplicationError> {
        match notification {
            crate::application::gameplay_coordinator::player_ui::RuntimePlayerUiNotification::Combat(in_combat) => {
                self.dirty = true;
                self.manager.player_combat_changed(in_combat)?;
                Ok(())
            }
            crate::application::gameplay_coordinator::player_ui::RuntimePlayerUiNotification::MirrorTimer(timer) => self.mirror_timer_notification(timer),
            crate::application::gameplay_coordinator::player_ui::RuntimePlayerUiNotification::TutorialFlags(flags) => {
                self.world.tutorials().replace_flags(&flags);
                Ok(())
            }
        }
    }

    pub(in crate::application) fn tutorial_state(&self) -> solarity_ui::UiTutorialState {
        self.world.tutorials()
    }

    pub(in crate::application) fn trigger_tutorial(
        &mut self,
        index: u32,
    ) -> Result<(), ApplicationError> {
        self.dirty = true;
        self.manager.trigger_tutorial(index)?;
        Ok(())
    }
    pub(super) fn stop_world_mirror_timers(&mut self) -> Result<(), ApplicationError> {
        let mut first_error = None;
        for timer in 0..3 {
            if let Err(error) = self.mirror_timer_notification(TimedMirrorTimerUpdate {
                update: WorldMirrorTimerUpdate::Stop { timer },
                timestamp_ms: 0,
            }) {
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    pub(in crate::application) fn mirror_timer_notification(
        &mut self,
        notification: TimedMirrorTimerUpdate,
    ) -> Result<(), ApplicationError> {
        self.dirty = true;
        dispatch_notification(
            &mut self.manager,
            &self.world,
            &self.spell_names,
            notification,
        )
    }
}

fn dispatch_notification(
    manager: &mut solarity_ui::FrameManager,
    world: &solarity_ui::UiWorldState,
    names: &solarity_asset::SpellNameCatalog,
    notification: TimedMirrorTimerUpdate,
) -> Result<(), ApplicationError> {
    let timer = notification.update.timer();
    let token = UiEventArgument::String(UiMirrorTimer::token(timer).into());
    let (name, arguments) = match notification.update {
        WorldMirrorTimerUpdate::Start {
            value,
            maximum,
            scale,
            paused,
            spell_id,
            ..
        } => (
            "MIRROR_TIMER_START",
            vec![
                token,
                UiEventArgument::Integer(i64::from(value)),
                UiEventArgument::Integer(i64::from(maximum)),
                UiEventArgument::Integer(i64::from(scale)),
                UiEventArgument::Integer(i64::from(paused)),
                UiEventArgument::String(timer_label(manager, names, timer, spell_id)?),
            ],
        ),
        WorldMirrorTimerUpdate::Pause { paused, .. } => (
            "MIRROR_TIMER_PAUSE",
            vec![token, UiEventArgument::Integer(i64::from(paused))],
        ),
        WorldMirrorTimerUpdate::Stop { .. } => ("MIRROR_TIMER_STOP", vec![token]),
    };
    // 519A50 invokes Lua before changing the record. Even a contained Lua
    // failure must not prevent the server-owned anchor from being stored.
    let event = manager.dispatch_event(name, &UiEventPayload::new(arguments));
    let tutorial = if matches!(notification.update, WorldMirrorTimerUpdate::Start { .. }) {
        match timer {
            0 => manager.trigger_tutorial(26),
            1 => manager.trigger_tutorial(28),
            _ => Ok(()),
        }
    } else {
        Ok(())
    };
    match notification.update {
        WorldMirrorTimerUpdate::Start { .. } => {
            if timer < 3 {
                world.set_mirror_timer(timer as usize, project_timer(names, notification));
            }
        }
        WorldMirrorTimerUpdate::Stop { .. } => {
            world.set_mirror_timer(timer as usize, UiMirrorTimer::default())
        }
        WorldMirrorTimerUpdate::Pause { .. } => {}
    }
    event?;
    tutorial?;
    Ok(())
}

pub(super) fn project_timer(
    names: &solarity_asset::SpellNameCatalog,
    notification: TimedMirrorTimerUpdate,
) -> UiMirrorTimer {
    let WorldMirrorTimerUpdate::Start {
        timer,
        value,
        maximum,
        scale,
        paused,
        spell_id,
    } = notification.update
    else {
        return UiMirrorTimer::default();
    };
    UiMirrorTimer {
        kind: timer,
        value,
        maximum,
        scale,
        paused,
        label: names.name(spell_id).map(str::to_owned),
        timestamp_ms: notification.timestamp_ms,
    }
}

fn timer_label(
    manager: &solarity_ui::FrameManager,
    names: &solarity_asset::SpellNameCatalog,
    timer: u32,
    spell_id: u32,
) -> Result<String, ApplicationError> {
    if let Some(name) = names.name(spell_id) {
        return Ok(name.into());
    }
    Ok(manager
        .localized_text(&format!("{}_LABEL", UiMirrorTimer::token(timer)))
        .map_err(solarity_ui::GlueError::from)?
        .unwrap_or_default())
}
