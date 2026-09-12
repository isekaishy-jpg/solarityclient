//! Complete and retained-seed resolution of authored region constraints.

use super::{
    Affine2, UiRegionGeometry, UiRegionGeometryPlan, UiScreenRect, clamp_screen_rect,
    resolution_error,
};
use crate::script::{UiRuntimeAnchor, UiRuntimeObjectPlan};
use crate::{UiLayoutError, UiObjectRole, UiPoint};

const AXIS_EPSILON: f64 = 0.0001;
const MAX_REGION_ANCHORS: usize = 9;

impl UiRegionGeometryPlan {
    /// Validates and resolves a complete initial arena and its reverse links.
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
            retained: None,
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
            dependencies: super::RegionDependencies::new(live)?,
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

/// Public bounds and the inherited affine state needed by dependent regions.
#[derive(Clone, Copy)]
pub(super) struct ResolvedRegion {
    pub(super) public: UiRegionGeometry,
    pub(super) presentation: Affine2,
}

/// Depth-first resolution with either full storage or a borrowed retained seed.
pub(super) struct GeometryResolver<'plan> {
    pub(super) live: &'plan UiRuntimeObjectPlan,
    pub(super) screen: UiScreenRect,
    pub(super) retained: Option<(&'plan UiRegionGeometryPlan, &'plan [usize])>,
    pub(super) resolved: Vec<Option<ResolvedRegion>>,
    pub(super) visiting: Vec<bool>,
}

impl GeometryResolver<'_> {
    pub(super) fn resolve(&mut self, index: usize) -> Result<ResolvedRegion, UiLayoutError> {
        let slot = if let Some((retained, object_indices)) = self.retained {
            match object_indices.binary_search(&index) {
                Ok(slot) => slot,
                Err(_) => {
                    return retained
                        .regions
                        .get(index)
                        .zip(retained.presentations.get(index))
                        .map(|(&public, &presentation)| ResolvedRegion {
                            public,
                            presentation,
                        })
                        .ok_or_else(|| {
                            resolution_error(format!(
                                "live region index {index} is outside the arena"
                            ))
                        });
                }
            }
        } else {
            index
        };
        if let Some(region) = self.resolved.get(slot).copied().flatten() {
            return Ok(region);
        }
        let Some(visiting) = self.visiting.get_mut(slot) else {
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
        self.visiting[slot] = false;
        let region = result?;
        self.resolved[slot] = Some(region);
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
        let parent = parent_index
            .map(|parent| self.resolve(parent))
            .transpose()?;
        let effective_scale = scale * parent.map_or(1.0, |region| region.public.effective_scale);
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
            let parent_region = self.resolve(parent)?.public;
            let bounds = parent_region
                .logical_bounds
                .scaled(parent_region.effective_scale / effective_scale);
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
            let parent_region = self.resolve(parent_index)?.public;
            let parent_bounds = parent_region
                .logical_bounds
                .scaled(parent_region.effective_scale / effective_scale);
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
                Some(target) => {
                    let target = self.resolve(target)?.public;
                    target
                        .logical_bounds
                        .scaled(target.effective_scale / effective_scale)
                }
                None => self.screen.scaled(1.0 / effective_scale),
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

        let parent_bounds = parent.map(|region| {
            region
                .public
                .logical_bounds
                .scaled(region.public.effective_scale / effective_scale)
        });
        let fallback_left = parent_bounds.map_or(0.0, |bounds| bounds.left);
        // A stock ScrollChild without authored points begins at the scroll
        // frame's top-left. Its content height then extends downward and feeds
        // the native vertical range rather than moving the first line upward.
        let fallback_bottom = parent_bounds.map_or(0.0, |bounds| {
            if role == UiObjectRole::ScrollChild {
                bounds.top - authored.1.max(0.0)
            } else {
                bounds.bottom
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
        let mut logical_bounds = UiScreenRect {
            left: horizontal.0,
            bottom: vertical.0,
            right: horizontal.0 + horizontal.1,
            top: vertical.0 + vertical.1,
        };
        if let Some(insets) = object.clamp_insets {
            logical_bounds = clamp_screen_rect(
                logical_bounds,
                (
                    self.screen.right / effective_scale,
                    self.screen.top / effective_scale,
                ),
                insets,
            );
        }
        let scale_transform = Affine2::scale_about(scale, 0.0, 0.0);
        let local = Affine2::translation(object.animation_offset.0, object.animation_offset.1)
            .compose(scale_transform);
        let parent_transform = parent.map_or(Affine2::IDENTITY, |region| region.presentation);
        let presentation = parent_transform.compose(local);
        let presentation_bounds = presentation.bounds(logical_bounds);
        let effectively_shown =
            shown && parent.is_none_or(|region| region.public.effectively_shown);
        let effective_alpha = alpha * parent.map_or(1.0, |region| region.public.effective_alpha);
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
