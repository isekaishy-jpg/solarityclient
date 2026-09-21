//! Complete retained-state publication for live refresh and covered startup.

use super::{GlueManager, build_live_hierarchy, synchronize_resolved_dimensions};
use crate::glue::pointer::UiPointerPlan;
use crate::startup::complete_unyielding;
use crate::{
    UiEventError, UiPresentationPlan, UiRegionGeometryPlan, UiRenderPlan, UiScrollFramePlan,
};

impl GlueManager {
    /// Ordinary live updates keep callbacks ordered around admitted native preparation.
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
        if live == self.native.live {
            return Ok(());
        }
        if live.is_scroll_only_update_from(&self.native.live) {
            return self.refresh_scroll_state(live);
        }
        if live.is_visual_transform_only_update_from(&self.native.live) {
            return self.refresh_visual_transform_state(live);
        }
        if live.is_visibility_only_update_from(&self.native.live)
            && self.visibility_slots_are_resident(&mut live)?
        {
            return self.refresh_visibility_state(live);
        }
        if live.is_button_state_only_update_from(&self.native.live) {
            return self.refresh_button_state(live);
        }
        let extent = self.native.geometry.ui_extent();
        let mut geometry = self.runtime.font_system().prepare(
            &mut self.assets.borrow_mut(),
            &mut live,
            move |live, _, _| UiRegionGeometryPlan::resolve(live, extent),
        )??;
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
            &self.native.fonts,
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
            geometry = self.runtime.font_system().prepare(
                &mut self.assets.borrow_mut(),
                &mut live,
                move |live, _, _| UiRegionGeometryPlan::resolve(live, extent),
            )??;
        }
        if let Some(budget) = budget {
            budget.checkpoint().await;
        }
        self.runtime
            .publish_resolved_geometry(&self.bundle, &geometry)?;
        if let Some(profile) = &mut profile {
            profile.mark("published");
        }
        let html = self.runtime.simple_html().clone();
        let height = self.glyph_logical_height;
        self.prepare_native(move |state, fonts, assets| {
            synchronize_resolved_dimensions(&mut live, &geometry);
            let scroll_frames = UiScrollFramePlan::from_live(&live);
            let text_changes = (!html_changed)
                .then(|| live.text_layout_changes_from(&state.live))
                .flatten();

            if let Some(text_changes) = text_changes.as_ref()
                && state.glyphs.supports_live_text_objects(
                    &live,
                    height,
                    text_changes.iter().copied(),
                )
            {
                state
                    .glyphs
                    .refresh_live_text_objects(&live, &geometry, height, text_changes)?;
            } else if !html_changed && state.glyphs.supports_live_text(&live, height) {
                state.glyphs.refresh_live_text(&live, &geometry, height)?;
            } else {
                state.glyphs.rebuild_live_ui(
                    &html,
                    &live,
                    &geometry,
                    &state.fonts,
                    assets,
                    fonts,
                    height,
                )?;
            }
            let presentation = UiPresentationPlan::resolve(&live, &geometry, &state.backdrops);
            let render_plan = UiRenderPlan::prepare_with_glyphs(
                &presentation,
                &state.glyphs,
                &geometry,
                &scroll_frames,
                geometry.ui_extent(),
            )?;
            let (objects, child_indices) = build_live_hierarchy(&live)?;
            let pointer = UiPointerPlan::from_live(&live);
            state.live = live;
            state.geometry = geometry;
            state.scroll_frames = scroll_frames;
            state.presentation = presentation;
            state.render_plan = render_plan;
            state.objects = objects;
            state.child_indices = child_indices;
            state.pointer = pointer;

            Ok::<_, UiEventError>(())
        })??;
        if let Some(budget) = budget {
            budget.checkpoint().await;
        }
        if let Some(profile) = &mut profile {
            profile.mark("prepared");
        }
        Ok(())
    }
}
