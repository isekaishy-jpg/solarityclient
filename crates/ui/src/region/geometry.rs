//! Screen-space resolution for the live post-Lua region arena.

use crate::script::{UiRuntimeAnchor, UiRuntimeObjectPlan};
use crate::{UiLayoutError, UiObjectRole, UiPoint};

const AXIS_EPSILON: f64 = 0.0001;
const MAX_REGION_ANCHORS: usize = 9;

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
    /// Returns the untransformed rectangle used by anchor dependencies.
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
            let scale_transform = Affine2::scale_about(
                object.scale,
                (logical_bounds.left + logical_bounds.right) * 0.5,
                (logical_bounds.bottom + logical_bounds.top) * 0.5,
            );
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

    /// Re-solves the transitive geometry island rooted at changed objects.
    ///
    /// Parent inheritance and authored anchor targets are the only edges that
    /// can carry a region mutation to another object. Keeping every unrelated
    /// resolved slot as a seed avoids walking the complete Glue arena for a
    /// tooltip or another small dynamic layout island.
    pub(crate) fn refresh_dependency_regions(
        &mut self,
        live: &UiRuntimeObjectPlan,
        root_indices: impl IntoIterator<Item = usize>,
    ) -> Result<Vec<usize>, UiLayoutError> {
        if live.objects().len() != self.regions.len()
            || self.presentations.len() != self.regions.len()
        {
            return Err(resolution_error(
                "live and retained geometry arenas have different sizes",
            ));
        }
        let mut affected = vec![false; self.regions.len()];
        for object_index in root_indices {
            let Some(slot) = affected.get_mut(object_index) else {
                return Err(resolution_error(format!(
                    "geometry refresh root {object_index} is outside the arena"
                )));
            };
            *slot = true;
        }
        loop {
            let mut added = false;
            for (object_index, object) in live.objects().iter().enumerate() {
                if affected[object_index] {
                    continue;
                }
                let depends_on_changed = object.parent.is_some_and(|parent| affected[parent])
                    || live
                        .anchors_for(object)
                        .iter()
                        .any(|anchor| anchor.target.is_some_and(|target| affected[target]));
                if depends_on_changed {
                    affected[object_index] = true;
                    added = true;
                }
            }
            if !added {
                break;
            }
        }
        let object_indices = affected
            .iter()
            .enumerate()
            .filter_map(|(object_index, affected)| affected.then_some(object_index))
            .collect::<Vec<_>>();
        if object_indices.is_empty() {
            return Ok(object_indices);
        }
        let screen = UiScreenRect {
            left: 0.0,
            bottom: 0.0,
            right: self.ui_extent.0,
            top: self.ui_extent.1,
        };
        let resolved = self
            .regions
            .iter()
            .copied()
            .zip(self.presentations.iter().copied())
            .enumerate()
            .map(|(object_index, (public, presentation))| {
                (!affected[object_index]).then_some(ResolvedRegion {
                    public,
                    presentation,
                })
            })
            .collect();
        let mut resolver = GeometryResolver {
            live,
            screen,
            resolved,
            visiting: vec![false; live.objects().len()],
        };
        for &object_index in &object_indices {
            resolver.resolve(object_index)?;
        }
        for &object_index in &object_indices {
            let resolved = resolver.resolved[object_index].ok_or_else(|| {
                resolution_error(format!(
                    "refreshed live region {object_index} remained unresolved"
                ))
            })?;
            self.regions[object_index] = resolved.public;
            self.presentations[object_index] = resolved.presentation;
        }
        Ok(object_indices)
    }
}

/// One retained region slot affected by an animation-owner subtree update.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct UiRegionVisualChange {
    pub(crate) object_index: usize,
    pub(crate) translation: [f32; 2],
}

