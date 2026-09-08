//! Session-owned native mirror-timer records and ordered UI notifications.

use std::collections::VecDeque;

use solarity_network::WorldMirrorTimerUpdate;

/// One notification anchored to the same clock used by `GetMirrorTimerProgress`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::application) struct TimedMirrorTimerUpdate {
    pub update: WorldMirrorTimerUpdate,
    pub timestamp_ms: u32,
}

/// Retained independently of UI residency and cleared by native world exit.
#[derive(Default)]
pub(in crate::application) struct RuntimeMirrorTimers {
    slots: [Option<TimedMirrorTimerUpdate>; 3],
    pending: VecDeque<TimedMirrorTimerUpdate>,
}

impl RuntimeMirrorTimers {
    pub(in crate::application) fn clear_for_world_leave(&mut self) {
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
        self.pending.push_back(notification);
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

    pub(in crate::application) fn take_notification(&mut self) -> Option<TimedMirrorTimerUpdate> {
        self.pending.pop_front()
    }

    pub(in crate::application) fn discard_published_notifications(&mut self) {
        self.pending.clear();
    }
}
