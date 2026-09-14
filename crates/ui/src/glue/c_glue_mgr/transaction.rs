//! Script events execute immediately; a publication boundary merges their mutations.

use std::cell::Cell;
use std::rc::Rc;

use super::{GlueManager, UiEventError};

/// The scope shares only nesting depth so callbacks can mutably borrow the owner.
#[derive(Default)]
pub(super) struct DeferredPresentation {
    depth: Rc<Cell<usize>>,
    full: bool,
    dirty: Vec<(usize, u32)>,
    visual: Vec<usize>,
}

/// Restores nesting on errors and unwinds without borrowing the UI manager.
pub(in crate::glue) struct PresentationHold(Rc<Cell<usize>>);

impl Drop for PresentationHold {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}

impl GlueManager {
    /// Batches native presentation only. Lua reads authoritative live dimensions,
    /// bounds and properties during every handler, including newly created regions.
    /// Callers must not render or hit-test until the outermost transaction returns.
    ///
    /// # Errors
    /// Returns an error if the final retained presentation cannot be resolved.
    pub fn with_deferred_presentation<T>(
        &mut self,
        publish: impl FnOnce(&mut Self) -> T,
    ) -> Result<T, UiEventError> {
        let hold = self.hold_presentation();
        let result = publish(self);
        drop(hold);
        self.flush_deferred_presentation()?;
        Ok(result)
    }

    /// A FrameManager facade can keep its complete event API inside the same scope.
    pub(in crate::glue) fn hold_presentation(&self) -> PresentationHold {
        let depth = &self.deferred_presentation.depth;
        depth.set(depth.get() + 1);
        PresentationHold(Rc::clone(depth))
    }

    /// Copies event journals before the next callback reuses the Lua dirty arena.
    pub(super) fn defer_event_presentation(
        &mut self,
        dispatch: &crate::script::UiScriptEventDispatch,
    ) -> Result<bool, UiEventError> {
        if self.deferred_presentation.depth.get() == 0 {
            return Ok(false);
        }
        let topology_changed = !self.retained_object_topology_matches_runtime();
        let full = topology_changed
            || (dispatch.changed && !dispatch.targeted_objects && !dispatch.targeted_visual);
        if full {
            // Full refresh previously delivered range/layout callbacks here.
            // Keep those callbacks before the next event; defer only the large
            // native snapshot, glyph and draw publication that followed them.
            self.runtime.prepare_snapshot_layout(&self.bundle)?;
        }
        let pending = &mut self.deferred_presentation;
        pending.full |= full;
        pending.dirty.extend_from_slice(&dispatch.dirty_objects);
        pending.visual.extend_from_slice(&dispatch.visual_objects);
        Ok(true)
    }

    /// Publishes once at the outermost boundary; nested scopes leave journals intact.
    pub(in crate::glue) fn flush_deferred_presentation(&mut self) -> Result<(), UiEventError> {
        if self.deferred_presentation.depth.get() != 0 {
            return Ok(());
        }
        let pending = &mut self.deferred_presentation;
        if !pending.full && pending.dirty.is_empty() && pending.visual.is_empty() {
            return Ok(());
        }
        let full = std::mem::take(&mut pending.full);
        let mut dirty = std::mem::take(&mut pending.dirty);
        let mut visual = std::mem::take(&mut pending.visual);
        dirty.sort_unstable_by_key(|(index, _)| *index);
        dirty.dedup_by(|later, earlier| {
            if later.0 == earlier.0 {
                earlier.1 |= later.1;
                true
            } else {
                false
            }
        });
        visual.sort_unstable();
        visual.dedup();
        let result = if full {
            self.refresh_live_state()
        } else if !dirty.is_empty() {
            self.refresh_targeted_objects(&dirty, &visual).map(|_| ())
        } else if !visual.is_empty() {
            self.refresh_targeted_visual_objects(&visual)
        } else {
            Ok(())
        };
        // Restore capacity without retaining stale mutations after publication.
        dirty.clear();
        visual.clear();
        self.deferred_presentation.dirty = dirty;
        self.deferred_presentation.visual = visual;
        result
    }
}
