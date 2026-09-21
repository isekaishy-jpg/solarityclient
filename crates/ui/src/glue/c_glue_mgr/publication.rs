//! Retained UI generation publication and dependency-island updates.

use super::{
    GlueManager, UiEventError, UiFrameStrata, UiObjectKind, UiPointerPlan, UiPresentationPlan,
    UiRenderPlan, synchronize_resolved_dimensions_for,
};

impl GlueManager {
    /// Publishes a topology-stable event from its typed mutation journal.
    ///
    /// Geometry follows the indexed dependency island. Draw ordering retains
    /// its complete publisher when packet membership changes, and font work
    /// stays confined to labels whose source state actually changed.
    pub(super) fn refresh_targeted_objects(
        &mut self,
        dirty_objects: &[(usize, u32)],
        visual_objects: &[usize],
    ) -> Result<bool, UiEventError> {
        if self.runtime.is_scroll_journal(dirty_objects) {
            self.refresh_scroll_objects(dirty_objects)?;
            if !visual_objects.is_empty() {
                self.refresh_targeted_visual_objects(visual_objects)?;
            }
            return Ok(true);
        }
        let mut ui_profile =
            solarity_profiling::profile!("ui.publication.refresh_targeted_objects");
        self.deferred_scroll_refresh.clear();
        let had_dirty_objects = !dirty_objects.is_empty();
        let effective = self.runtime.effective_layout_journal(
            &self.bundle,
            &self.native.live,
            dirty_objects,
        )?;
        let dirty_objects = effective.as_ref();
        if had_dirty_objects && dirty_objects.is_empty() {
            if !visual_objects.is_empty() {
                self.refresh_targeted_visual_objects(visual_objects)?;
            }

            return Ok(!visual_objects.is_empty());
        }
        // Reclassify mixed transactions after removing restored layout writes.
        // Only a pruned journal enters here: the color publisher's unchanged
        // fallback journal must still be able to use the general path below.
        if matches!(effective, std::borrow::Cow::Owned(_))
            && self.runtime.is_texture_vertex_color_journal(dirty_objects)
        {
            self.refresh_texture_vertex_colors(dirty_objects, visual_objects)?;
            return Ok(true);
        }

        let edit_box_text_journal = self
            .runtime
            .is_edit_box_text_journal(&self.native.live, dirty_objects);
        if let Some(tooltip_owner) = self.retained_tooltip_journal_owner(dirty_objects)
            && self.refresh_retained_tooltip(tooltip_owner, dirty_objects, visual_objects)?
        {
            return Ok(true);
        }
        let mut visual_objects_refreshed = false;
        if edit_box_text_journal {
            if !visual_objects.is_empty() {
                self.refresh_targeted_visual_objects(visual_objects)?;
                visual_objects_refreshed = true;
            }
            if self.refresh_retained_edit_box_text(dirty_objects)? {
                return Ok(true);
            }
        }
        if self
            .runtime
            .is_retained_content_journal(&self.native.live, dirty_objects)
            && self.refresh_retained_content(dirty_objects, visual_objects)?
        {
            return Ok(true);
        }
        ui_profile.mark("classified");
        if !visual_objects_refreshed && !visual_objects.is_empty() {
            self.runtime.refresh_visual_objects(
                &self.bundle,
                &mut self.native.live,
                visual_objects,
            )?;
        }
        ui_profile.mark("visual");
        let mut text_objects = self.runtime.refresh_dirty_objects(
            &self.bundle,
            &mut self.native.live,
            dirty_objects,
        )?;
        ui_profile.mark("copied");

        let changed = dirty_objects
            .iter()
            .map(|&(index, _)| index)
            .chain(visual_objects.iter().copied())
            .collect::<Vec<_>>();
        let refreshed = self.prepare_native(move |state, _, _| {
            state
                .geometry
                .refresh_dependency_regions(&state.live, changed)
        })??;
        let geometry = &self.native.geometry;
        ui_profile.mark("geometry");
        self.runtime.publish_resolved_geometry_objects(
            &self.bundle,
            geometry,
            refreshed.changed_objects,
        )?;
        let affected = refreshed.affected_objects;
        synchronize_resolved_dimensions_for(
            &mut self.native.live,
            geometry,
            affected.iter().copied(),
        );
        text_objects.extend(
            affected
                .iter()
                .copied()
                .filter(|&index| self.native.live.objects()[index].text.is_some()),
        );
        text_objects.sort_unstable();
        text_objects.dedup();
        ui_profile.mark("published");
        let html = self.runtime.simple_html().clone();
        let height = self.glyph_logical_height;
        self.prepare_native(move |state, fonts, assets| {
            let geometry = &state.geometry;
            state
                .scroll_frames
                .refresh_objects(&state.live, affected.iter().copied());
            let scroll_frames = &state.scroll_frames;
            if !text_objects.is_empty() {
                if state.glyphs.supports_live_text_objects(
                    &state.live,
                    height,
                    text_objects.iter().copied(),
                ) {
                    state.glyphs.refresh_live_text_objects(
                        &state.live,
                        geometry,
                        height,
                        &text_objects,
                    )?;
                } else if state.glyphs.supports_live_text(&state.live, height) {
                    state
                        .glyphs
                        .refresh_live_text(&state.live, geometry, height)?;
                } else {
                    state.glyphs.rebuild_live_ui(
                        &html,
                        &state.live,
                        geometry,
                        &state.fonts,
                        assets,
                        fonts,
                        height,
                    )?;
                }
            }
            if !state.presentation.refresh_objects(
                &state.live,
                geometry,
                &state.backdrops,
                &affected,
            ) && !state.presentation.rebuild_objects(
                &state.live,
                geometry,
                &state.backdrops,
                &affected,
            ) {
                // Native role reparenting changes old/new owner policy outside the
                // region dependency island; reconstruct that structural transaction.
                state.presentation =
                    UiPresentationPlan::resolve(&state.live, geometry, &state.backdrops);
            }
            let presentation = &state.presentation;
            let render_plan = UiRenderPlan::prepare_with_glyphs(
                presentation,
                &state.glyphs,
                geometry,
                scroll_frames,
                geometry.ui_extent(),
            )?;
            if !state
                .pointer
                .refresh_objects(&state.live, affected.iter().copied())
            {
                state.pointer = UiPointerPlan::from_live(&state.live);
            }
            state.render_plan = render_plan;

            Ok::<_, UiEventError>(())
        })??;
        ui_profile.mark("prepared");
        Ok(true)
    }

