//! Screen-space resolution for the live post-Lua region arena.

mod affine;
mod dependencies;
mod refresh;
mod resolver;

use crate::UiLayoutError;
use crate::script::UiRuntimeObjectPlan;
use affine::Affine2;
use dependencies::RegionDependencies;

/// One axis-aligned rectangle in the stock bottom-left UI coordinate system.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiScreenRect {
    left: f64,
    bottom: f64,
    right: f64,
    top: f64,
}

impl UiScreenRect {
    pub(crate) const fn from_edges(left: f64, bottom: f64, right: f64, top: f64) -> Self {
        Self {
            left,
            bottom,
            right,
            top,
        }
    }

    /// Returns the left edge in logical UI units.
    #[must_use]
    pub const fn left(self) -> f64 {
        self.left
    }

    /// Returns the lower edge in logical UI units.
    #[must_use]
    pub const fn bottom(self) -> f64 {
        self.bottom
    }

    /// Returns the right edge in logical UI units.
    #[must_use]
    pub const fn right(self) -> f64 {
        self.right
    }

    /// Returns the upper edge in logical UI units.
    #[must_use]
    pub const fn top(self) -> f64 {
        self.top
    }

    /// Returns the nonnegative rectangle width.
    #[must_use]
    pub fn width(self) -> f64 {
        self.right - self.left
    }

    /// Returns the nonnegative rectangle height.
    #[must_use]
    pub fn height(self) -> f64 {
        self.top - self.bottom
    }

    pub(crate) const fn translated(self, delta: [f32; 2]) -> Self {
        Self {
            left: self.left + delta[0] as f64,
            bottom: self.bottom + delta[1] as f64,
            right: self.right + delta[0] as f64,
            top: self.top + delta[1] as f64,
        }
    }

    fn scaled(self, scale: f64) -> Self {
        Self {
            left: self.left * scale,
            bottom: self.bottom * scale,
            right: self.right * scale,
            top: self.top * scale,
        }
    }
}

/// 4893C0 shifts left/bottom first, then right/top; the latter win for oversize frames.
/// Coordinates and insets are in the region's own units, including the screen extent.
pub(crate) fn clamp_screen_rect(
    mut bounds: UiScreenRect,
    screen: (f64, f64),
    [left, right, top, bottom]: [f64; 4],
) -> UiScreenRect {
    if bounds.left < -left {
        bounds.right -= bounds.left + left;
        bounds.left = -left;
    }
    if bounds.bottom < -bottom {
        bounds.top -= bounds.bottom + bottom;
        bounds.bottom = -bottom;
    }
    if bounds.right > screen.0 - right {
        bounds.left -= bounds.right - (screen.0 - right);
        bounds.right = screen.0 - right;
    }
    if bounds.top > screen.1 - top {
        bounds.bottom -= bounds.top - (screen.1 - top);
        bounds.top = screen.1 - top;
    }
    bounds
}

/// Resolved startup geometry and inherited presentation state for one region.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiRegionGeometry {
    logical_bounds: UiScreenRect,
    presentation_bounds: UiScreenRect,
    effectively_shown: bool,
    effective_alpha: f64,
    effective_scale: f64,
    animation_active: bool,
}

impl UiRegionGeometry {
    /// Returns the rectangle in the region's own scaled coordinate units.
    #[must_use]
    pub const fn logical_bounds(self) -> UiScreenRect {
        self.logical_bounds
    }

    /// Returns bounds after local and ancestor scale are composed.
    #[must_use]
    pub const fn presentation_bounds(self) -> UiScreenRect {
        self.presentation_bounds
    }

    /// Returns visibility after applying the live ownership chain.
    #[must_use]
    pub const fn effectively_shown(self) -> bool {
        self.effectively_shown
    }

    /// Returns alpha multiplied through the ownership chain.
    #[must_use]
    pub const fn effective_alpha(self) -> f64 {
        self.effective_alpha
    }

