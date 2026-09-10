//! Shared model opacity survives material, equipment and GPU generation changes.

use std::cell::Cell;

use solarity_systems::EntityOpacity;

#[derive(Default)]
pub(super) struct EntityOpacityOwner {
    state: Cell<EntityOpacity>,
    selection: Cell<Option<u32>>,
    initial_animation_pending: Cell<bool>,
    transport_guid: Cell<u64>,
    removed_at: Cell<Option<u32>>,
}

impl EntityOpacityOwner {
    pub fn set_transport_guid(&self, guid: u64) {
        self.transport_guid.set(guid);
    }

    pub fn mark_removed(&self, now: u32) {
        if self.removed_at.get().is_none() {
            self.removed_at.set(Some(now));
        }
    }

    /// The model inherits the primary byte, independently of camera opacity.
    pub fn retirement(&self) -> Option<(u32, f32, u64)> {
        self.removed_at.get().map(|now| {
            (
                now,
                self.state.get().retirement_opacity(),
                self.transport_guid.get(),
            )
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
