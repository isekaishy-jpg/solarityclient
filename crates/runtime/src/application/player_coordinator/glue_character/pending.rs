//! A selected attempt cannot be revived after withdrawal, including selection ABA.

use super::super::worker_presentation::AppearanceTask;
use super::super::{ResidentGlueCharacterKey, ResidentGlueCharacterModel};
use solarity_rendering::CharacterComponentTextureLevel;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;

/// Cache ownership lives in the task until all source/readiness work is terminal.
pub(in crate::application::player_coordinator) struct PendingGlueCharacter {
    pub(super) submitted_at: Instant,
    pub(super) key: ResidentGlueCharacterKey,
    pub(super) level: CharacterComponentTextureLevel,
    pub(super) withdrawn: Arc<AtomicBool>,
    pub(super) task: AppearanceTask<Option<ResidentGlueCharacterModel>>,
}

impl PendingGlueCharacter {
    /// Facing is applied at publication, while component quality changes residency.
    pub(super) fn matches(
        &self,
        key: &ResidentGlueCharacterKey,
        level: CharacterComponentTextureLevel,
    ) -> bool {
        !self.withdrawn.load(Ordering::Acquire)
            && self.level == level
            && self.key.same_residency(key)
    }

    /// Cancels dependent work and withdraws only this consumer's source demand.
    pub(in crate::application::player_coordinator) fn withdraw(&mut self) {
        if !self.withdrawn.swap(true, Ordering::AcqRel) {
            self.task.retire();
        }
    }
}
