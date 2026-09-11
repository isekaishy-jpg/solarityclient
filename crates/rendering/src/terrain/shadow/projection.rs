//! Primary 7BBC50 caster view and 7BAC10/8750B0 direct-depth receiver transform.

use glam::{Mat4, Vec3, Vec4};
use thiserror::Error;

use super::WorldEnvironmentShadowMap;
use super::quality::WorldShadowQuality;

/// Invalid inputs to the original nondegenerate shadow-camera contract.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum WorldShadowProjectionError {
    /// The disabled quality has no shadow texture or projection.
    #[error("disabled shadow quality has no projection")]
    Disabled,
    /// The selected quality allocates only the primary unit map.
    #[error("shadow quality has no environment maps")]
    NoEnvironmentMaps,
    /// Partial-map bounds must form a finite, nonempty rectangle.
    #[error("shadow update has invalid crop bounds")]
    InvalidCrop,
    /// World positions and day/night direction must be finite.
    #[error("shadow projection inputs must be finite")]
    NonFinite,
    /// Original 6C0050 requires nonzero forward and perpendicular up vectors.
    #[error("shadow light direction produces a degenerate view")]
    DegenerateDirection,
}

/// One camera-relative primary map with stock's separate caster/receiver centers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldShadowProjection {
    texture_size: u32,
    radius: f32,
    origin: Vec3,
    light_direction: Vec3,
    receiver_center: Vec3,
    caster_view: Mat4,
    caster_projection: Mat4,
    receiver_rows: [Vec4; 3],
    caster_bounds: [Vec3; 2],
    caster_planes: [Vec4; 6],
}

impl WorldShadowProjection {
    /// Restricts the caster projection and admission volume to one cached region.
    /// Receiver rows retain the complete map's extent, as in original 874890.
    ///
    /// # Errors
    /// Rejects nonfinite or empty light-space regions.
    pub fn with_caster_region(
        mut self,
        crop: [f32; 4],
    ) -> Result<Self, WorldShadowProjectionError> {
        if !crop.iter().all(|value| value.is_finite()) || crop[0] >= crop[1] || crop[2] >= crop[3] {
            return Err(WorldShadowProjectionError::InvalidCrop);
        }
        self.caster_projection =
            Mat4::orthographic_lh(crop[0], crop[1], crop[2], crop[3], 1., 4000.);
        (self.caster_bounds, self.caster_planes) = caster_volume(
            self.caster_view,
            self.origin,
            [crop[0], crop[1]],
            [crop[2], crop[3]],
        );
        Ok(self)
    }

    /// Applies 7BAFD0's default camera-footprint crop to root-caster admission.
    ///
    /// The rendered map and receiver transform retain their full extent. A
    /// disjoint or empty footprint retains stock's original uncropped volume.
    #[must_use]
    pub fn with_camera_culling(mut self, camera: crate::WorldCameraFrame) -> Self {
        let inverse = camera.view().inverse() * camera.projection().inverse();
        let mut minimum = Vec3::splat(f32::INFINITY);
        let mut maximum = Vec3::splat(f32::NEG_INFINITY);
        for x in [-1., 1.] {
            for y in [-1., 1.] {
                for z in [0., 1.] {
                    let world = inverse.project_point3(Vec3::new(x, y, z));
                    let light = self
                        .caster_projection
                        .project_point3(self.caster_view.transform_point3(world - self.origin));
                    minimum = minimum.min(light);
                    maximum = maximum.max(light);
                }
            }
        }
        // Original light depth is [-1,1]; our Vulkan depth is [0,1]. The
        // intersection gate is equivalent and does not crop the depth planes.
        minimum = minimum.clamp(Vec3::new(-1., -1., 0.), Vec3::ONE);
        maximum = maximum.clamp(Vec3::new(-1., -1., 0.), Vec3::ONE);
        if minimum.cmplt(maximum).all() {
            (self.caster_bounds, self.caster_planes) = caster_volume(
                self.caster_view,
                self.origin,
                [minimum.x * self.radius, maximum.x * self.radius],
                [minimum.y * self.radius, maximum.y * self.radius],
            );
        }
        self
    }
    /// Original 874760 uploads camera-space rows, direction, and a zero primary fade plane.
    // The private size is established by primary(), which rejects Disabled;
    // this assertion guards an internal invariant rather than caller input.
    #[allow(clippy::expect_used)]
    pub(crate) fn m2_state(
        self,
        view: Mat4,
        camera_position: Vec3,
        environment: Option<crate::WorldEnvironmentShadowFrame<'_>>,
    ) -> crate::M2ShadowState {
        // Diffuse_T1 VS31 applies c224..226 to its model/view result. Convert
        // our camera-relative world rows without cancelling large translations.
        let inverse_rotation = glam::Mat3::from_mat4(view).inverse();
        let eye_to_relative_world = Mat4::from_cols(
            inverse_rotation.x_axis.extend(0.),
            inverse_rotation.y_axis.extend(0.),
            inverse_rotation.z_axis.extend(0.),
            (camera_position - self.origin).extend(1.),
        );
        let rows = self
            .receiver_rows
            .map(|row| eye_to_relative_world.transpose() * row);
        let mut matrices = [crate::M2ShadowMatrix::disabled(); 4];
        matrices[0] = crate::M2ShadowMatrix::new(rows[0], rows[1], rows[2]);
        if let Some(environment) = environment {
            for (index, projection) in environment.receivers().into_iter().enumerate() {
                let rows = projection
                    .receiver_rows()
                    .map(|row| eye_to_relative_world.transpose() * row);
                matrices[index + 1] = crate::M2ShadowMatrix::new(rows[0], rows[1], rows[2]);
            }
        }
        crate::M2ShadowState::new(
            matrices,
            Vec4::ZERO,
            view.transform_vector3(self.light_direction).normalize(),
            // Construction rejects a disabled map, so the divisor is nonzero.
            crate::M2ShadowState::stock_filter_offsets(self.texture_size)
                .expect("enabled primary shadow projection has a nonzero texture size"),
        )
        .with_world_mode(environment.map_or(1, |frame| frame.quality().shader_mode()))
    }
    /// Creates the primary map shared by all enabled quality levels.
    ///
    /// `day_night_direction` is the original ray direction, pointing away from
    /// the light. Both shader stages must subtract `origin` from world positions
    /// before applying these matrices. Higher qualities need their additional
    /// environment maps as well as this primary projection.
    ///
    /// # Errors
    ///
    /// Rejects disabled quality, nonfinite inputs, and degenerate light views.
    pub fn primary(
        quality: WorldShadowQuality,
        center: Vec3,
        origin: Vec3,
        day_night_direction: Vec3,
    ) -> Result<Self, WorldShadowProjectionError> {
        Self::build(quality, center, origin, day_night_direction, None)
    }

