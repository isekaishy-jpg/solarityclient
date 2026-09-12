//! Retained UI generation publication and dependency-island updates.

use super::{
    GlueManager, UiEventError, UiFrameStrata, UiGlyphAtlasPlan, UiObjectKind, UiPointerPlan,
    UiPresentationPlan, UiRenderPlan, synchronize_resolved_dimensions_for,
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
        let timings = std::env::var_os("SOLARITY_UI_TIMINGS").is_some();
        let started = std::time::Instant::now();
        self.deferred_slider_refresh = None;
        let had_dirty_objects = !dirty_objects.is_empty();
        let effective =
            self.runtime
                .effective_layout_journal(&self.bundle, &self.live, dirty_objects)?;
        let dirty_objects = effective.as_ref();
        if had_dirty_objects && dirty_objects.is_empty() {
            if !visual_objects.is_empty() {
                self.refresh_targeted_visual_objects(visual_objects)?;
            }
            if timings {
                let visuals = visual_objects
                    .iter()
                    .map(|&index| {
                        self.live.objects()[index]
                            .name
                            .as_deref()
                            .unwrap_or("<anonymous>")
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                eprintln!(
                    "UI unchanged layout: objects={} visuals=[{visuals}] total={:.3}ms",
                    dirty_objects.len(),
                    started.elapsed().as_secs_f64() * 1000.
                );
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
        if timings {
            let journal = dirty_objects
                .iter()
                .map(|&(object_index, flags)| {
                    let object = &self.live.objects()[object_index];
                    format!(
                        "{}:{:?}:{flags:#04x}",
                        object.name.as_deref().unwrap_or("<anonymous>"),
                        object.kind,
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            eprintln!("UI targeted journal: {journal}");
        }
        let edit_box_text_journal = self
            .runtime
            .is_edit_box_text_journal(&self.live, dirty_objects);
        if let Some(tooltip_owner) = self.retained_tooltip_journal_owner(dirty_objects)
            && self.refresh_retained_tooltip(
                tooltip_owner,
                dirty_objects,
                visual_objects,
                started,
            )?
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
                if timings {
                    eprintln!(
                        "UI retained EditBox patch: objects={} visuals={} total={:.3}ms",
                        dirty_objects.len(),
                        visual_objects.len(),
                        started.elapsed().as_secs_f64() * 1_000.0,
                    );
                }
                return Ok(true);
            }
        }
        if self
            .runtime
            .is_retained_content_journal(&self.live, dirty_objects)
            && self.refresh_retained_content(dirty_objects, visual_objects, started)?
        {
            return Ok(true);
        }
        if !visual_objects_refreshed && !visual_objects.is_empty() {
            self.runtime
                .refresh_visual_objects(&self.bundle, &mut self.live, visual_objects)?;
        }
        let mut text_objects =
            self.runtime
                .refresh_dirty_objects(&self.bundle, &mut self.live, dirty_objects)?;
        let copied_elapsed = started.elapsed();
        let refreshed = self.geometry.refresh_dependency_regions(
            &self.live,
            dirty_objects
                .iter()
                .map(|&(index, _)| index)
                .chain(visual_objects.iter().copied()),
        )?;
        let geometry = &self.geometry;
        let geometry_elapsed = started.elapsed();
        self.runtime.publish_resolved_geometry_objects(
            &self.bundle,
            geometry,
            refreshed.changed_objects,
        )?;
        let affected = refreshed.affected_objects;
        synchronize_resolved_dimensions_for(&mut self.live, geometry, affected.iter().copied());
        text_objects.extend(
            affected
                .iter()
                .copied()
                .filter(|&index| self.live.objects()[index].text.is_some()),
        );
        text_objects.sort_unstable();
        text_objects.dedup();
        let published_elapsed = started.elapsed();
        self.scroll_frames
            .refresh_objects(&self.live, affected.iter().copied());
        let scroll_frames = &self.scroll_frames;
        let scroll_elapsed = started.elapsed();
        if !text_objects.is_empty() {
            if self.glyphs.supports_live_text_objects(
                &self.live,
                self.glyph_logical_height,
                text_objects.iter().copied(),
            ) {
                self.glyphs.refresh_live_text_objects(
                    &self.live,
                    geometry,
                    self.glyph_logical_height,
                    &text_objects,
                )?;
            } else if self
                .glyphs
                .supports_live_text(&self.live, self.glyph_logical_height)
            {
                self.glyphs
                    .refresh_live_text(&self.live, geometry, self.glyph_logical_height)?;
            } else {
                self.glyphs = UiGlyphAtlasPlan::from_live_ui(
                    self.runtime.simple_html(),
                    &self.live,
                    geometry,
                    &self.fonts,
                    &mut self.assets.borrow_mut(),
                    self.glyph_logical_height,
                )?;
            }
        }
        let glyph_elapsed = started.elapsed();
        if !self
            .presentation
            .refresh_objects(&self.live, geometry, &self.backdrops, &affected)
            && !self
                .presentation
                .rebuild_objects(&self.live, geometry, &self.backdrops, &affected)
        {
            // Native role reparenting changes old/new owner policy outside the
            // region dependency island; reconstruct that structural transaction.
            self.presentation = UiPresentationPlan::resolve(&self.live, geometry, &self.backdrops);
        }
        let presentation = &self.presentation;
        let presentation_elapsed = started.elapsed();
        let render_plan = UiRenderPlan::prepare_with_glyphs(
            presentation,
            &self.glyphs,
            geometry,
            scroll_frames,
            geometry.ui_extent(),
        )?;
        let render_elapsed = started.elapsed();
        if !self
            .pointer
            .refresh_objects(&self.live, affected.iter().copied())
        {
            self.pointer = UiPointerPlan::from_live(&self.live);
        }
        let plans_elapsed = started.elapsed();
        self.render_plan = render_plan;
        if timings {
            eprintln!(
                "UI targeted publish: copy={:.3}ms geometry={:.3}ms writeback={:.3}ms scroll={:.3}ms glyphs={:.3}ms presentation={:.3}ms mesh={:.3}ms pointer={:.3}ms total={:.3}ms",
                copied_elapsed.as_secs_f64() * 1_000.0,
                geometry_elapsed
                    .saturating_sub(copied_elapsed)
                    .as_secs_f64()
                    * 1_000.0,
                published_elapsed
                    .saturating_sub(geometry_elapsed)
                    .as_secs_f64()
                    * 1_000.0,
                scroll_elapsed
                    .saturating_sub(published_elapsed)
                    .as_secs_f64()
                    * 1_000.0,
                glyph_elapsed.saturating_sub(scroll_elapsed).as_secs_f64() * 1_000.0,
                presentation_elapsed
                    .saturating_sub(glyph_elapsed)
                    .as_secs_f64()
                    * 1_000.0,
                render_elapsed
                    .saturating_sub(presentation_elapsed)
                    .as_secs_f64()
                    * 1_000.0,
                plans_elapsed.saturating_sub(render_elapsed).as_secs_f64() * 1_000.0,
                started.elapsed().as_secs_f64() * 1_000.0,
            );
        }
        Ok(true)
    }

    /// Reuses content and texture-layout slots while material, packet order,
    /// and source counts remain stable. Anchor-dependent non-texture movement
    /// leaves the complete publisher responsible for rebuilding those regions.
    pub(super) fn refresh_retained_content(
        &mut self,
        dirty_objects: &[(usize, u32)],
        visual_objects: &[usize],
        started: std::time::Instant,
    ) -> Result<bool, UiEventError> {
        if !visual_objects.is_empty() {
            // Script visibility/animation can target a different subtree than
            // the content journal. Publish those draw slots as well as Lua
            // state before the content path limits work to its own roots.
            if !self.try_refresh_targeted_visual_objects(visual_objects)? {
                if std::env::var_os("SOLARITY_UI_TIMINGS").is_some() {
                    eprintln!("UI content fallback: visual topology");
                }
                return Ok(false);
            }
        }
        let text_objects =
            self.runtime
                .refresh_dirty_objects(&self.bundle, &mut self.live, dirty_objects)?;
        let copied = started.elapsed();
        if !text_objects.is_empty()
            && !self.glyphs.supports_live_text_objects(
                &self.live,
                self.glyph_logical_height,
                text_objects.iter().copied(),
            )
        {
            if std::env::var_os("SOLARITY_UI_TIMINGS").is_some() {
                eprintln!("UI content fallback: glyph coverage");
            }
            return Ok(false);
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
        let refreshed = self.geometry.refresh_dependency_regions(
            &self.live,
            dirty_objects.iter().map(|&(object_index, _)| object_index),
        )?;
        self.runtime.publish_resolved_geometry_objects(
            &self.bundle,
            &self.geometry,
            refreshed.changed_objects,
        )?;
        let changed_regions = refreshed.affected_objects;
        let geometry = &self.geometry;
        let mut texture_objects = dirty_objects
            .iter()
            .map(|&(object_index, _)| object_index)
            .collect::<Vec<_>>();
        for (&index, previous) in changed_regions.iter().zip(refreshed.previous_regions) {
            let object = &self.live.objects()[index];
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
                if std::env::var_os("SOLARITY_UI_TIMINGS").is_some() {
                    eprintln!(
                        "UI content fallback: dependent geometry object={index} name={:?} kind={:?} old={old:?} new={new:?}",
                        object.name, object.kind
                    );
                }
                return Ok(false);
            }
        }
        synchronize_resolved_dimensions_for(
            &mut self.live,
            geometry,
            changed_regions.iter().copied(),
        );
        self.scroll_frames
            .refresh_objects(&self.live, changed_regions.iter().copied());
        let scroll_frames = &self.scroll_frames;
        let resolved = started.elapsed();
        if !text_objects.is_empty() {
            self.glyphs.refresh_live_text_objects(
                &self.live,
                geometry,
                self.glyph_logical_height,
                &text_objects,
            )?;
            if !self.render_plan.refresh_glyph_objects(
                &self.glyphs,
                &self.live,
                geometry,
                scroll_frames,
                &text_objects,
            )? {
                return Ok(false);
            }
        }
        let glyphs = started.elapsed();
        texture_objects.extend(changed_regions.iter().copied());
        texture_objects.sort_unstable();
        texture_objects.dedup();
        if !self.presentation.refresh_objects(
            &self.live,
            geometry,
            &self.backdrops,
            &texture_objects,
        ) {
            return Ok(false);
        }
        let presentation = &self.presentation;
        if !self.render_plan.refresh_texture_objects(
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
        self.collect_visual_subtrees(&roots);
        let geometry = &self.geometry;
        for &index in &self.visual_indices {
            self.presentation
                .refresh_visual_object(&self.live, geometry, index, [0.0; 2]);
        }
        self.render_plan.refresh_region_opacities(
            &self.presentation,
            geometry,
            &self.visual_indices,
        )?;
        let rendered = started.elapsed();
        if !self
            .pointer
            .refresh_objects(&self.live, dirty_objects.iter().map(|&(index, _)| index))
        {
            return Ok(false);
        }
        if std::env::var_os("SOLARITY_UI_TIMINGS").is_some() {
            eprintln!(
                "UI retained content patch: objects={} text_objects={} copy={:.3}ms resolve={:.3}ms glyphs={:.3}ms presentation={:.3}ms total={:.3}ms",
                dirty_objects.len(),
                text_objects.len(),
                copied.as_secs_f64() * 1_000.0,
                resolved.saturating_sub(copied).as_secs_f64() * 1_000.0,
                glyphs.saturating_sub(resolved).as_secs_f64() * 1_000.0,
                rendered.saturating_sub(glyphs).as_secs_f64() * 1_000.0,
                started.elapsed().as_secs_f64() * 1_000.0,
            );
        }
        Ok(true)
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
                let object = self.live.objects().get(index)?;
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
        started: std::time::Instant,
    ) -> Result<bool, UiEventError> {
        let missing_topology = !visual_objects.is_empty()
            && !self.try_refresh_targeted_visual_objects(visual_objects)?;
        if missing_topology
            && !self.runtime.is_tooltip_materialization_journal(
                &self.live,
                tooltip_owner,
                dirty_objects,
            )
        {
            return Ok(false);
        }
        let text_objects =
            self.runtime
                .refresh_dirty_objects(&self.bundle, &mut self.live, dirty_objects)?;
        let copied = started.elapsed();
        if text_objects.is_empty()
            || !self.glyphs.supports_live_text_objects(
                &self.live,
                self.glyph_logical_height,
                text_objects.iter().copied(),
            )
        {
            return Ok(false);
        }
        let refreshed_geometry = self.geometry.refresh_dependency_regions(
            &self.live,
            dirty_objects.iter().map(|&(object_index, _)| object_index),
        )?;
        let resolved = started.elapsed();
        self.runtime.publish_resolved_geometry_objects(
            &self.bundle,
            &self.geometry,
            refreshed_geometry.changed_objects,
        )?;
        let changed_regions = refreshed_geometry.affected_objects;
        synchronize_resolved_dimensions_for(
            &mut self.live,
            &self.geometry,
            changed_regions.iter().copied(),
        );
        let published = started.elapsed();
        self.glyphs.refresh_live_text_objects(
            &self.live,
            &self.geometry,
            self.glyph_logical_height,
            &text_objects,
        )?;
        let laid_out = started.elapsed();
        if !self.presentation.refresh_backdrop_object(
            &self.live,
            &self.geometry,
            &self.backdrops,
            tooltip_owner,
        ) {
            return Ok(false);
        }
        let mut texture_objects = vec![tooltip_owner];
        for &object_index in &changed_regions {
            let object = &self.live.objects()[object_index];
            if object.texture.as_ref().is_some_and(|texture| {
                texture.file.is_some()
                    || texture.solid_color.is_some()
                    || texture.portrait_unit.is_some()
            }) {
                if self.presentation.refresh_texture_geometry_object(
                    &self.live,
                    &self.geometry,
                    object_index,
                ) {
                    texture_objects.push(object_index);
                } else if self.geometry.region(object_index).is_some_and(|region| {
                    region.effectively_shown()
                        && (region.effective_alpha() > 0.0 || region.animation_active())
                }) {
                    return Ok(false);
                }
                // Hidden textures without slots stay deferred until their reveal.
            }
            if (object.model.is_some() || object.minimap.is_some())
                && self
                    .geometry
                    .region(object_index)
                    .is_some_and(|region| region.effectively_shown())
            {
                return Ok(false);
            }
        }
        texture_objects.sort_unstable();
        texture_objects.dedup();
        let presented = started.elapsed();
        let glyphs_retained = self.render_plan.refresh_glyph_objects(
            &self.glyphs,
            &self.live,
            &self.geometry,
            &self.scroll_frames,
            &text_objects,
        )?;
        let backdrop_retained = self.render_plan.refresh_texture_objects(
            &self.presentation,
            &self.geometry,
            &self.scroll_frames,
            &texture_objects,
        )?;
        let rendered = started.elapsed();
        if !glyphs_retained || !backdrop_retained {
            return Ok(false);
        }
        self.collect_visual_subtrees(&[tooltip_owner]);
        self.render_plan.refresh_region_opacities(
            &self.presentation,
            &self.geometry,
            &self.visual_indices,
        )?;
        if std::env::var_os("SOLARITY_UI_TIMINGS").is_some() {
            eprintln!(
                "UI retained tooltip patch: owner={tooltip_owner} text_objects={} copy={:.3}ms geometry={:.3}ms publish={:.3}ms glyphs={:.3}ms presentation={:.3}ms vertices={:.3}ms final={:.3}ms total={:.3}ms",
                text_objects.len(),
                copied.as_secs_f64() * 1_000.0,
                resolved.saturating_sub(copied).as_secs_f64() * 1_000.0,
                published.saturating_sub(resolved).as_secs_f64() * 1_000.0,
                laid_out.saturating_sub(published).as_secs_f64() * 1_000.0,
                presented.saturating_sub(laid_out).as_secs_f64() * 1_000.0,
                rendered.saturating_sub(presented).as_secs_f64() * 1_000.0,
                started.elapsed().saturating_sub(rendered).as_secs_f64() * 1_000.0,
                started.elapsed().as_secs_f64() * 1_000.0,
            );
        }
        Ok(true)
    }

    pub(super) fn refresh_retained_edit_box_text(
        &mut self,
        dirty_objects: &[(usize, u32)],
    ) -> Result<bool, UiEventError> {
        let timings = std::env::var_os("SOLARITY_UI_TIMINGS").is_some();
        let started = std::time::Instant::now();
        let text_objects =
            self.runtime
                .refresh_dirty_objects(&self.bundle, &mut self.live, dirty_objects)?;
        let copied = started.elapsed();
        if text_objects.is_empty()
            || !self.glyphs.supports_live_text_objects(
                &self.live,
                self.glyph_logical_height,
                text_objects.iter().copied(),
            )
        {
            return Ok(false);
        }
        self.glyphs.refresh_live_text_objects(
            &self.live,
            &self.geometry,
            self.glyph_logical_height,
            &text_objects,
        )?;
        let laid_out = started.elapsed();
        if !self.render_plan.refresh_glyph_objects(
            &self.glyphs,
            &self.live,
            &self.geometry,
            &self.scroll_frames,
            &text_objects,
        )? {
            return Ok(false);
        }
        let rendered = started.elapsed();
        self.pointer
            .refresh_edit_box_focus(&self.live, &text_objects);
        if timings {
            eprintln!(
                "UI retained EditBox stages: copy={:.3}ms layout={:.3}ms vertices={:.3}ms pointer={:.3}ms",
                copied.as_secs_f64() * 1_000.0,
                laid_out.saturating_sub(copied).as_secs_f64() * 1_000.0,
                rendered.saturating_sub(laid_out).as_secs_f64() * 1_000.0,
                started.elapsed().saturating_sub(rendered).as_secs_f64() * 1_000.0,
            );
        }
        Ok(true)
    }

    /// Publishes visual mutations, rebuilding only when newly shown slots are absent.
    pub(super) fn refresh_targeted_visual_objects(
        &mut self,
        visual_objects: &[usize],
    ) -> Result<(), UiEventError> {
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
            .refresh_visual_objects(&self.bundle, &mut self.live, visual_objects)?;
        self.collect_visual_subtrees(visual_objects);
        let changes = self
            .geometry
            .refresh_visual_regions(&self.live, &self.visual_indices);
        if !self.current_visual_slots_are_resident() {
            return Ok(false);
        }
        for change in &changes {
            self.presentation.refresh_visual_object(
                &self.live,
                &self.geometry,
                change.object_index,
                change.translation,
            );
        }
        self.render_plan.refresh_visual_objects(
            &changes,
            &self.geometry,
            &self.presentation,
            &self.scroll_frames,
            &self.live,
            &self.glyphs,
        )?;
        Ok(true)
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
