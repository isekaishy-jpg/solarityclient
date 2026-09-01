//! Screen-space resolution for the live post-Lua region arena.

use crate::script::{UiRuntimeAnchor, UiRuntimeObjectPlan};
use crate::{UiLayoutError, UiObjectRole, UiPoint};

const AXIS_EPSILON: f64 = 0.0001;

/// One axis-aligned rectangle in the stock bottom-left UI coordinate system.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiScreenRect {
    left: f64,
    bottom: f64,
    right: f64,
    top: f64,
}

impl UiScreenRect {
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
}

/// Resolved startup geometry and inherited presentation state for one region.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiRegionGeometry {
    logical_bounds: UiScreenRect,
    presentation_bounds: UiScreenRect,
    effectively_shown: bool,
    effective_alpha: f64,
    effective_scale: f64,
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
}

/// Dense geometry parallel to the live Glue object arena.
pub struct UiRegionGeometryPlan {
    ui_extent: (f64, f64),
    regions: Vec<UiRegionGeometry>,
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
        let regions = resolver
            .resolved
            .into_iter()
            .enumerate()
            .map(|(index, region)| {
                region.map(|region| region.public).ok_or_else(|| {
                    resolution_error(format!("live region {index} remained unresolved"))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { ui_extent, regions })
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
        let alpha = object.alpha;
        let scale = object.scale;
        let role = object.role;
        let mut anchors = self.live.anchors_for(object).to_vec();
        if anchors.is_empty()
            && !matches!(role, UiObjectRole::Object | UiObjectRole::ScrollChild)
            && let Some(parent) = parent_index
        {
            anchors.push(UiRuntimeAnchor {
                point: UiPoint::Center,
                target: Some(parent),
                relative_point: UiPoint::Center,
                offset: (0.0, 0.0),
            });
        }

        let mut x_constraints = Vec::with_capacity(anchors.len());
        let mut y_constraints = Vec::with_capacity(anchors.len());
        let mut all_x = Vec::with_capacity(anchors.len());
        let mut all_y = Vec::with_capacity(anchors.len());
        for anchor in anchors {
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
            all_x.push(x);
            all_y.push(y);
            if constrains_x(anchor.point) {
                x_constraints.push(x);
            }
            if constrains_y(anchor.point) {
                y_constraints.push(y);
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
            if x_constraints.is_empty() {
                &all_x
            } else {
                &x_constraints
            },
            fallback_left,
        )?;
        let vertical = solve_axis(
            authored.1,
            if y_constraints.is_empty() {
                &all_y
            } else {
                &y_constraints
            },
            fallback_bottom,
        )?;
        let logical_bounds = UiScreenRect {
            left: horizontal.0,
            bottom: vertical.0,
            right: horizontal.0 + horizontal.1,
            top: vertical.0 + vertical.1,
        };
        let local = Affine2::scale_about(
            scale,
            (logical_bounds.left + logical_bounds.right) * 0.5,
            (logical_bounds.bottom + logical_bounds.top) * 0.5,
        );
        let parent_transform = parent.map_or(Affine2::IDENTITY, |region| region.presentation);
        let presentation = parent_transform.compose(local);
        let presentation_bounds = presentation.bounds(logical_bounds);
        let effectively_shown =
            shown && parent.is_none_or(|region| region.public.effectively_shown);
        let effective_alpha = alpha * parent.map_or(1.0, |region| region.public.effective_alpha);
        let effective_scale = scale * parent.map_or(1.0, |region| region.public.effective_scale);
        Ok(ResolvedRegion {
            public: UiRegionGeometry {
                logical_bounds,
                presentation_bounds,
                effectively_shown,
                effective_alpha,
                effective_scale,
            },
            presentation,
        })
    }
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