    /// Reuses content and texture-layout slots while material, packet order,
    /// and source counts remain stable. Anchor-dependent non-texture movement
    /// leaves the complete publisher responsible for rebuilding those regions.
    pub(super) fn refresh_retained_content(
        &mut self,
        dirty_objects: &[(usize, u32)],
        visual_objects: &[usize],
    ) -> Result<bool, UiEventError> {
        let mut ui_profile =
            solarity_profiling::profile!("ui.publication.refresh_retained_content");
        if !visual_objects.is_empty() {
            // Script visibility/animation can target a different subtree than
            // the content journal. Publish those draw slots as well as Lua
            // state before the content path limits work to its own roots.
            if !self.try_refresh_targeted_visual_objects(visual_objects)? {
                return Ok(false);
            }
        }
        let text_objects = self.runtime.refresh_dirty_objects(
            &self.bundle,
            &mut self.native.live,
            dirty_objects,
        )?;
        ui_profile.mark("copied");
        let height = self.glyph_logical_height;
        let prepared_text = text_objects.clone();
        let prepared_dirty = dirty_objects.to_vec();
        let Some(refreshed) = self.prepare_native(move |state, fonts, assets| {
            state.glyphs.ensure_live_text_objects(
                &state.live,
                assets,
                fonts,
                height,
                prepared_text.iter().copied(),
            )?;
            if !prepared_text.is_empty()
                && !state.glyphs.supports_live_text_objects(
                    &state.live,
                    height,
                    prepared_text.iter().copied(),
                )
            {
                return Ok(None);
            }
            // CharacterCreateIconButtonTemplate moves its bevel and resizes its
            // shadow on mouse-down/up. Retain those texture slots, but account for
            // anchors that can move objects outside the explicit mutation set.
            // A content callback can also replace icon UVs, corner colors, and
            // frame backdrops. The renderer verifies their complete source slots
            // before retaining them; a changed material or membership rebuilds.
            // Geometry resolves transactionally and publishes only the dependency
            // island. Keep its old region values for eligibility comparisons, not
            // another copy of the complete arena and reverse dependency graph.
            let refreshed = state.geometry.refresh_dependency_regions(
                &state.live,
                prepared_dirty.iter().map(|&(object_index, _)| object_index),
            )?;
            Ok::<_, UiEventError>(Some(refreshed))
        })??
        else {
            return Ok(false);
        };
        self.runtime.publish_resolved_geometry_objects(
            &self.bundle,
            &self.native.geometry,
            refreshed.changed_objects,
        )?;
        let dirty_objects = dirty_objects.to_vec();
        self.prepare_native(move |state, _, _| {
            let changed_regions = refreshed.affected_objects;
            let geometry = &state.geometry;
            let mut texture_objects = dirty_objects
                .iter()
                .map(|&(object_index, _)| object_index)
                .collect::<Vec<_>>();
            for (&index, previous) in changed_regions.iter().zip(refreshed.previous_regions) {
                let object = &state.live.objects()[index];
                let Some(current) = geometry.region(index) else {
                    return Ok(false);
                };
                let old = previous.presentation_bounds();
                let new = current.presentation_bounds();
                // Dimension writeback can perturb f64 anchors by a few ulps. The
                // renderer consumes f32 coordinates, so those identical payloads
                // do not constitute movement of an unrelated region.
                if [
                    old.left() as f32,
                    old.bottom() as f32,
                    old.right() as f32,
                    old.top() as f32,
                ] == [
                    new.left() as f32,
                    new.bottom() as f32,
                    new.right() as f32,
                    new.top() as f32,
                ] && previous.effective_scale() as f32 == current.effective_scale() as f32
                {
                    continue;
                }
                if object.kind == UiObjectKind::Texture {
                    texture_objects.push(index);
                } else if text_objects.binary_search(&index).is_err() {
                    return Ok(false);
                }
            }
            synchronize_resolved_dimensions_for(
                &mut state.live,
                geometry,
                changed_regions.iter().copied(),
            );
            state
                .scroll_frames
                .refresh_objects(&state.live, changed_regions.iter().copied());
            let scroll_frames = &state.scroll_frames;
            if !text_objects.is_empty() {
                state.glyphs.refresh_live_text_objects(
                    &state.live,
                    geometry,
                    height,
                    &text_objects,
                )?;
                if !state.render_plan.refresh_glyph_objects(
                    &state.glyphs,
                    &state.live,
                    geometry,
                    scroll_frames,
                    &text_objects,
                )? {
                    return Ok(false);
                }
            }
            texture_objects.extend(changed_regions.iter().copied());
            texture_objects.sort_unstable();
            texture_objects.dedup();
            if !state.presentation.refresh_objects(
                &state.live,
                geometry,
                &state.backdrops,
                &texture_objects,
            ) {
                return Ok(false);
            }
            let presentation = &state.presentation;
            if !state.render_plan.refresh_texture_objects(
                presentation,
                geometry,
                scroll_frames,
                &texture_objects,
            )? {
                return Ok(false);
            }
            let roots = dirty_objects
                .iter()
                .map(|&(object_index, _)| object_index)
                .collect::<Vec<_>>();
            state.collect_visual_subtrees(&roots);
            let geometry = &state.geometry;
            for &index in &state.visual_indices {
                state
                    .presentation
                    .refresh_visual_object(&state.live, geometry, index, [0.0; 2]);
            }
            state.render_plan.refresh_region_opacities(
                &state.presentation,
                geometry,
                &state.visual_indices,
            )?;
            if !state
                .pointer
                .refresh_objects(&state.live, dirty_objects.iter().map(|&(index, _)| index))
            {
                return Ok(false);
            }

            Ok(true)
        })?
    }

