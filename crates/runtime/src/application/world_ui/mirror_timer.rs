//! Native mirror-timer event ordering and selected-locale labels.

#[cfg(test)]
#[path = "../../../tests/application/mirror_timer.rs"]
mod tests;

use solarity_network::WorldMirrorTimerUpdate;
use solarity_ui::{UiEventArgument, UiEventPayload, UiMirrorTimer};

use super::RuntimeWorldUi;
use crate::application::{
    ApplicationError, gameplay_coordinator::mirror_timer::TimedMirrorTimerUpdate,
};

impl RuntimeWorldUi {
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
