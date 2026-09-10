//! Shared model opacity survives material, equipment and GPU generation changes.

use std::cell::Cell;
use std::rc::{Rc, Weak};

use solarity_systems::EntityOpacity;

#[derive(Default)]
pub(super) struct EntityOpacityOwner {
    state: Cell<EntityOpacity>,
    selection: Cell<Option<u32>>,
    initial_animation_pending: Cell<bool>,
    transport_guid: Cell<u64>,
    removed_at: Cell<Option<u32>>,
    camera_hidden: Cell<bool>,
    player_hidden: Cell<bool>,
    removed_hidden: Cell<bool>,
}

pub(super) struct RemovedEntityPresentation {
    pub time_ms: u32,
    pub opacity: f32,
    pub transport_guid: u64,
    pub visible: bool,
}

impl EntityOpacityOwner {
    pub fn set_camera_opacity(&self, byte: u8) {
        self.camera_hidden.set(byte == 0);
        let mut state = self.state.get();
        state.set_multiplier(f32::from(byte) / 255.0);
        self.state.set(state);
    }

    pub fn set_player_hidden(&self, hidden: bool) {
        self.player_hidden.set(hidden);
    }

    pub fn hidden(&self) -> bool {
        self.camera_hidden.get() || self.player_hidden.get()
    }

    pub fn set_transport_guid(&self, guid: u64) {
        self.transport_guid.set(guid);
    }

    pub fn mark_removed(&self, now: u32) {
        if self.removed_at.get().is_none() {
            self.removed_hidden.set(self.hidden());
            self.removed_at.set(Some(now));
        }
    }

    /// The model inherits the primary byte, independently of camera opacity.
    pub fn retirement(&self) -> Option<RemovedEntityPresentation> {
        self.removed_at.get().map(|now| RemovedEntityPresentation {
            time_ms: now,
            opacity: self.state.get().retirement_opacity(),
            transport_guid: self.transport_guid.get(),
            visible: !self.removed_hidden.get(),
        })
    }

    pub fn has_model(&self, display_id: u32) -> bool {
        self.selection.get() == Some(display_id)
    }

    /// CPU model admission publishes a target once for each selected display.
    pub fn select_model(&self, display_id: u32, target: f32, duration_ms: u32, now: u32) {
        if self.has_model(display_id) {
            return;
        }
        self.selection.set(Some(display_id));
        let mut state = self.state.get();
        state.select(target, duration_ms, now);
        self.state.set(state);
        self.initial_animation_pending.set(true);
    }

    /// 73FCC0 suppresses entry interpolation when the selected behavior is Birth.
    pub fn resolve_initial_animation(&self, birth: bool) {
        if self.initial_animation_pending.replace(false) && birth {
            let mut state = self.state.get();
            state.finish();
            self.state.set(state);
        }
    }

    pub fn advance(&self, now: u32) {
        let mut state = self.state.get();
        state.advance(now);
        self.state.set(state);
    }

    pub fn opacity(&self) -> f32 {
        self.selection
            .get()
            .map_or(1.0, |_| self.state.get().opacity())
    }

    pub fn transitioning(&self) -> bool {
        self.state.get().transitioning()
    }
}

/// 6066E0 restores the old subject before a new camera subject takes ownership.
#[derive(Default)]
pub(super) struct CameraOpacitySubject {
    owner: Weak<EntityOpacityOwner>,
}

impl CameraOpacitySubject {
    pub fn update(&mut self, owner: Option<&Rc<EntityOpacityOwner>>, byte: u8) {
        if let Some(previous) = self.owner.upgrade()
            && owner.is_none_or(|owner| !Rc::ptr_eq(owner, &previous))
        {
            previous.set_camera_opacity(255);
        }
        self.owner = owner.map_or_else(Weak::new, Rc::downgrade);
        if let Some(owner) = owner {
            owner.set_camera_opacity(byte);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changing_camera_subject_restores_only_the_previous_multiplier() {
        let old = Rc::new(EntityOpacityOwner::default());
        old.select_model(1, 1.0, 1000, 0);
        old.advance(500);
        let primary = old.opacity();
        let mut camera = CameraOpacitySubject::default();
        camera.update(Some(&old), 0);
        assert_eq!(old.opacity(), 0.0);
        assert!(old.transitioning());
        let new = Rc::new(EntityOpacityOwner::default());
        new.select_model(1, 1.0, 0, 0);
        camera.update(Some(&new), 127);
        assert_eq!(old.opacity(), primary);
        assert!(old.transitioning());
        assert!(new.opacity() > 0.49 && new.opacity() < 0.5);
        camera.update(None, 255);
        assert_eq!(new.opacity(), 1.0);
        camera.update(Some(&old), 0);
        let weak = Rc::downgrade(&old);
        drop(old);
        assert!(
            weak.upgrade().is_none(),
            "camera does not retain removed units"
        );
        camera.update(Some(&new), 255);
        assert_eq!(new.opacity(), 1.0);
    }

    #[test]
    fn removal_keeps_its_visibility_even_after_camera_subject_release() {
        let owner = Rc::new(EntityOpacityOwner::default());
        owner.select_model(1, 1., 0, 0);
        let mut camera = CameraOpacitySubject::default();
        camera.update(Some(&owner), 0);
        owner.mark_removed(100);
        camera.update(None, 255);
        assert!(!owner.hidden());
        let removed = owner
            .retirement()
            .unwrap_or_else(|| panic!("removed snapshot"));
        assert_eq!(removed.time_ms, 100);
        assert_eq!(
            removed.opacity, 1.0,
            "camera visibility does not replace primary opacity"
        );
        assert!(!removed.visible);
        owner.mark_removed(200);
        assert_eq!(owner.retirement().map(|value| value.time_ms), Some(100));
    }
}