    pub(super) fn retained_tooltip_journal_owner(
        &self,
        dirty_objects: &[(usize, u32)],
    ) -> Option<usize> {
        let mut owner = None;
        for &(object_index, _) in dirty_objects {
            let mut cursor = Some(object_index);
            let mut candidate = None;
            while let Some(index) = cursor {
                let object = self.native.live.objects().get(index)?;
                if object.frame_strata == Some(UiFrameStrata::Tooltip) {
                    candidate = Some(index);
                    break;
                }
                cursor = object.parent;
            }
            let candidate = candidate?;
            if owner.is_some_and(|owner| owner != candidate) {
                return None;
            }
            owner = Some(candidate);
        }
        owner
    }

    pub(super) fn refresh_retained_tooltip(
        &mut self,
        tooltip_owner: usize,
        dirty_objects: &[(usize, u32)],
        visual_objects: &[usize],
    ) -> Result<bool, UiEventError> {
        let mut ui_profile =
            solarity_profiling::profile!("ui.publication.refresh_retained_tooltip");
        let missing_topology = !visual_objects.is_empty()
            && !self.try_refresh_targeted_visual_objects(visual_objects)?;
        if missing_topology
            && !self.runtime.is_tooltip_materialization_journal(
                &self.native.live,
                tooltip_owner,
                dirty_objects,
            )
        {
            return Ok(false);
        }
        let text_objects = self.runtime.refresh_dirty_objects(
            &self.bundle,
            &mut self.native.live,
            dirty_objects,
        )?;
        ui_profile.mark("copied");
        let height = self.glyph_logical_height;
        let prepared_text = text_objects.clone();
        let prepared_dirty = dirty_objects.to_vec();
        let Some(refreshed_geometry) = self.prepare_native(move |state, fonts, assets| {
            state.glyphs.ensure_live_text_objects(
                &state.live,
                assets,
                fonts,
                height,
                prepared_text.iter().copied(),
            )?;
            if prepared_text.is_empty()
                || !state.glyphs.supports_live_text_objects(
                    &state.live,
                    height,
                    prepared_text.iter().copied(),
                )
            {
                return Ok(None);
            }
            let refreshed_geometry = state.geometry.refresh_dependency_regions(
                &state.live,
                prepared_dirty.iter().map(|&(object_index, _)| object_index),
            )?;
            Ok::<_, UiEventError>(Some(refreshed_geometry))
        })??
        else {
            return Ok(false);
        };
        self.runtime.publish_resolved_geometry_objects(
            &self.bundle,
            &self.native.geometry,
            refreshed_geometry.changed_objects,
        )?;
        self.prepare_native(move |state, _, _| {
            let changed_regions = refreshed_geometry.affected_objects;
            synchronize_resolved_dimensions_for(
                &mut state.live,
                &state.geometry,
                changed_regions.iter().copied(),
            );
            state.glyphs.refresh_live_text_objects(
                &state.live,
                &state.geometry,
                height,
                &text_objects,
            )?;
            if !state.presentation.refresh_backdrop_object(
                &state.live,
                &state.geometry,
                &state.backdrops,
                tooltip_owner,
            ) {
                return Ok(false);
            }
            let mut texture_objects = vec![tooltip_owner];
            for &object_index in &changed_regions {
                let object = &state.live.objects()[object_index];
                if object.texture.as_ref().is_some_and(|texture| {
                    texture.file.is_some()
                        || texture.solid_color.is_some()
                        || texture.portrait_unit.is_some()
                }) {
                    if state.presentation.refresh_texture_geometry_object(
                        &state.live,
                        &state.geometry,
                        object_index,
                    ) {
                        texture_objects.push(object_index);
                    } else if state.geometry.region(object_index).is_some_and(|region| {
                        region.effectively_shown()
                            && (region.effective_alpha() > 0.0 || region.animation_active())
                    }) {
                        return Ok(false);
                    }
                    // Hidden textures without slots stay deferred until their reveal.
                }
                if (object.model.is_some() || object.minimap.is_some())
                    && state
                        .geometry
                        .region(object_index)
                        .is_some_and(|region| region.effectively_shown())
                {
                    return Ok(false);
                }
            }
            texture_objects.sort_unstable();
            texture_objects.dedup();
            let glyphs_retained = state.render_plan.refresh_glyph_objects(
                &state.glyphs,
                &state.live,
                &state.geometry,
                &state.scroll_frames,
                &text_objects,
            )?;
            let backdrop_retained = state.render_plan.refresh_texture_objects(
                &state.presentation,
                &state.geometry,
                &state.scroll_frames,
                &texture_objects,
            )?;
            if !glyphs_retained || !backdrop_retained {
                return Ok(false);
            }
            state.collect_visual_subtrees(&[tooltip_owner]);
            state.render_plan.refresh_region_opacities(
                &state.presentation,
                &state.geometry,
                &state.visual_indices,
            )?;

            Ok(true)
        })?
    }