    /// Constructs an environment map using its published or pending center.
    ///
    /// The persistent refresh owner supplies the center; this constructor does
    /// not snap it again or advance the cache. Original 7BAC10 gives each extent
    /// a separate depth bias while keeping the primary light-view convention.
    ///
    /// # Errors
    /// Rejects qualities without environment maps and invalid light views.
    pub fn environment(
        quality: WorldShadowQuality,
        map: WorldEnvironmentShadowMap,
        center: Vec3,
        origin: Vec3,
        day_night_direction: Vec3,
    ) -> Result<Self, WorldShadowProjectionError> {
        if quality.shader_mode() < 2 {
            return Err(WorldShadowProjectionError::NoEnvironmentMaps);
        }
        Self::build(quality, center, origin, day_night_direction, Some(map))
    }

    fn build(
        quality: WorldShadowQuality,
        center: Vec3,
        origin: Vec3,
        day_night_direction: Vec3,
        environment: Option<WorldEnvironmentShadowMap>,
    ) -> Result<Self, WorldShadowProjectionError> {
        let texture_size = quality
            .texture_size()
            .ok_or(WorldShadowProjectionError::Disabled)?;
        if !center.is_finite() || !origin.is_finite() || !day_night_direction.is_finite() {
            return Err(WorldShadowProjectionError::NonFinite);
        }
        // 7BB570 scales vertical light by five, clamps at -1.2, then normalizes.
        let direction = Vec3::new(
            day_night_direction.x,
            day_night_direction.y,
            (day_night_direction.z * 5.0).max(-1.2),
        );
        let light_direction = normalize(direction)?;
        let (radius, bias, receiver_center) = environment.map_or_else(
            || {
                (
                    20.,
                    0.1,
                    Vec3::new(
                        quantize(center.x, texture_size),
                        quantize(center.y, texture_size),
                        center.z,
                    ),
                )
            },
            |map| (map.radius(), map.depth_bias(), center),
        );
        // 875F80 retains the unsnapped scene center for 7BBC50's caster camera.
        let caster_view = light_view(center, origin, light_direction)?;
        let receiver_view = light_view(receiver_center, origin, light_direction)?;
        let caster_projection =
            Mat4::orthographic_lh(-radius, radius, -radius, radius, 1.0, 4000.0);
        let mut x = receiver_view.row(0) * radius.recip();
        let mut y = receiver_view.row(1) * -radius.recip();
        let mut depth = receiver_view.row(2);
        // Direct-depth backend: primary bias is 0.5 * 0.2, then depth / 4000.
        depth.w -= bias;
        depth *= 0.00025;
        let half_texel = 0.5 / texture_size as f32;
        x.w += half_texel;
        y.w += half_texel;
        let (caster_bounds, caster_planes) =
            caster_volume(caster_view, origin, [-radius, radius], [-radius, radius]);
        Ok(Self {
            texture_size,
            radius,
            origin,
            light_direction,
            receiver_center,
            caster_view,
            caster_projection,
            receiver_rows: [x, y, depth],
            caster_bounds,
            caster_planes,
        })
    }

    /// Applies 7BB9D0's primary root-unit radius, world-box, and six-plane tests.
    ///
    /// Bounds and radius must describe the registered root model in world space.
    /// Attached children inherit root admission and bypass this spatial test.
    #[must_use]
    pub fn admits_unit(self, minimum: Vec3, maximum: Vec3, radius: f32) -> bool {
        (0.25..=10_000.0).contains(&radius) && self.admits_bounds(minimum, maximum)
    }

