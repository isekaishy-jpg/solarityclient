//! Build-12340 model-bound classification against the owner's liquid plane.

use glam::{Mat4, Vec3, Vec4};

use super::M2TransparentPass;

/// Liquid state supplied by a model owner's spatial callback.
///
/// Attachments inherit this state before classifying their own model bounds.
/// Terrain scenery has a different callback and retains its default state.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum M2LiquidState {
    /// No liquid was found at the owner's registered probe.
    #[default]
    Above,
    /// The surface lies above the owner's complete world render bounds.
    Below,
    /// Potential intersection, retaining the normalized view-space plane.
    Surface(Vec4),
}

impl M2LiquidState {
    /// Transforms the horizontal world plane as build 12340 `0x008350A0` does.
    #[must_use]
    pub fn at_world_height(height: f32, view: Mat4) -> Self {
        let mut normal = view.transform_vector3(Vec3::Z);
        let length_squared = normal.length_squared();
        if length_squared > 2.384_185_8e-7 {
            normal /= length_squared.sqrt();
        }
        let point = view.transform_point3(Vec3::new(0.0, 0.0, height));
        Self::Surface(normal.extend(-normal.dot(point)))
    }

    /// Replays `0x00821C8C..0x00821DDA`, including equality at both tangencies.
    ///
    /// The first model-view column scales the authored radius. The native
    /// non-clipping fallback selects the camera's side only when both survive.
    #[must_use]
    pub fn classify_model(
        self,
        center: Vec3,
        radius: f32,
        model_view: Mat4,
        clipping_enabled: bool,
        camera_below: bool,
    ) -> M2LiquidPasses {
        let Self::Surface(plane) = self else {
            return M2LiquidPasses {
                above: self == Self::Above,
                below: self == Self::Below,
                plane: Vec4::ZERO,
            };
        };
        let center = model_view.transform_point3(center);
        let radius = radius * model_view.x_axis.truncate().length();
        // The x87 plane distance stays extended through both comparisons;
        // rounding it to f32 moves immediately adjacent tangencies into water.
        let distance = plane.as_dvec4().dot(center.as_dvec3().extend(1.0));
        M2LiquidPasses {
            above: distance >= -f64::from(radius),
            below: distance <= f64::from(radius),
            plane,
        }
        .with_clipping_support(clipping_enabled, camera_below)
    }
}

/// Model-specific queues after the inherited owner plane meets authored bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2LiquidPasses {
    above: bool,
    below: bool,
    plane: Vec4,
}

impl M2LiquidPasses {
    /// Applies the native camera-side fallback to meshes and ribbons. Particle
    /// routing consumes the original classification without this fallback.
    #[must_use]
    pub const fn with_clipping_support(mut self, enabled: bool, camera_below: bool) -> Self {
        if !enabled && self.above && self.below {
            self.above = !camera_below;
            self.below = camera_below;
        }
        self
    }

    /// Whether ordinary translucent meshes enter pass one.
    #[must_use]
    pub const fn above(self) -> bool {
        self.above
    }

    /// Whether ordinary translucent meshes enter pass two.
    #[must_use]
    pub const fn below(self) -> bool {
        self.below
    }

    /// View-space clip plane for a mesh crossing the surface; nonnegative
    /// distances survive. `0x0081FB10` negates all four terms for pass two.
    #[must_use]
    pub fn clip_plane(self, pass: M2TransparentPass) -> Option<Vec4> {
        (self.above && self.below).then_some(match pass {
            M2TransparentPass::One => self.plane,
            M2TransparentPass::Two => -self.plane,
        })
    }

    /// Ribbons choose one queue even when their owning model crosses water.
    #[must_use]
    pub const fn ribbon_pass(self) -> M2TransparentPass {
        if self.above {
            M2TransparentPass::One
        } else {
            M2TransparentPass::Two
        }
    }
}