    pub(super) fn refresh_retained_edit_box_text(
        &mut self,
        dirty_objects: &[(usize, u32)],
    ) -> Result<bool, UiEventError> {
        let mut ui_profile =
            solarity_profiling::profile!("ui.publication.refresh_retained_edit_box_text");
        let text_objects = self.runtime.refresh_dirty_objects(
            &self.bundle,
            &mut self.native.live,
            dirty_objects,
        )?;
        ui_profile.mark("copied");
        let height = self.glyph_logical_height;
        self.prepare_native(move |state, fonts, assets| {
            state.glyphs.ensure_live_text_objects(
                &state.live,
                assets,
                fonts,
                height,
                text_objects.iter().copied(),
            )?;
            if text_objects.is_empty()
                || !state.glyphs.supports_live_text_objects(
                    &state.live,
                    height,
                    text_objects.iter().copied(),
                )
            {
                return Ok(false);
            }
            state.glyphs.refresh_live_text_objects(
                &state.live,
                &state.geometry,
                height,
                &text_objects,
            )?;
            if !state.render_plan.refresh_glyph_objects(
                &state.glyphs,
                &state.live,
                &state.geometry,
                &state.scroll_frames,
                &text_objects,
            )? {
                return Ok(false);
            }
            state
                .pointer
                .refresh_edit_box_focus(&state.live, &text_objects);

            Ok(true)
        })?
    }