    /// Returns scale multiplied through the ownership chain.
    #[must_use]
    pub const fn effective_scale(self) -> f64 {
        self.effective_scale
    }

    /// Returns whether this region or an ancestor owns a playing animation.
    #[must_use]
    pub const fn animation_active(self) -> bool {
        self.animation_active
    }
}

/// Dense geometry parallel to the live Glue object arena.
#[derive(Clone)]
pub struct UiRegionGeometryPlan {
    ui_extent: (f64, f64),
    regions: Vec<UiRegionGeometry>,
    presentations: Vec<Affine2>,
    dependencies: RegionDependencies,
}

pub(crate) struct UiRegionGeometryRefresh {
    /// Previous geometry in affected-object order, retained only for this island.
    pub(crate) previous_regions: Vec<UiRegionGeometry>,
    pub(crate) affected_objects: Vec<usize>,
    pub(crate) changed_objects: Vec<usize>,
}

impl UiRegionGeometryPlan {
    /// Moves one transform-only region while retaining the solved dependency graph.
    pub(crate) fn translate_region(&mut self, object_index: usize, delta: [f32; 2]) {
        if let Some(region) = self.regions.get_mut(object_index) {
            region.logical_bounds = region.logical_bounds.translated(delta);
            region.presentation_bounds = region.presentation_bounds.translated(delta);
        }
    }

    /// Recomputes presentation-only state for a parent-before-child list.
    ///
    /// Animation alpha and translation never affect anchor resolution or
    /// logical bounds. Retaining each composed affine transform therefore lets
    /// the frame tick patch only the animated ownership subtrees.
    pub(crate) fn refresh_visual_regions(
        &mut self,
        live: &UiRuntimeObjectPlan,
        object_indices: &[usize],
    ) -> Vec<UiRegionVisualChange> {
        let mut changes = Vec::with_capacity(object_indices.len());
        for &object_index in object_indices {
            let Some(object) = live.objects().get(object_index) else {
                continue;
            };
            let Some(previous) = self.regions.get(object_index).copied() else {
                continue;
            };
            let parent_region = object
                .parent
                .and_then(|index| self.regions.get(index))
                .copied();
            let parent_transform = object
                .parent
                .and_then(|index| self.presentations.get(index))
                .copied()
                .unwrap_or(Affine2::IDENTITY);
            let logical_bounds = previous.logical_bounds;
            let scale_transform = Affine2::scale_about(object.scale, 0.0, 0.0);
            let local = Affine2::translation(object.animation_offset.0, object.animation_offset.1)
                .compose(scale_transform);
            let presentation = parent_transform.compose(local);
            let presentation_bounds = presentation.bounds(logical_bounds);
            let alpha = (object.alpha + object.animation_alpha_delta).clamp(0.0, 1.0);
            let effective_alpha =
                alpha * parent_region.map_or(1.0, UiRegionGeometry::effective_alpha);
            let effectively_shown =
                object.shown && parent_region.is_none_or(UiRegionGeometry::effectively_shown);
            let animation_active = object.animation_active
                || parent_region.is_some_and(UiRegionGeometry::animation_active);
            self.presentations[object_index] = presentation;
            self.regions[object_index].presentation_bounds = presentation_bounds;
            self.regions[object_index].effectively_shown = effectively_shown;
            self.regions[object_index].effective_alpha = effective_alpha;
            self.regions[object_index].animation_active = animation_active;
            changes.push(UiRegionVisualChange {
                object_index,
                translation: [
                    (presentation_bounds.left() - previous.presentation_bounds.left()) as f32,
                    (presentation_bounds.bottom() - previous.presentation_bounds.bottom()) as f32,
                ],
            });
        }
        changes
    }
}

/// One retained region slot affected by an animation-owner subtree update.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct UiRegionVisualChange {
    pub(crate) object_index: usize,
    pub(crate) translation: [f32; 2],
}

fn resolution_error(message: impl Into<String>) -> UiLayoutError {
    UiLayoutError::Resolution {
        message: message.into(),
    }
}
