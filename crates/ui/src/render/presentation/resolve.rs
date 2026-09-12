//! Object-local packet resolution shared by full construction and retained publication.

use super::{
    BackdropPresentationContext, UiBackdropStatePlan, UiBlendMode, UiDrawLayer, UiFrameStrata,
    UiModelPresentation, UiObjectRole, UiPresentationPacketKey, UiPresentationPlan,
    UiRegionGeometryPlan, UiRuntimeObjectPlan, UiTexturePresentation, UiTextureSource,
    append_backdrop, draw_rank, nearest_owning_frame, nearest_owning_scroll_frame,
    widget_role_is_active,
};

/// Reuses immutable owner indexes while resolving only named live objects.
pub(super) struct PresentationResolver<'a> {
    pub(super) live: &'a UiRuntimeObjectPlan,
    pub(super) geometry: &'a UiRegionGeometryPlan,
    pub(super) backdrops: &'a UiBackdropStatePlan,
    pub(super) disabled_texture_owners: &'a [bool],
}

/// Proposed packets stay separate until retained slot eligibility is established.
#[derive(Default)]
pub(super) struct ResolvedObjects {
    pub(super) keyed: Vec<(UiPresentationPacketKey, UiTexturePresentation)>,
    pub(super) models: Vec<UiModelPresentation>,
    pub(super) cleared_models: Vec<(usize, bool)>,
}

impl PresentationResolver<'_> {
    /// Applies the same native visibility, role, source and packet policy as full resolution.
    pub(super) fn append(&self, object_index: usize, output: &mut ResolvedObjects) {
        let Self {
            live,
            geometry,
            backdrops,
            disabled_texture_owners,
        } = *self;
        let object = &live.objects()[object_index];
        let ResolvedObjects {
            keyed,
            models,
            cleared_models,
        } = output;

        if let (Some(backdrop), Some(region), Some(strata), Some(frame_level)) = (
            backdrops.state(object_index),
            geometry.region(object_index),
            object.frame_strata,
            object.frame_level,
        ) && ((region.effectively_shown()
            && (region.effective_alpha() > 0.0 || region.animation_active()))
            || strata == UiFrameStrata::Tooltip)
        {
            append_backdrop(
                keyed,
                backdrop,
                BackdropPresentationContext {
                    object_index,
                    object,
                    clip_object: nearest_owning_scroll_frame(live, object_index),
                    bounds: region.presentation_bounds(),
                    effective_alpha: region.effective_alpha() as f32
                        * f32::from(region.effectively_shown()),
                    effective_scale: region.effective_scale(),
                    strata,
                    frame_level,
                },
            );
        }
        if let Some(model) = UiPresentationPlan::configured_model(live, geometry, object_index)
            && geometry.region(object_index).is_some_and(|region| {
                region.effectively_shown()
                    && (region.effective_alpha() > 0.0 || region.animation_active())
            })
        {
            models.push(model);
        }
        if object
            .model
            .as_ref()
            .is_some_and(|model| model.file.is_none() && model.instance_generation != 0)
        {
            cleared_models.push((
                object_index,
                geometry.region(object_index).is_some_and(|region| {
                    region.effectively_shown() && region.effective_alpha() > 0.0
                }),
            ));
        }
        if let Some(minimap) = UiPresentationPlan::configured_minimap(live, geometry, object_index)
            && let (Some(strata), Some(frame_level)) = (object.frame_strata, object.frame_level)
            && geometry.region(object_index).is_some_and(|region| {
                region.effectively_shown()
                    && (region.effective_alpha() > 0.0 || region.animation_active())
            })
        {
            // FUN_0057DCA0 registers the ARTWORK callback and creates its
            // player texture before XML children. The texture constructor's
            // final argument is visibility, not a draw sublevel.
            let key = UiPresentationPacketKey {
                strata,
                frame_level,
                frame_sequence: object_index,
                draw_rank: draw_rank(UiDrawLayer::Artwork, UiObjectRole::Object),
                draw_sub_level: 0,
            };
            keyed.push((
                key,
                UiTexturePresentation {
                    key,
                    object_index,
                    owner_index: object_index,
                    clip_object: nearest_owning_scroll_frame(live, object_index),
                    slider_object: None,
                    source: UiTextureSource::Minimap(object_index),
                    blend_mode: UiBlendMode::Blend,
                    bounds: minimap.bounds,
                    tex_coords: [0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0],
                    vertex_colors: [[1.0; 4]; 4],
                    opacity: minimap.opacity,
                    horizontal_tiling: false,
                    vertical_tiling: false,
                    non_blocking: true,
                    desaturated: false,
                },
            ));
        }
        let Some(texture) = &object.texture else {
            return;
        };
        let Some(region) = geometry.region(object_index) else {
            return;
        };
        let Some(owner_index) = nearest_owning_frame(live, object) else {
            return;
        };
        let owner = &live.objects()[owner_index];
        let (Some(strata), Some(frame_level)) = (owner.frame_strata, owner.frame_level) else {
            return;
        };
        // Hidden texture children of a visible frame retain their draw
        // slots. Stock scripts toggle ordinary overlays (not just native
        // button-state roles), so hiding one must not discard the slot
        // needed by the next content/visibility journal. Whole hidden
        // frames still defer their texture topology until first reveal.
        let owner_presented = geometry.region(owner_index).is_some_and(|owner_region| {
            owner_region.effectively_shown()
                && (owner_region.effective_alpha() > 0.0 || owner_region.animation_active())
        });
        if (!region.effectively_shown()
            || region.effective_alpha() <= 0.0 && !region.animation_active())
            && strata != UiFrameStrata::Tooltip
            && !owner_presented
        {
            return;
        }
        let source = if let Some(unit) = &texture.portrait_unit {
            UiTextureSource::UnitPortrait(unit.clone())
        } else if let Some(path) = &texture.file {
            UiTextureSource::Asset(path.clone())
        } else if let Some(color) = texture.solid_color {
            UiTextureSource::SolidColor(color.map(|value| value as f32))
        } else {
            return;
        };
        let key = UiPresentationPacketKey {
            strata,
            frame_level,
            frame_sequence: owner_index,
            draw_rank: draw_rank(texture.draw_layer, object.role),
            draw_sub_level: texture.draw_sub_level,
        };
        let vertex_colors = texture
            .vertex_colors
            .map(|color| color.map(|value| value as f32));
        let effective_alpha = region.effective_alpha() as f32
            * f32::from(region.effectively_shown())
            * f32::from(widget_role_is_active(
                object.role,
                owner,
                disabled_texture_owners[owner_index],
            ));
        keyed.push((
            key,
            UiTexturePresentation {
                key,
                object_index,
                owner_index,
                clip_object: nearest_owning_scroll_frame(live, object_index),
                slider_object: (object.role == UiObjectRole::ThumbTexture
                    && live.anchors_for(object).is_empty())
                .then_some(object.parent)
                .flatten()
                .filter(|parent| live.objects()[*parent].slider.is_some()),
                source,
                blend_mode: texture.blend_mode,
                bounds: region.presentation_bounds(),
                tex_coords: texture.tex_coords.map(|value| value as f32),
                vertex_colors,
                opacity: effective_alpha,
                horizontal_tiling: texture.horizontal_tiling,
                vertical_tiling: texture.vertical_tiling,
                non_blocking: texture.non_blocking,
                desaturated: texture.desaturated,
            },
        ));
    }
}
