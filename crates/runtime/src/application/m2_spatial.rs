//! Build-12340 scenery size classes and camera-distance opacity.

use glam::{Mat4, Vec3};

/// CMapObj 7BDB10 classifies the largest transformed render-box dimension.
/// The center is the transformed render-box midpoint (4F5E80), not the origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SceneryDistance {
    center: Vec3,
    category: usize,
}

impl SceneryDistance {
    const FAR: [f32; 5] = [30., 100., 200., 750., 1250.];
    const FADE: [f32; 5] = [5., 10., 15., 20., 50.];

    pub(super) const fn center(self) -> Vec3 {
        self.center
    }

    pub(super) const fn category(self) -> usize {
        self.category
    }

    fn ordinary_far(category: usize, detail: f32) -> f32 {
        let far = Self::FAR[category];
        if (1..=3).contains(&category) {
            far * detail
        } else {
            far
        }
    }

    fn shadow_square(category: usize, detail: f32) -> f32 {
        let far = f64::from(Self::FAR[category]);
        let far = if (1..=3).contains(&category) {
            far * f64::from(detail)
        } else {
            far
        };
        let start = far - f64::from(Self::FADE[category]);
        (start * start) as f32
    }

    /// Conservative broad-query extents for the two existing distance policies.
    /// The final decisions still use their original f32/x87-compatible math.
    /// Padding covers f32 subtraction/square rounding at the ordinary boundary;
    /// it expands only the candidate query, never actual visibility or shadows.
    pub(super) fn frame_query_radii(detail: f32) -> [f64; 5] {
        std::array::from_fn(|category| {
            let far = Self::ordinary_far(category, detail);
            if !detail.is_finite() || !far.is_finite() {
                return f64::INFINITY;
            }
            let square = (far * far).max(Self::shadow_square(category, detail));
            f64::from(square).sqrt() * (1.0 + 32.0 * f64::from(f32::EPSILON)) + 1.0e-6
        })
    }

    /// 7BABC0 and the terrain shadow collector use ADF3DC: the square of the
    /// fade-start radius, not the farther ordinary visibility cutoff. 78F570
    /// retains the scaled radius and subtraction in x87 until the square store.
    pub(super) fn admits_shadow(self, camera: Vec3, detail: f32) -> bool {
        let square = Self::shadow_square(self.category, detail);
        self.center.as_dvec3().distance_squared(camera.as_dvec3()) <= f64::from(square)
    }

    /// 78FB60 compares the stored group depth against native far squares.
    /// 78F570 retains the scaled radius in x87 when forming each square.
    pub(super) fn admits_group(self, depth: f32, detail: f32) -> bool {
        self.category >= minimum_category(depth, detail)
    }
    /// 7BDB10 uses the placement origin when every render-box axis is reversed.
    /// Otherwise 7F9430 orders each product, even for partially reversed boxes,
    /// retaining x87 precision until each axis contribution is stored as float.
    pub(super) fn world_bounds(minimum: Vec3, maximum: Vec3, transform: Mat4) -> (Vec3, Vec3) {
        let mut world_minimum = transform.w_axis.truncate();
        let mut world_maximum = world_minimum;
        if minimum.cmple(maximum).any() {
            for axis in 0..3 {
                let basis = transform.col(axis).truncate().as_dvec3();
                let first = basis * f64::from(minimum[axis]);
                let second = basis * f64::from(maximum[axis]);
                world_minimum = (world_minimum.as_dvec3() + first.min(second)).as_vec3();
                world_maximum = (world_maximum.as_dvec3() + first.max(second)).as_vec3();
            }
        }
        (world_minimum, world_maximum)
    }

    /// Replays 7BDB10's affine bounds and 7BDD31's inclusive class thresholds.
    pub(super) fn new(minimum: Vec3, maximum: Vec3, transform: Mat4) -> Self {
        let (world_minimum, world_maximum) = Self::world_bounds(minimum, maximum, transform);
        let size = (world_maximum - world_minimum).max_element();
        let category = [1.0, 4.0, 15.0, 100.0]
            .iter()
            .position(|limit| size <= *limit)
            .unwrap_or(4);
        Self {
            center: transform.transform_point3((minimum + maximum) * 0.5),
            category,
        }
    }

    /// 78F570 scales only classes 1..=3 by environmentDetail; 791CB0
    /// rejects beyond the far radius and snaps alpha at 0.01 and 0.99.
    pub(super) fn opacity(self, camera: Vec3, environment_detail: f32) -> f32 {
        let distance_squared = self.center.distance_squared(camera);
        let distance = Self::ordinary_far(self.category, environment_detail);
        if distance_squared > distance * distance {
            return 0.0;
        }
        let fade = Self::FADE[self.category];
        let start = distance - fade;
        if distance_squared <= start * start {
            return 1.0;
        }
        let opacity = 1.0 - (distance_squared.sqrt() - start) / fade;
        if opacity > 0.99 {
            1.0
        } else if opacity <= 0.01 {
            0.0
        } else {
            opacity
        }
    }
}

/// 78FB60 selects the first stored far square strictly beyond the group depth.
fn minimum_category(depth: f32, detail: f32) -> usize {
    if depth <= 0.0 {
        return 0;
    }
    let square = f64::from(depth) * f64::from(depth);
    [30.0, 100.0, 200.0, 750.0]
        .into_iter()
        .enumerate()
        .position(|(index, radius)| {
            let radius = if index == 0 {
                radius
            } else {
                radius * f64::from(detail)
            };
            square < f64::from((radius * radius) as f32)
        })
        .unwrap_or(4)
}

/// Immutable CPU culling data for a terrain-owned placement generation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct StaticM2Spatial {
    sphere: (Vec3, f32),
    scenery: SceneryDistance,
    world_bounds: (Vec3, Vec3),
}

impl StaticM2Spatial {
    /// Matches the existing model-sphere and native scenery classification math.
    pub(super) fn new(minimum: Vec3, maximum: Vec3, radius: f32, transform: Mat4) -> Self {
        let center = transform.transform_point3((minimum + maximum) * 0.5);
        let maximum_scale = transform.x_axis.truncate().length().max(
            transform
                .y_axis
                .truncate()
                .length()
                .max(transform.z_axis.truncate().length()),
        );
        Self {
            sphere: (center, radius * maximum_scale),
            scenery: SceneryDistance::new(minimum, maximum, transform),
            world_bounds: SceneryDistance::world_bounds(minimum, maximum, transform),
        }
    }

    pub(super) const fn sphere(self) -> (Vec3, f32) {
        self.sphere
    }

    pub(super) const fn scenery(self) -> SceneryDistance {
        self.scenery
    }

    /// Native transformed render bounds are immutable for this scenery lifetime.
    /// Validation stays at the consuming collector's original admission boundary.
    pub(super) const fn world_bounds(self) -> (Vec3, Vec3) {
        self.world_bounds
    }
}
