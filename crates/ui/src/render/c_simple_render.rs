//! Batched UI mesh generation recovered from `CSimpleRender.cpp`.

use solarity_rendering::{
    UiMeshPlan, UiRenderBlend, UiRenderQuad, UiRenderSource, UiRenderState, UiRenderTransform,
    UiTextureAddressMode, UiTextureResidency,
};

use crate::{
    UiBlendMode, UiGlyphAtlasPlan, UiGlyphQuad, UiPresentationPlan, UiRegionGeometryPlan,
    UiRenderError, UiScrollFramePlan, UiTextureAssetPlan, UiTexturePresentation, UiTextureSource,
};
use crate::{UiObjectRole, script::UiRuntimeObjectPlan};

/// Renderer-owned mesh data derived from one complete live presentation pass.
#[derive(Clone, Debug, PartialEq)]
pub struct UiRenderPlan {
    mesh: UiMeshPlan,
    texture_assets: UiTextureAssetPlan,
}

impl UiRenderPlan {
    /// Flattens stock packet order into shared geometry and material runs.
    ///
    /// Source-less `SetTexture(r, g, b, a)` regions multiply that color into
    /// each live corner before selecting the renderer's vertex-color pipeline.
    /// Texture-backed regions retain their canonical MPQ path and exact tiling,
    /// blend, and residency state.
    ///
    /// # Errors
    ///
    /// Returns [`UiRenderError`] when live positions or mesh counts cannot be
    /// represented by the renderer's fixed 32-bit UI ABI.
    pub fn prepare(
        presentation: &UiPresentationPlan,
        logical_extent: (f64, f64),
    ) -> Result<Self, UiRenderError> {
        let quads = presentation.members_in_draw_order().iter().map(render_quad);
        let mesh = UiMeshPlan::prepare([logical_extent.0 as f32, logical_extent.1 as f32], quads)?;
        let texture_assets = UiTextureAssetPlan::prepare(&mesh)?;
        Ok(Self {
            mesh,
            texture_assets,
        })
    }

    /// Interleaves live textures and clipped glyphs by the shared stock packet key.
    ///
    /// # Errors
    ///
    /// Returns [`UiRenderError`] under the same fixed mesh ABI constraints as
    /// [`Self::prepare`].
    pub fn prepare_with_glyphs(
        presentation: &UiPresentationPlan,
        glyphs: &UiGlyphAtlasPlan,
        geometry: &UiRegionGeometryPlan,
        scroll_frames: &UiScrollFramePlan,
        logical_extent: (f64, f64),
    ) -> Result<Self, UiRenderError> {
        let mut ordered = presentation
            .members_in_draw_order()
            .iter()
            .enumerate()
            .filter_map(|(sequence, texture)| {
                let quad = render_quad_with_scroll(texture, geometry, scroll_frames)?;
                Some((Some(texture.key()), texture.object_index(), sequence, quad))
            })
            .collect::<Vec<_>>();
        let texture_count = ordered.len();
        ordered.extend(
            glyphs
                .retained_scroll_quads(geometry, scroll_frames)
                .into_iter()
                .enumerate()
                .filter_map(|(sequence, quad)| {
                    let rendered =
                        render_glyph_quad(glyphs.identity(), &quad, geometry, scroll_frames)?;
                    Some((
                        quad.packet_key(),
                        quad.object_index(),
                        texture_count + sequence,
                        rendered,
                    ))
                }),
        );
        ordered.sort_by_key(|(key, object_index, sequence, _)| {
            (key.is_none(), *key, *object_index, *sequence)
        });
        let mesh = UiMeshPlan::prepare(
            [logical_extent.0 as f32, logical_extent.1 as f32],
            ordered.into_iter().map(|(_, _, _, quad)| quad),
        )?;
        let texture_assets = UiTextureAssetPlan::prepare(&mesh)?;
        Ok(Self {
            mesh,
            texture_assets,
        })
    }

    /// Returns upload-ready vertices, indices, and adjacent material batches.
    #[must_use]
    pub const fn mesh(&self) -> &UiMeshPlan {
        &self.mesh
    }

    /// Returns unique MPQ image requests and their batch associations.
    #[must_use]
    pub const fn texture_assets(&self) -> &UiTextureAssetPlan {
        &self.texture_assets
    }

