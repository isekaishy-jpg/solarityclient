//! Complete retained-state publication for live refresh and covered startup.

use super::{GlueManager, build_live_hierarchy, synchronize_resolved_dimensions};
use crate::glue::pointer::UiPointerPlan;
use crate::startup::complete_unyielding;
use crate::{
    UiEventError, UiPresentationPlan, UiRegionGeometryPlan, UiRenderPlan, UiScrollFramePlan,
};

impl GlueManager {
    /// Ordinary live updates complete without allocating or parking a task.
    pub(super) fn refresh_live_state(&mut self) -> Result<(), UiEventError> {
        complete_unyielding(self.refresh_live_state_cooperatively(None))
    }

    /// A covered startup may suspend between native publication operations.
    pub(super) async fn refresh_live_state_cooperatively(
        &mut self,
        budget: Option<&crate::startup::StartupBudget>,
    ) -> Result<(), UiEventError> {
        // Live refresh never suspends; keep its existing phase names. Covered
        // entry is timed per poll by its owner, never across inter-frame waits.
        let mut profile = budget
            .is_none()
            .then(|| solarity_profiling::profile!("ui.mod.refresh_live_state"));
        self.deferred_scroll_refresh.clear();
        let mut live = match budget {
            Some(budget) => {
                self.runtime
                    .snapshot_objects_for_startup(&self.bundle, budget)
                    .await?
            }
            None => self.runtime.snapshot_objects(&self.bundle)?,
        };
        if let Some(profile) = &mut profile {
            profile.mark("first_snapshot");
        }
        if live == self.live {
            return Ok(());
        }
        if live.is_scroll_only_update_from(&self.live) {
            return self.refresh_scroll_state(live);
        }
        if live.is_visual_transform_only_update_from(&self.live) {
            return self.refresh_visual_transform_state(live);
        }
        if live.is_visibility_only_update_from(&self.live)
            && self.visibility_slots_are_resident(&live)?
        {
            return self.refresh_visibility_state(live);
        }
        if live.is_button_state_only_update_from(&self.live) {
            return self.refresh_button_state(live);
        }
        let mut geometry = UiRegionGeometryPlan::resolve(&live, self.geometry.ui_extent())?;
        if let Some(budget) = budget {
            budget.checkpoint().await;
        }
        if let Some(profile) = &mut profile {
            profile.mark("first_geometry");
        }
        let html_changed = self.runtime.refresh_simple_html_layout(
            &self.bundle,
            &live,
            &geometry,
            &self.fonts,
            &mut self.assets.borrow_mut(),
            self.glyph_logical_height,
        )?;
        if html_changed {
            live = match budget {
                Some(budget) => {
                    self.runtime
                        .snapshot_objects_for_startup(&self.bundle, budget)
                        .await?
                }
                None => self.runtime.snapshot_objects(&self.bundle)?,
            };
            geometry = UiRegionGeometryPlan::resolve(&live, self.geometry.ui_extent())?;
        }
        if let Some(budget) = budget {
            budget.checkpoint().await;
        }
        self.runtime
            .publish_resolved_geometry(&self.bundle, &geometry)?;
        if let Some(profile) = &mut profile {
            profile.mark("published");
        }
        synchronize_resolved_dimensions(&mut live, &geometry);
        let scroll_frames = UiScrollFramePlan::from_live(&live);
        if let Some(budget) = budget {
            budget.checkpoint().await;
        }
        if let Some(profile) = &mut profile {
            profile.mark("scroll");
        }
        let text_changes = (!html_changed)
            .then(|| live.text_layout_changes_from(&self.live))
            .flatten();

        if let Some(text_changes) = text_changes.as_ref()
            && self.glyphs.supports_live_text_objects(
                &live,
                self.glyph_logical_height,
                text_changes.iter().copied(),
            )
        {
            self.glyphs.refresh_live_text_objects(
                &live,
                &geometry,
                self.glyph_logical_height,
                text_changes,
            )?;
        } else if !html_changed
            && self
                .glyphs
                .supports_live_text(&live, self.glyph_logical_height)
        {
            self.glyphs
                .refresh_live_text(&live, &geometry, self.glyph_logical_height)?;
        } else {
            self.glyphs.rebuild_live_ui(
                self.runtime.simple_html(),
                &live,
                &geometry,
                &self.fonts,
                &mut self.assets.borrow_mut(),
                self.glyph_logical_height,
            )?;
        }
        if let Some(budget) = budget {
            budget.checkpoint().await;
        }
        if let Some(profile) = &mut profile {
            profile.mark("glyph");
        }
        let presentation = UiPresentationPlan::resolve(&live, &geometry, &self.backdrops);
        if let Some(budget) = budget {
            budget.checkpoint().await;
        }
        if let Some(profile) = &mut profile {
            profile.mark("presentation");
        }
        let render_plan = UiRenderPlan::prepare_with_glyphs(
            &presentation,
            &self.glyphs,
            &geometry,
            &scroll_frames,
            geometry.ui_extent(),
        )?;
        if let Some(budget) = budget {
            budget.checkpoint().await;
        }
        if let Some(profile) = &mut profile {
            profile.mark("render");
        }
        let (objects, child_indices) = build_live_hierarchy(&live)?;
        let pointer = UiPointerPlan::from_live(&live);
        if let Some(profile) = &mut profile {
            profile.mark("plans");
        }
        self.live = live;
        self.geometry = geometry;
        self.scroll_frames = scroll_frames;
        self.presentation = presentation;
        self.render_plan = render_plan;
        self.objects = objects;
        self.child_indices = child_indices;
        self.pointer = pointer;

        Ok(())
    }
}