impl UiRegionGeometryPlan {
    pub(crate) fn resolve(
        live: &UiRuntimeObjectPlan,
        ui_extent: (f64, f64),
    ) -> Result<Self, UiLayoutError> {
        if !ui_extent.0.is_finite()
            || !ui_extent.1.is_finite()
            || ui_extent.0 <= 0.0
            || ui_extent.1 <= 0.0
        {
            return Err(resolution_error(
                "UI geometry extent must be finite and positive",
            ));
        }
        let screen = UiScreenRect {
            left: 0.0,
            bottom: 0.0,
            right: ui_extent.0,
            top: ui_extent.1,
        };
        let mut resolver = GeometryResolver {
            live,
            screen,
            resolved: vec![None; live.objects().len()],
            visiting: vec![false; live.objects().len()],
        };
        for index in 0..live.objects().len() {
            resolver.resolve(index)?;
        }
        let resolved = resolver
            .resolved
            .into_iter()
            .enumerate()
            .map(|(index, region)| {
                region.ok_or_else(|| {
                    resolution_error(format!("live region {index} remained unresolved"))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let regions = resolved.iter().map(|region| region.public).collect();
        let presentations = resolved.iter().map(|region| region.presentation).collect();
        Ok(Self {
            ui_extent,
            regions,
            presentations,
        })
    }

    /// Returns the stock aspect-compensated logical canvas extent.
    #[must_use]
    pub const fn ui_extent(&self) -> (f64, f64) {
        self.ui_extent
    }

    /// Returns geometry for one live object-arena index.
    #[must_use]
    pub fn region(&self, object_index: usize) -> Option<UiRegionGeometry> {
        self.regions.get(object_index).copied()
    }

    /// Returns the number of live resolved regions.
    #[must_use]
    pub fn region_count(&self) -> usize {
        self.regions.len()
    }
}

#[derive(Clone, Copy)]
struct ResolvedRegion {
    public: UiRegionGeometry,
    presentation: Affine2,
}

struct GeometryResolver<'plan> {
    live: &'plan UiRuntimeObjectPlan,
    screen: UiScreenRect,
    resolved: Vec<Option<ResolvedRegion>>,
    visiting: Vec<bool>,
}

impl GeometryResolver<'_> {
    fn resolve(&mut self, index: usize) -> Result<ResolvedRegion, UiLayoutError> {
        if let Some(region) = self.resolved.get(index).copied().flatten() {
            return Ok(region);
        }
        let Some(visiting) = self.visiting.get_mut(index) else {
            return Err(resolution_error(format!(
                "live region index {index} is outside the arena"
            )));
        };
        if std::mem::replace(visiting, true) {
            return Err(resolution_error(format!(
                "live region anchor cycle reaches object {index}"
            )));
        }

        let result = self.resolve_inner(index);
        self.visiting[index] = false;
        let region = result?;
        self.resolved[index] = Some(region);
        Ok(region)
    }

    fn resolve_inner(&mut self, index: usize) -> Result<ResolvedRegion, UiLayoutError> {
        let object = self.live.objects().get(index).ok_or_else(|| {
            resolution_error(format!("live region index {index} has no object state"))
        })?;
        let parent_index = object.parent;
        let authored = (object.width, object.height);
        let shown = object.shown;
        let alpha = (object.alpha + object.animation_alpha_delta).clamp(0.0, 1.0);
        let scale = object.scale;
        let role = object.role;
        let empty_anchor = UiRuntimeAnchor {
            point: UiPoint::Center,
            target: None,
            relative_point: UiPoint::Center,
            offset: (0.0, 0.0),
        };
        let live_anchors = self.live.anchors_for(object);
        debug_assert!(live_anchors.len() <= MAX_REGION_ANCHORS);
        let authored_anchor_count = live_anchors.len();
        let mut authored_anchor_storage = [empty_anchor; MAX_REGION_ANCHORS];
        authored_anchor_storage[..authored_anchor_count].copy_from_slice(live_anchors);
        let authored_anchors = &authored_anchor_storage[..authored_anchor_count];
        let mut synthesized_anchors = [empty_anchor; 2];
        let mut synthesized_anchor_count = 0;
        let bar_fill = parent_index
            .and_then(|parent| self.live.objects()[parent].status_bar)
            .filter(|(_, texture)| *texture == Some(index))
            .and_then(|(bar, _)| {
                bar.fill_fraction
                    .map(|fraction| (bar.fill_vertical, fraction))
            });
        if let Some((vertical, fraction)) = bar_fill
            && let Some(parent) = parent_index
        {
            let bounds = self.resolve(parent)?.public.logical_bounds;
            synthesized_anchors[0] = UiRuntimeAnchor {
                point: UiPoint::BottomLeft,
                target: Some(parent),
                relative_point: UiPoint::BottomLeft,
                offset: (0.0, 0.0),
            };
            synthesized_anchors[1] = UiRuntimeAnchor {
                point: UiPoint::TopRight,
                target: Some(parent),
                relative_point: UiPoint::TopRight,
                offset: if vertical {
                    (0.0, -(1.0 - fraction) * bounds.height())
                } else {
                    (-(1.0 - fraction) * bounds.width(), 0.0)
                },
            };
            synthesized_anchor_count = 2;
        }
        if authored_anchors.is_empty()
            && role == UiObjectRole::ThumbTexture
            && let Some(parent_index) = parent_index
            && let Some(slider) = self.live.objects()[parent_index].slider
        {
            let parent_bounds = self.resolve(parent_index)?.public.logical_bounds;
            let fraction = if slider.maximum > slider.minimum {
                ((slider.value - slider.minimum) / (slider.maximum - slider.minimum))
                    .clamp(0.0, 1.0)
            } else {
                0.0
            };
            let (relative_point, offset) = if slider.vertical {
                (
                    UiPoint::Top,
                    (
                        0.0,
                        -authored.1 * 0.5
                            - fraction * (parent_bounds.height() - authored.1).max(0.0),
                    ),
                )
            } else {
                (
                    UiPoint::Left,
                    (
                        authored.0 * 0.5 + fraction * (parent_bounds.width() - authored.0).max(0.0),
                        0.0,
                    ),
                )
            };
            synthesized_anchors[0] = UiRuntimeAnchor {
                point: UiPoint::Center,
                target: Some(parent_index),
                relative_point,
                offset,
            };
            synthesized_anchor_count = 1;
        }
        if authored_anchors.is_empty()
            && synthesized_anchor_count == 0
            && let Some(parent) = parent_index
        {
            if stock_role_texture_fills_owner(role) && authored == (0.0, 0.0) {
                // Singular button and check-button texture slots are native
                // widget regions. Stock gives an otherwise geometry-free slot
                // the owner's complete rectangle; GlueXML relies on this for
                // every file-only GluePanelButton and checkbox state texture.
                synthesized_anchors[0] = UiRuntimeAnchor {
                    point: UiPoint::TopLeft,
                    target: Some(parent),
                    relative_point: UiPoint::TopLeft,
                    offset: (0.0, 0.0),
                };
                synthesized_anchors[1] = UiRuntimeAnchor {
                    point: UiPoint::BottomRight,
                    target: Some(parent),
                    relative_point: UiPoint::BottomRight,
                    offset: (0.0, 0.0),
                };
                synthesized_anchor_count = 2;
            } else if !matches!(role, UiObjectRole::Object | UiObjectRole::ScrollChild) {
                synthesized_anchors[0] = UiRuntimeAnchor {
                    point: UiPoint::Center,
                    target: Some(parent),
                    relative_point: UiPoint::Center,
                    offset: (0.0, 0.0),
                };
                synthesized_anchor_count = 1;
            }
        }

        let anchors = if authored_anchors.is_empty() || bar_fill.is_some() {
            &synthesized_anchors[..synthesized_anchor_count]
        } else {
            authored_anchors
        };
        debug_assert!(anchors.len() <= MAX_REGION_ANCHORS);
        let empty_constraint = AxisConstraint {
            factor: 0.0,
            value: 0.0,
        };
        let mut x_constraints = [empty_constraint; MAX_REGION_ANCHORS];
        let mut y_constraints = [empty_constraint; MAX_REGION_ANCHORS];
        let mut all_x = [empty_constraint; MAX_REGION_ANCHORS];
        let mut all_y = [empty_constraint; MAX_REGION_ANCHORS];
        let mut x_constraint_count = 0;
        let mut y_constraint_count = 0;
        for (anchor_index, anchor) in anchors.iter().copied().enumerate() {
            let target = match anchor.target {
                Some(target) => self.resolve(target)?.public.logical_bounds,
                None => self.screen,
            };
            let x = AxisConstraint {
                factor: point_x_factor(anchor.point),
                value: axis_coordinate(
                    target.left,
                    target.right,
                    point_x_factor(anchor.relative_point),
                ) + anchor.offset.0,
            };
            let y = AxisConstraint {
                factor: point_y_factor(anchor.point),
                value: axis_coordinate(
                    target.bottom,
                    target.top,
                    point_y_factor(anchor.relative_point),
                ) + anchor.offset.1,
            };
            all_x[anchor_index] = x;
            all_y[anchor_index] = y;
            if constrains_x(anchor.point) {
                x_constraints[x_constraint_count] = x;
                x_constraint_count += 1;
            }
            if constrains_y(anchor.point) {
                y_constraints[y_constraint_count] = y;
                y_constraint_count += 1;
            }
        }

        let parent = parent_index
            .map(|parent| self.resolve(parent))
            .transpose()?;
        let fallback_left = parent.map_or(0.0, |region| region.public.logical_bounds.left);
        // A stock ScrollChild without authored points begins at the scroll
        // frame's top-left. Its content height then extends downward and feeds
        // the native vertical range rather than moving the first line upward.
        let fallback_bottom = parent.map_or(0.0, |region| {
            if role == UiObjectRole::ScrollChild {
                region.public.logical_bounds.top - authored.1.max(0.0)
            } else {
                region.public.logical_bounds.bottom
            }
        });
        let horizontal = solve_axis(
            authored.0,
            if x_constraint_count == 0 {
                &all_x[..anchors.len()]
            } else {
                &x_constraints[..x_constraint_count]
            },
            fallback_left,
        )?;
        let vertical = solve_axis(
            authored.1,
            if y_constraint_count == 0 {
                &all_y[..anchors.len()]
            } else {
                &y_constraints[..y_constraint_count]
            },
            fallback_bottom,
        )?;
        let logical_bounds = UiScreenRect {
            left: horizontal.0,
            bottom: vertical.0,
            right: horizontal.0 + horizontal.1,
            top: vertical.0 + vertical.1,
        };
        let scale_transform = Affine2::scale_about(
            scale,
            (logical_bounds.left + logical_bounds.right) * 0.5,
            (logical_bounds.bottom + logical_bounds.top) * 0.5,
        );
        let local = Affine2::translation(object.animation_offset.0, object.animation_offset.1)
            .compose(scale_transform);
        let parent_transform = parent.map_or(Affine2::IDENTITY, |region| region.presentation);
        let presentation = parent_transform.compose(local);
        let presentation_bounds = presentation.bounds(logical_bounds);
        let effectively_shown =
            shown && parent.is_none_or(|region| region.public.effectively_shown);
        let effective_alpha = alpha * parent.map_or(1.0, |region| region.public.effective_alpha);
        let effective_scale = scale * parent.map_or(1.0, |region| region.public.effective_scale);
        let animation_active =
            object.animation_active || parent.is_some_and(|region| region.public.animation_active);
        Ok(ResolvedRegion {
            public: UiRegionGeometry {
                logical_bounds,
                presentation_bounds,
                effectively_shown,
                effective_alpha,
                effective_scale,
                animation_active,
            },
            presentation,
        })
    }
}

const fn stock_role_texture_fills_owner(role: UiObjectRole) -> bool {
    matches!(
        role,
        UiObjectRole::NormalTexture
            | UiObjectRole::PushedTexture
            | UiObjectRole::DisabledTexture
            | UiObjectRole::HighlightTexture
            | UiObjectRole::CheckedTexture
            | UiObjectRole::DisabledCheckedTexture
    )
}

#[derive(Clone, Copy)]
struct AxisConstraint {
    factor: f64,
    value: f64,
}

fn solve_axis(
    authored_extent: f64,
    constraints: &[AxisConstraint],
    fallback_begin: f64,
) -> Result<(f64, f64), UiLayoutError> {
    let extent = authored_extent.max(0.0);
    let Some(first) = constraints.first().copied() else {
        return Ok((fallback_begin, extent));
    };
    let mut best_pair = None;
    let mut best_separation = 0.0;
    for (left_index, left) in constraints.iter().enumerate() {
        for right in &constraints[left_index + 1..] {
            let separation = (right.factor - left.factor).abs();
            if separation > best_separation {
                best_pair = Some((*left, *right));
                best_separation = separation;
            }
        }
    }
    let (begin, extent) = if best_separation > AXIS_EPSILON {
        let Some((first, second)) = best_pair else {
            return Err(resolution_error(
                "separated live region constraints have no axis pair",
            ));
        };
        let inferred = (second.value - first.value) / (second.factor - first.factor);
        let begin = first.value - first.factor * inferred;
        if inferred < 0.0 {
            (begin + inferred, -inferred)
        } else {
            (begin, inferred)
        }
    } else {
        (first.value - first.factor * extent, extent)
    };
    if !begin.is_finite() || !extent.is_finite() {
        return Err(resolution_error(
            "live region constraints produced non-finite geometry",
        ));
    }
    Ok((begin, extent))
}

const fn point_x_factor(point: UiPoint) -> f64 {
    match point {
        UiPoint::Top | UiPoint::Center | UiPoint::Bottom => 0.5,
        UiPoint::TopRight | UiPoint::Right | UiPoint::BottomRight => 1.0,
        _ => 0.0,
    }
}

const fn point_y_factor(point: UiPoint) -> f64 {
    match point {
        UiPoint::Left | UiPoint::Center | UiPoint::Right => 0.5,
        UiPoint::TopLeft | UiPoint::Top | UiPoint::TopRight => 1.0,
        _ => 0.0,
    }
}

const fn constrains_x(point: UiPoint) -> bool {
    matches!(
        point,
        UiPoint::TopLeft
            | UiPoint::Left
            | UiPoint::BottomLeft
            | UiPoint::TopRight
            | UiPoint::Right
            | UiPoint::BottomRight
    )
}

const fn constrains_y(point: UiPoint) -> bool {
    matches!(
        point,
        UiPoint::TopLeft
            | UiPoint::Top
            | UiPoint::TopRight
            | UiPoint::BottomLeft
            | UiPoint::Bottom
            | UiPoint::BottomRight
    )
}

fn axis_coordinate(begin: f64, end: f64, factor: f64) -> f64 {
    begin + (end - begin) * factor
}

#[derive(Clone, Copy)]
struct Affine2 {
    xx: f64,
    xy: f64,
    yx: f64,
    yy: f64,
    tx: f64,
    ty: f64,
}

impl Affine2 {
    const IDENTITY: Self = Self {
        xx: 1.0,
        xy: 0.0,
        yx: 0.0,
        yy: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    fn scale_about(scale: f64, center_x: f64, center_y: f64) -> Self {
        Self {
            xx: scale,
            xy: 0.0,
            yx: 0.0,
            yy: scale,
            tx: center_x * (1.0 - scale),
            ty: center_y * (1.0 - scale),
        }
    }

    const fn translation(x: f64, y: f64) -> Self {
        Self {
            tx: x,
            ty: y,
            ..Self::IDENTITY
        }
    }

    fn compose(self, local: Self) -> Self {
        Self {
            xx: self.xx * local.xx + self.xy * local.yx,
            xy: self.xx * local.xy + self.xy * local.yy,
            yx: self.yx * local.xx + self.yy * local.yx,
            yy: self.yx * local.xy + self.yy * local.yy,
            tx: self.xx * local.tx + self.xy * local.ty + self.tx,
            ty: self.yx * local.tx + self.yy * local.ty + self.ty,
        }
    }

    fn transform(self, x: f64, y: f64) -> (f64, f64) {
        (
            self.xx * x + self.xy * y + self.tx,
            self.yx * x + self.yy * y + self.ty,
        )
    }

    fn bounds(self, rect: UiScreenRect) -> UiScreenRect {
        let corners = [
            self.transform(rect.left, rect.bottom),
            self.transform(rect.left, rect.top),
            self.transform(rect.right, rect.bottom),
            self.transform(rect.right, rect.top),
        ];
        UiScreenRect {
            left: corners
                .iter()
                .map(|corner| corner.0)
                .fold(f64::INFINITY, f64::min),
            bottom: corners
                .iter()
                .map(|corner| corner.1)
                .fold(f64::INFINITY, f64::min),
            right: corners
                .iter()
                .map(|corner| corner.0)
                .fold(f64::NEG_INFINITY, f64::max),
            top: corners
                .iter()
                .map(|corner| corner.1)
                .fold(f64::NEG_INFINITY, f64::max),
        }
    }
}

fn resolution_error(message: impl Into<String>) -> UiLayoutError {
    UiLayoutError::Resolution {
        message: message.into(),
    }
}