    /// Refreshes animation-only draw state without touching vertex/index bytes.
    ///
    /// Returns `false` if the new geometry cannot be represented as rigid
    /// per-object translation and therefore requires a complete mesh rebuild.
    pub(crate) fn refresh_visual_states(
        &mut self,
        previous: &UiRegionGeometryPlan,
        current: &UiRegionGeometryPlan,
        presentation: &UiPresentationPlan,
        scroll_frames: &UiScrollFramePlan,
    ) -> Result<bool, UiRenderError> {
        if previous.region_count() != current.region_count() {
            return Ok(false);
        }
        let mut translations = Vec::with_capacity(current.region_count());
        for object_index in 0..current.region_count() {
            let (Some(previous), Some(current)) =
                (previous.region(object_index), current.region(object_index))
            else {
                return Ok(false);
            };
            let previous = previous.presentation_bounds();
            let current = current.presentation_bounds();
            let horizontal = current.left() - previous.left();
            let vertical = current.bottom() - previous.bottom();
            if !nearly_equal(current.right() - previous.right(), horizontal)
                || !nearly_equal(current.top() - previous.top(), vertical)
            {
                return Ok(false);
            }
            let translation = [horizontal as f32, vertical as f32];
            if translation.iter().any(|value| !value.is_finite()) {
                return Ok(false);
            }
            translations.push(translation);
        }
        for (object_index, translation) in translations.into_iter().enumerate() {
            self.mesh.translate_object(object_index, translation)?;
        }
        self.mesh.refresh_object_opacities(|object_index| {
            presentation.object_opacity(object_index).or_else(|| {
                current
                    .region(object_index)
                    .map(|region| region.effective_alpha() as f32)
            })
        })?;
        for object_index in 0..current.region_count() {
            if scroll_frames.state(object_index).is_none() {
                continue;
            }
            let Some(viewport) = current.region(object_index) else {
                return Ok(false);
            };
            let viewport = viewport.presentation_bounds();
            self.mesh.set_transform_clip(
                UiRenderTransform::ScrollFrame(object_index),
                Some([
                    viewport.left() as f32,
                    viewport.bottom() as f32,
                    viewport.right() as f32,
                    viewport.top() as f32,
                ]),
            )?;
        }
        Ok(true)
    }

    /// Patches only draw slots owned by animation-affected regions.
    pub(crate) fn refresh_visual_objects(
        &mut self,
        changes: &[crate::region::UiRegionVisualChange],
        geometry: &UiRegionGeometryPlan,
        presentation: &UiPresentationPlan,
        scroll_frames: &UiScrollFramePlan,
        live: &UiRuntimeObjectPlan,
    ) -> Result<(), UiRenderError> {
        for change in changes {
            self.mesh
                .translate_object(change.object_index, change.translation)?;
            let opacity = presentation
                .object_opacity(change.object_index)
                .or_else(|| {
                    geometry
                        .region(change.object_index)
                        .map(|region| region.effective_alpha() as f32)
                })
                .unwrap_or(1.0);
            self.mesh.set_object_opacity(change.object_index, opacity)?;
            if let Some(text) = live
                .objects()
                .get(change.object_index)
                .and_then(|object| object.text.as_ref())
                && live.objects()[change.object_index].kind == crate::UiObjectKind::EditBox
            {
                self.mesh.set_state_opacity(
                    UiRenderState::EditBoxCaret(change.object_index),
                    opacity * f32::from(text.caret_visible),
                )?;
            }
            if scroll_frames.state(change.object_index).is_some()
                && let Some(viewport) = geometry.region(change.object_index)
            {
                let viewport = viewport.presentation_bounds();
                self.mesh.set_transform_clip(
                    UiRenderTransform::ScrollFrame(change.object_index),
                    Some([
                        viewport.left() as f32,
                        viewport.bottom() as f32,
                        viewport.right() as f32,
                        viewport.top() as f32,
                    ]),
                )?;
            }
        }
        Ok(())
    }

    /// Patches retained Button state slots without rebuilding geometry bytes.
    pub(crate) fn refresh_button_state_opacities(
        &mut self,
        presentation: &UiPresentationPlan,
    ) -> Result<(), UiRenderError> {
        let mut opacities = Vec::<Option<f32>>::new();
        for member in presentation.members_in_draw_order() {
            if member.object_index() >= opacities.len() {
                opacities.resize(member.object_index() + 1, None);
            }
            opacities[member.object_index()] = Some(member.opacity());
        }
        self.mesh.refresh_object_opacities(|object_index| {
            opacities.get(object_index).copied().flatten()
        })?;
        Ok(())
    }