    /// Publishes visual mutations, rebuilding only when newly shown slots are absent.
    pub(super) fn refresh_targeted_visual_objects(
        &mut self,
        visual_objects: &[usize],
    ) -> Result<(), UiEventError> {
        let _profile_scope = solarity_profiling::profile!(
            "ui.glue.c_glue_mgr.publication.refresh_targeted_visual_objects"
        );
        if !self.try_refresh_targeted_visual_objects(visual_objects)? {
            self.rebuild_visual_topology_from_live()?;
        }
        // Hit testing reads current visibility, alpha, and transformed bounds
        // from geometry. Visual journals do not alter the retained target facts.
        Ok(())
    }

    /// Lets a content publisher absorb missing visual topology in its one rebuild.
    ///
    /// Live visual state and geometry are refreshed even on false, but no full
    /// glyph layout or mesh publication occurs before the caller handles content.
    pub(super) fn try_refresh_targeted_visual_objects(
        &mut self,
        visual_objects: &[usize],
    ) -> Result<bool, UiEventError> {
        self.runtime
            .refresh_visual_objects(&self.bundle, &mut self.native.live, visual_objects)?;
        let visual_objects = visual_objects.to_vec();
        self.prepare_native(move |state, _, _| {
            state.collect_visual_subtrees(&visual_objects);
            let changes = state
                .geometry
                .refresh_visual_regions(&state.live, &state.visual_indices);
            if !state.current_visual_slots_are_resident() {
                return Ok(false);
            }
            for change in &changes {
                state.presentation.refresh_visual_object(
                    &state.live,
                    &state.geometry,
                    change.object_index,
                    change.translation,
                );
            }
            state.render_plan.refresh_visual_objects(
                &changes,
                &state.geometry,
                &state.presentation,
                &state.scroll_frames,
                &state.live,
                &state.glyphs,
            )?;
            Ok(true)
        })?
    }
}
