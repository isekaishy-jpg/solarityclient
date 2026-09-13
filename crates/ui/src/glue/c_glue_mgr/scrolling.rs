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
        let previous = self
            .runtime
            .refresh_scroll_objects(&self.bundle, &mut self.live, dirty)?;
        for (index, old) in &previous {
            let new = &self.live.objects()[*index];
            let thumb = self
                .children(*index)
                .into_iter()
                .flatten()
                .copied()
                .find(|&child| {
                    let candidate = &self.live.objects()[child];
                    candidate.role == UiObjectRole::ThumbTexture
                        && self.live.anchors_for(candidate).is_empty()
                });
            self.render_plan.refresh_scroll_object(
                *index,
                (old, new),
                thumb,
                &mut self.geometry,
                &mut self.presentation,
            );
        }
        self.scroll_frames
            .refresh_objects(&self.live, dirty.iter().map(|(index, _)| *index));
        Ok(())
    }
}