    /// Patches ScrollFrame and native thumb draw state without rebuilding mesh bytes.
    pub(crate) fn refresh_scroll_transforms(
        &mut self,
        previous: &UiRuntimeObjectPlan,
        current: &UiRuntimeObjectPlan,
        geometry: &mut UiRegionGeometryPlan,
        presentation: &mut UiPresentationPlan,
    ) {
        for (object_index, (old, new)) in
            previous.objects().iter().zip(current.objects()).enumerate()
        {
            if old.scroll_offset != new.scroll_offset
                && let (Some((horizontal, vertical)), Some(child)) =
                    (new.scroll_offset, new.scroll_child)
                && let Some(child_geometry) = geometry.region(child)
            {
                let scale = child_geometry.effective_scale() as f32;
                self.mesh.set_transform_translation(
                    UiRenderTransform::ScrollFrame(object_index),
                    [-(horizontal as f32) * scale, vertical as f32 * scale],
                );
            }
            let (Some(old_slider), Some(new_slider)) = (old.slider, new.slider) else {
                continue;
            };
            if old_slider.value == new_slider.value {
                continue;
            }
            let Some(thumb_index) = current.objects().iter().position(|candidate| {
                candidate.parent == Some(object_index)
                    && candidate.role == UiObjectRole::ThumbTexture
                    && current.anchors_for(candidate).is_empty()
            }) else {
                continue;
            };
            let (Some(track), Some(thumb)) =
                (geometry.region(object_index), geometry.region(thumb_index))
            else {
                continue;
            };
            let old_fraction =
                slider_fraction(old_slider.minimum, old_slider.maximum, old_slider.value);
            let new_fraction =
                slider_fraction(new_slider.minimum, new_slider.maximum, new_slider.value);
            let fraction_delta = (new_fraction - old_fraction) as f32;
            let track = track.presentation_bounds();
            let thumb = thumb.presentation_bounds();
            let delta = if new_slider.vertical {
                [
                    0.0,
                    -fraction_delta * (track.height() - thumb.height()).max(0.0) as f32,
                ]
            } else {
                [
                    fraction_delta * (track.width() - thumb.width()).max(0.0) as f32,
                    0.0,
                ]
            };
            self.mesh
                .translate_transform(UiRenderTransform::Slider(object_index), delta);
            geometry.translate_region(thumb_index, delta);
            presentation.translate_object(thumb_index, delta);
        }
    }
}

fn nearly_equal(left: f64, right: f64) -> bool {
    let magnitude = left.abs().max(right.abs()).max(1.0);
    (left - right).abs() <= f64::EPSILON * magnitude * 16.0
}

