//! The session-global active battlefield context is independent of world entry.

use solarity_network::WorldBattlefieldStatus;

#[derive(Default)]
pub(super) struct BattlefieldState {
    pub(super) active_queue: Option<u32>,
}

impl BattlefieldState {
    /// 54AE40 retains the cached map kind when lookup fails or status is not three.
    pub(super) fn receive(
        &mut self,
        update: WorldBattlefieldStatus,
        maps: &std::collections::HashMap<u32, bool>,
        arena: &mut bool,
    ) {
        match update {
            WorldBattlefieldStatus::Ignored => {}
            WorldBattlefieldStatus::Cleared(queue) => {
                if self.active_queue == Some(queue) {
                    self.active_queue = None;
                    *arena = false;
                }
            }
            WorldBattlefieldStatus::Status { queue, status, map } => {
                if status == 3 {
                    if let Some(kind) = map.and_then(|id| maps.get(&id)) {
                        *arena = *kind;
                    }
                    self.active_queue = Some(queue);
                } else if self.active_queue == Some(queue) {
                    self.active_queue = None;
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/application/battlefield.rs"]
mod tests;