    /// Applies the world-box and six-plane tests shared by scenery collectors.
    /// Per-owner flags, distance limits, and group admission remain with callers.
    #[must_use]
    pub fn admits_bounds(self, minimum: Vec3, maximum: Vec3) -> bool {
        if !minimum.is_finite()
            || !maximum.is_finite()
            || !minimum.cmple(maximum).all()
            || !minimum.cmple(self.caster_bounds[1]).all()
            || !maximum.cmpge(self.caster_bounds[0]).all()
        {
            return false;
        }
        // 9839E0 selects each plane's positive AABB vertex, accepting the
        // original AA2E74 negative tolerance at the volume boundary.
        self.caster_planes.iter().all(|plane| {
            let normal = plane.truncate();
            let vertex = Vec3::select(normal.cmpge(Vec3::ZERO), maximum, minimum);
            normal.dot(vertex) + plane.w >= -0.019_444_443
        })
    }

    /// Returns the square map extent selected by quality.
    pub const fn texture_size(self) -> u32 {
        self.texture_size
    }

    /// Returns the world origin subtracted before either transform.
    pub const fn origin(self) -> Vec3 {
        self.origin
    }

    /// Returns the adjusted normalized ray used to position the light camera.
    pub const fn light_direction(self) -> Vec3 {
        self.light_direction
    }

    /// Returns the quantized receiver center retained by original 875F80.
    pub const fn receiver_center(self) -> Vec3 {
        self.receiver_center
    }

    /// Returns the light view for positions relative to `origin`.
    pub const fn caster_view(self) -> Mat4 {
        self.caster_view
    }

    /// Returns the caster projection with Vulkan's zero-through-one depth range.
    pub const fn caster_projection(self) -> Mat4 {
        self.caster_projection
    }

    /// Returns clip-square x/y and direct-comparison depth rows.
    pub const fn receiver_rows(self) -> [Vec4; 3] {
        self.receiver_rows
    }
}

/// Stores the original 7BAFD0 orthographic root-caster volume once per frame.
fn caster_volume(
    view: Mat4,
    origin: Vec3,
    horizontal: [f32; 2],
    vertical: [f32; 2],
) -> ([Vec3; 2], [Vec4; 6]) {
    let inverse = view.inverse();
    let mut minimum = Vec3::splat(f32::INFINITY);
    let mut maximum = Vec3::splat(f32::NEG_INFINITY);
    for x in horizontal {
        for y in vertical {
            for z in [1., 4000.] {
                let corner = inverse.transform_point3(Vec3::new(x, y, z)) + origin;
                minimum = minimum.min(corner);
                maximum = maximum.max(corner);
            }
        }
    }
    let row = |index| {
        let mut row = view.row(index);
        row.w -= row.truncate().dot(origin);
        row
    };
    let planes = [
        row(0) - Vec4::W * horizontal[0],
        -row(0) + Vec4::W * horizontal[1],
        row(1) - Vec4::W * vertical[0],
        -row(1) + Vec4::W * vertical[1],
        row(2) - Vec4::W,
        -row(2) + Vec4::W * 4000.,
    ];
    ([minimum, maximum], planes)
}

/// Reproduces the extended intermediate used before 875F80's floor/store pair.
fn quantize(value: f32, texture_size: u32) -> f32 {
    ((f64::from(value) * f64::from(texture_size) + 0.5).floor() / f64::from(texture_size)) as f32
}

/// Preserves the original float stores around x87 normalization at 4C3420.
fn normalize(value: Vec3) -> Result<Vec3, WorldShadowProjectionError> {
    let length = value.as_dvec3().length();
    if length <= 0.0 || !length.is_finite() {
        return Err(WorldShadowProjectionError::DegenerateDirection);
    }
    Ok((value.as_dvec3() / length).as_vec3())
}

/// Reproduces 6C0050's left-handed light view with the fixed world-X up vector.
fn light_view(
    center: Vec3,
    origin: Vec3,
    direction: Vec3,
) -> Result<Mat4, WorldShadowProjectionError> {
    let eye = (center.as_dvec3() - direction.as_dvec3() * 2000.0 - origin.as_dvec3()).as_vec3();
    let target = center - origin;
    let forward = normalize(target - eye)?;
    let right = normalize(Vec3::X.cross(forward))?;
    let up = normalize(forward.cross(right))?;
    let translation = Vec3::new(
        -right.as_dvec3().dot(eye.as_dvec3()) as f32,
        -up.as_dvec3().dot(eye.as_dvec3()) as f32,
        -forward.as_dvec3().dot(eye.as_dvec3()) as f32,
    );
    Ok(Mat4::from_cols(
        Vec4::new(right.x, up.x, forward.x, 0.0),
        Vec4::new(right.y, up.y, forward.y, 0.0),
        Vec4::new(right.z, up.z, forward.z, 0.0),
        translation.extend(1.0),
    ))
}