fn slider_fraction(minimum: f64, maximum: f64, value: f64) -> f64 {
    if maximum > minimum {
        ((value - minimum) / (maximum - minimum)).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Converts one already clipped coverage glyph into a sampled UI quad.
fn render_glyph_quad(
    atlas_identity: u64,
    glyph: &UiGlyphQuad,
    geometry: &UiRegionGeometryPlan,
    scroll_frames: &UiScrollFramePlan,
) -> Option<UiRenderQuad> {
    let color = glyph.color();
    let quad = UiRenderQuad::new(
        glyph.object_index(),
        UiRenderSource::GlyphAtlas(atlas_identity),
        UiRenderBlend::Alpha,
        UiTextureAddressMode::Clamp,
        UiTextureAddressMode::Clamp,
        UiTextureResidency::Blocking,
        false,
        glyph.bounds(),
        glyph.texture_coordinates(),
        [color; 4],
    )
    .with_opacity(
        geometry
            .region(glyph.object_index())
            .map_or(1.0, |region| region.effective_alpha() as f32),
    );
    let quad = attach_scroll_transform(quad, glyph.clip_object(), geometry, scroll_frames)?;
    Some(if glyph.is_caret() {
        quad.with_state(UiRenderState::EditBoxCaret(glyph.object_index()))
    } else {
        quad
    })
}

/// Converts one live texture region without disturbing its established order.
fn render_quad(texture: &UiTexturePresentation) -> UiRenderQuad {
    let bounds = texture.bounds();
    render_quad_parts(
        texture,
        [
            bounds.left() as f32,
            bounds.bottom() as f32,
            bounds.right() as f32,
            bounds.top() as f32,
        ],
        texture_coordinates(texture.tex_coords()),
        texture.vertex_colors(),
    )
}

/// Applies the assigned ScrollFrame translation and viewport to one texture.
fn render_quad_with_scroll(
    texture: &UiTexturePresentation,
    geometry: &UiRegionGeometryPlan,
    scroll_frames: &UiScrollFramePlan,
) -> Option<UiRenderQuad> {
    let quad = render_quad(texture);
    if let Some(clip_object) = texture.clip_object() {
        return attach_scroll_transform(quad, Some(clip_object), geometry, scroll_frames);
    }
    if let Some(slider) = texture.slider_object() {
        return Some(quad.with_transform(UiRenderTransform::Slider(slider), [0.0, 0.0], None));
    }
    Some(quad)
}

/// Attaches a fixed viewport and current translation to immutable child geometry.
fn attach_scroll_transform(
    quad: UiRenderQuad,
    clip_object: Option<usize>,
    geometry: &UiRegionGeometryPlan,
    scroll_frames: &UiScrollFramePlan,
) -> Option<UiRenderQuad> {
    let Some(clip_object) = clip_object else {
        return Some(quad);
    };
    let Some(scroll) = scroll_frames.state(clip_object) else {
        // EditBox text uses its own region as a CPU clip and does not carry a
        // ScrollFrame transform slot.
        return Some(quad);
    };
    let child_scale = geometry.region(scroll.child()?)?.effective_scale() as f32;
    let viewport = geometry.region(clip_object)?.presentation_bounds();
    let (horizontal, vertical) = scroll.offset();
    Some(quad.with_transform(
        UiRenderTransform::ScrollFrame(clip_object),
        [
            -(horizontal as f32) * child_scale,
            vertical as f32 * child_scale,
        ],
        Some([
            viewport.left() as f32,
            viewport.bottom() as f32,
            viewport.right() as f32,
            viewport.top() as f32,
        ]),
    ))
}

/// Preserves one presentation's material policy around adjusted quad fields.
fn render_quad_parts(
    texture: &UiTexturePresentation,
    bounds: [f32; 4],
    coordinates: [[f32; 2]; 4],
    colors: [[f32; 4]; 4],
) -> UiRenderQuad {
    let (source, solid_color) = match texture.source() {
        UiTextureSource::Asset(path) => (UiRenderSource::Texture(path.clone()), None),
        UiTextureSource::SolidColor(color) => (UiRenderSource::VertexColor, Some(*color)),
    };
    let colors = colors.map(|color| {
        solid_color.map_or(color, |solid| {
            [
                color[0] * solid[0],
                color[1] * solid[1],
                color[2] * solid[2],
                color[3] * solid[3],
            ]
        })
    });
    UiRenderQuad::new(
        texture.object_index(),
        source,
        match texture.blend_mode() {
            UiBlendMode::Blend => UiRenderBlend::Alpha,
            UiBlendMode::Add => UiRenderBlend::Additive,
        },
        address_mode(texture.horizontal_tiling()),
        address_mode(texture.vertical_tiling()),
        if texture.non_blocking() {
            UiTextureResidency::NonBlocking
        } else {
            UiTextureResidency::Blocking
        },
        texture.desaturated(),
        bounds,
        coordinates,
        colors,
    )
    .with_opacity(texture.opacity())
}

/// Groups the flattened stock corner order for interpolation and upload.
fn texture_coordinates(coordinates: [f32; 8]) -> [[f32; 2]; 4] {
    [
        [coordinates[0], coordinates[1]],
        [coordinates[2], coordinates[3]],
        [coordinates[4], coordinates[5]],
        [coordinates[6], coordinates[7]],
    ]
}

/// Maps each independently authored tiling flag onto Vulkan sampler vocabulary.
const fn address_mode(is_tiled: bool) -> UiTextureAddressMode {
    if is_tiled {
        UiTextureAddressMode::Repeat
    } else {
        UiTextureAddressMode::Clamp
    }
}
