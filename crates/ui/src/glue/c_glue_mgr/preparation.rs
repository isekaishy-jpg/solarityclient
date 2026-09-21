//! Sendable retained state; Lua and its callback/publication boundaries stay on main.

use super::*;

pub(super) struct UiNativeState {
    pub(super) visual_indices: Vec<usize>,
    pub(super) visual_work: Vec<usize>,
    pub(super) fonts: FontCatalog,
    pub(super) live: UiRuntimeObjectPlan,
    pub(super) geometry: UiRegionGeometryPlan,
    pub(super) scroll_frames: UiScrollFramePlan,
    pub(super) glyphs: UiGlyphAtlasPlan,
    pub(super) presentation: UiPresentationPlan,
    pub(super) render_plan: UiRenderPlan,
    pub(super) backdrops: UiBackdropStatePlan,
    pub(super) objects: Vec<GlueObject>,
    pub(super) child_indices: Vec<usize>,
    pub(super) pointer: UiPointerPlan,
}

impl Default for UiNativeState {
    fn default() -> Self {
        let live = UiRuntimeObjectPlan::default();
        let geometry = UiRegionGeometryPlan::resolve(&live, (1.0, 1.0))
            .unwrap_or_else(|_| unreachable!("empty geometry"));
        let backdrops = UiBackdropStatePlan::default();
        let presentation = UiPresentationPlan::resolve(&live, &geometry, &backdrops);
        let render_plan = UiRenderPlan::prepare(&presentation, (1.0, 1.0))
            .unwrap_or_else(|_| unreachable!("empty render plan"));
        let scroll_frames = UiScrollFramePlan::from_live(&live);
        let pointer = UiPointerPlan::from_live(&live);
        Self {
            visual_indices: Vec::new(),
            visual_work: Vec::new(),
            fonts: FontCatalog::default(),
            live,
            geometry,
            scroll_frames,
            glyphs: UiGlyphAtlasPlan::default(),
            presentation,
            render_plan,
            backdrops,
            objects: Vec::new(),
            child_indices: Vec::new(),
            pointer,
        }
    }
}

impl GlueManager {
    pub(super) fn prepare_native<R: Send + 'static>(
        &mut self,
        operation: impl FnOnce(&mut UiNativeState, &mut crate::FontSystem, &mut AssetStore) -> R
        + Send
        + 'static,
    ) -> Result<R, crate::FontError> {
        self.runtime.font_system().prepare(
            &mut self.assets.borrow_mut(),
            &mut self.native,
            operation,
        )
    }
}

impl UiNativeState {
    /// Reports whether the visible targeted subtree already owns draw slots.
    pub(super) fn current_visual_slots_are_resident(&self) -> bool {
        self.visual_indices.iter().all(|&object_index| {
            let Some(object) = self.live.objects().get(object_index) else {
                return true;
            };
            let presented = self.geometry.region(object_index).is_some_and(|region| {
                region.effectively_shown()
                    && (region.effective_alpha() > 0.0 || region.animation_active())
            });
            if !presented {
                return true;
            }
            let owns_quad = object.minimap.is_some()
                || object
                    .texture
                    .as_ref()
                    .is_some_and(|texture| texture.file.is_some() || texture.solid_color.is_some())
                || object
                    .text
                    .as_ref()
                    .is_some_and(|text| !text.content.is_empty())
                || self.backdrops.state(object_index).is_some();
            (!owns_quad || self.render_plan.mesh().contains_object(object_index))
                && (object.model.is_none() || self.presentation.contains_model(object_index))
        })
    }
    pub(super) fn collect_visual_subtrees(&mut self, roots: &[usize]) {
        self.visual_indices.clear();
        self.visual_work.clear();
        for &root in roots.iter().rev() {
            let mut parent = self.live.objects()[root].parent;
            let mut nested = false;
            while let Some(index) = parent {
                if roots.binary_search(&index).is_ok() {
                    nested = true;
                    break;
                }
                parent = self.live.objects()[index].parent;
            }
            if !nested {
                self.visual_work.push(root);
            }
        }
        while let Some(object_index) = self.visual_work.pop() {
            self.visual_indices.push(object_index);
            if let Some(object) = self.objects.get(object_index) {
                let first = object.first_child();
                let end = first + object.child_count();
                for child_index in (first..end).rev() {
                    self.visual_work.push(self.child_indices[child_index]);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/preparation.rs"]
mod tests;

impl UiNativeState {
    pub(super) fn rebuild_live_text_topology(
        &mut self,
        html: &crate::UiSimpleHtmlPlan,
        height: u32,
        fonts: &mut crate::FontSystem,
        assets: &mut AssetStore,
    ) -> Result<(), UiEventError> {
        if self.glyphs.supports_live_text(&self.live, height) {
            self.glyphs
                .refresh_live_text(&self.live, &self.geometry, height)?;
        } else {
            self.glyphs.rebuild_live_ui(
                html,
                &self.live,
                &self.geometry,
                &self.fonts,
                assets,
                fonts,
                height,
            )?;
        }
        self.render_plan = UiRenderPlan::prepare_with_glyphs(
            &self.presentation,
            &self.glyphs,
            &self.geometry,
            &self.scroll_frames,
            self.geometry.ui_extent(),
        )?;
        Ok(())
    }
    pub(super) fn children(&self, object_index: usize) -> Option<&[usize]> {
        let object = self.objects.get(object_index)?;
        Some(&self.child_indices[object.first_child()..object.first_child() + object.child_count()])
    }
}
