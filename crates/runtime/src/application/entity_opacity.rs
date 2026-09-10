//! Shared model opacity survives material, equipment and GPU generation changes.

use std::cell::Cell;

use solarity_systems::EntityOpacity;

#[derive(Default)]
pub(super) struct EntityOpacityOwner {
    state: Cell<EntityOpacity>,
    selection: Cell<Option<u32>>,
    initial_animation_pending: Cell<bool>,
}

impl EntityOpacityOwner {
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
