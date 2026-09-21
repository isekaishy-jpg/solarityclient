//! Scroll transactions preserve layout, glyphs and unchanged native objects.

use super::{GlueManager, UiEventError};
use crate::UiObjectRole;

impl GlueManager {
    /// Coalesces only pure scroll transactions. Other callback changes publish
    /// through the ordinary event path and cannot be hidden by a captured drag.
    pub fn flush_deferred_refresh(&mut self) -> Result<bool, UiEventError> {
        if self.deferred_scroll_refresh.is_empty() {
            return Ok(false);
        }
        let mut dirty = std::mem::take(&mut self.deferred_scroll_refresh);
        self.refresh_scroll_objects(&dirty)?;
        dirty.clear();
        self.deferred_scroll_refresh = dirty;
        Ok(true)
    }

    /// Copies named range owners and updates their indexed draw transforms.
    pub(super) fn refresh_scroll_objects(
        &mut self,
        dirty: &[(usize, u32)],
    ) -> Result<(), UiEventError> {
        let _profile_scope =
            solarity_profiling::profile!("ui.glue.c_glue_mgr.scrolling.refresh_scroll_objects");
        let previous =
            self.runtime
                .refresh_scroll_objects(&self.bundle, &mut self.native.live, dirty)?;
        let dirty = dirty.to_vec();
        self.prepare_native(move |state, _, _| {
            for (index, old) in &previous {
                let new = &state.live.objects()[*index];
                let thumb = state
                    .children(*index)
                    .into_iter()
                    .flatten()
                    .copied()
                    .find(|&child| {
                        let candidate = &state.live.objects()[child];
                        candidate.role == UiObjectRole::ThumbTexture
                            && state.live.anchors_for(candidate).is_empty()
                    });
                state.render_plan.refresh_scroll_object(
                    *index,
                    (old, new),
                    thumb,
                    &mut state.geometry,
                    &mut state.presentation,
                );
            }
            state
                .scroll_frames
                .refresh_objects(&state.live, dirty.iter().map(|(index, _)| *index));
            Ok(())
        })?
    }
}
