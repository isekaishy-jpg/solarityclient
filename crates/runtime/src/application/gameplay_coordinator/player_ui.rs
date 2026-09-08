//! Session-owned player UI snapshots and ordered server notifications.

use std::collections::VecDeque;

use solarity_network::WorldMirrorTimerUpdate;

/// One notification anchored to the same clock used by `GetMirrorTimerProgress`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::application) struct TimedMirrorTimerUpdate {
    pub update: WorldMirrorTimerUpdate,
    pub timestamp_ms: u32,
}

/// Shared ordering prevents a later flag packet changing an earlier timer trigger.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::application) enum RuntimePlayerUiNotification {
    MirrorTimer(TimedMirrorTimerUpdate),
    TutorialFlags(Vec<u8>),
    Combat(bool),
}

/// Retained independently of UI residency and cleared by native world exit.
#[derive(Default)]
pub(in crate::application) struct RuntimePlayerUiState {
    slots: [Option<TimedMirrorTimerUpdate>; 3],
    tutorial_flags: Vec<u8>,
    in_combat: bool,
    pending: VecDeque<RuntimePlayerUiNotification>,
}

impl RuntimePlayerUiState {
    pub(in crate::application) fn observe_combat(&mut self, world: &solarity_ecs::ActiveWorld) {
        let in_combat = world
            .local_player_guid()
            .ok()
            .and_then(|guid| world.unit_flags(guid))
            .is_some_and(|flags| flags.primary() & 0x80000 != 0);
        if self.in_combat != in_combat {
            self.in_combat = in_combat;
            self.pending
                .push_back(RuntimePlayerUiNotification::Combat(in_combat));
        }
    }

    pub(in crate::application) fn receive_tutorial_flags(&mut self, flags: &[u8]) {
        self.tutorial_flags = flags.to_vec();
        self.pending
            .push_back(RuntimePlayerUiNotification::TutorialFlags(flags.to_vec()));
    }

    pub(in crate::application) fn tutorial_flags(&self) -> &[u8] {
        &self.tutorial_flags
    }
    pub(in crate::application) fn clear_for_world_leave(&mut self) {
        self.in_combat = false;
        self.slots = [None; 3];
        self.pending.clear();
    }
    pub(in crate::application) fn receive(
        &mut self,
        update: WorldMirrorTimerUpdate,
        timestamp_ms: u32,
    ) {
        let notification = TimedMirrorTimerUpdate {
            update,
            timestamp_ms,
        };
        self.pending
            .push_back(RuntimePlayerUiNotification::MirrorTimer(notification));
        if let Some(slot) = self.slots.get_mut(update.timer() as usize) {
            match update {
                WorldMirrorTimerUpdate::Start { .. } => *slot = Some(notification),
                WorldMirrorTimerUpdate::Stop { .. } => *slot = None,
                // 519B03 emits an event without updating any BD0B80 field.
                WorldMirrorTimerUpdate::Pause { .. } => {}
            }
        }
    }

    pub(in crate::application) fn slots(&self) -> &[Option<TimedMirrorTimerUpdate>; 3] {
        &self.slots
    }

    pub(in crate::application) fn take_notification(
        &mut self,
    ) -> Option<RuntimePlayerUiNotification> {
        self.pending.pop_front()
    }

    pub(in crate::application) fn discard_published_notifications(&mut self) {
        self.pending.clear();
    }
}
