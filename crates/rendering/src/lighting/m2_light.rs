//! Stock implementation responsibility recovered from `M2Light.cpp`.

use glam::Vec3;

use crate::{M2LocalLightState, WorldCamera};

const DIRECTION_NORMALIZATION_EPSILON: f32 = 0.000_000_238_418_58;
const SUN_DIRECTION_EPSILON_SQUARED: f32 = 0.000_01;
const RED_LUMINANCE: f32 = 0.212_671;
const GREEN_LUMINANCE: f32 = 0.715_16;
const BLUE_LUMINANCE: f32 = 0.072_169;
const GLUE_CHARACTER_AMBIENT: Vec3 = Vec3::splat(0.60);
const GLUE_CHARACTER_DIFFUSE: Vec3 = Vec3::new(0.40, 0.40, 0.32);
const M2_POINT_LIGHT_ATTENUATION: Vec3 = Vec3::new(0.0, 0.7, 0.03);

/// One visible M2 point light transformed into scene coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2PointLight {
    position: Vec3,
    ambient: Vec3,
    diffuse: Vec3,
}

impl M2PointLight {
    /// Retains one authored point source after bone and placement transforms.
    #[must_use]
    pub const fn new(position: Vec3, ambient: Vec3, diffuse: Vec3) -> Self {
        Self {
            position,
            ambient,
            diffuse,
        }
    }

    /// Returns the point source in scene coordinates.
    #[must_use]
    pub const fn position(self) -> Vec3 {
        self.position
    }

    /// Returns this source's animated ambient contribution.
    #[must_use]
    pub const fn ambient(self) -> Vec3 {
        self.ambient
    }

    /// Returns this source's animated diffuse contribution.
    #[must_use]
    pub const fn diffuse(self) -> Vec3 {
        self.diffuse
    }

    /// Converts the source to build 12340's fixed-function attenuation.
    #[must_use]
    pub fn local_light_state(self) -> M2LocalLightState {
        M2LocalLightState::positional(
            self.position,
            self.ambient.clamp(Vec3::ZERO, Vec3::splat(16.0)),
            self.diffuse.clamp(Vec3::ZERO, Vec3::splat(16.0)),
            M2_POINT_LIGHT_ATTENUATION,
        )
    }
}

/// One directional source accumulated by build 12340's M2 sunlight merger.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2DirectionalLight {
    direction: Vec3,
    ambient: Vec3,
    diffuse: Vec3,
}

impl M2DirectionalLight {
    /// Retains the authored D3D ray direction and its final RGB contributions.
    #[must_use]
    pub const fn new(direction: Vec3, ambient: Vec3, diffuse: Vec3) -> Self {
        Self {
            direction,
            ambient,
            diffuse,
        }
    }

    /// Returns the transformed D3D ray direction before sunlight merging.
    #[must_use]
    pub const fn direction(self) -> Vec3 {
        self.direction
    }

    /// Returns this source's animated ambient contribution.
    #[must_use]
    pub const fn ambient(self) -> Vec3 {
        self.ambient
    }

    /// Returns this source's animated diffuse contribution.
    #[must_use]
    pub const fn diffuse(self) -> Vec3 {
        self.diffuse
    }
}

/// One finalized directional slot in the renderer's surface-to-light form.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2Sunlight {
    direction: Vec3,
    ambient: Vec3,
    diffuse: Vec3,
}

impl M2Sunlight {
    /// Returns the normalized vector from a rendered surface toward the light.
    #[must_use]
    pub const fn direction(self) -> Vec3 {
        self.direction
    }

    /// Returns the clamped ambient RGB accumulated from every source.
    #[must_use]
    pub const fn ambient(self) -> Vec3 {
        self.ambient
    }

    /// Returns the clamped RGB projected onto the merged sun direction.
    #[must_use]
    pub const fn diffuse(self) -> Vec3 {
        self.diffuse
    }

    /// Converts the finalized sun to the uniform representation used by M2s.
    #[must_use]
    pub fn local_light_state(self) -> M2LocalLightState {
        M2LocalLightState::directional(self.direction, self.ambient, self.diffuse)
    }
}

/// Replays `CM2Lighting::SetupSunlight` for WotLK directional sources.
///
/// Build 12340 combines color-weighted source directions into one hardware
/// sunlight slot. Authored vectors point along D3D light rays, so the final
/// vector is inverted once into the renderer's surface-to-light convention.
#[must_use]
pub fn merge_wotlk_directional_lights(lights: &[M2DirectionalLight]) -> Option<M2Sunlight> {
    if lights.is_empty() {
        return None;
    }

    let mut ambient = Vec3::ZERO;
    let mut red_vector = Vec3::ZERO;
    let mut green_vector = Vec3::ZERO;
    let mut blue_vector = Vec3::ZERO;
    let mut luminance_vector = Vec3::ZERO;
    let mut diffuse_sum = Vec3::ZERO;
    for light in lights {
        let mut direction = light.direction;
        let magnitude = direction.length();
        if magnitude > DIRECTION_NORMALIZATION_EPSILON {
            direction /= magnitude;
        }
        ambient += light.ambient;
        red_vector += direction * light.diffuse.x;
        green_vector += direction * light.diffuse.y;
        blue_vector += direction * light.diffuse.z;
        let luminance = light.diffuse.x * RED_LUMINANCE
            + light.diffuse.y * GREEN_LUMINANCE
            + light.diffuse.z * BLUE_LUMINANCE;
        luminance_vector += direction * luminance;
        diffuse_sum += light.diffuse;
    }

    let mut sun_direction = Vec3::new(0.0, 0.0, -1.0);
    if luminance_vector.length_squared() > SUN_DIRECTION_EPSILON_SQUARED {
        sun_direction = luminance_vector.normalize();
    }
    let projection = Vec3::new(
        sun_direction.dot(red_vector),
        sun_direction.dot(green_vector),
        sun_direction.dot(blue_vector),
    );
    // SetupSunlight caps diffuse at one; the fixed-function upload then clamps
    // every color channel to the hardware range. Apply both boundaries here
    // because this value goes directly into Solarity's shader descriptor.
    let diffuse = (projection * 1.25 - diffuse_sum * 0.25)
        .min(Vec3::ONE)
        .max(Vec3::ZERO);
    let ambient = (ambient + (diffuse_sum - projection) * 0.25)
        .min(Vec3::splat(16.0))
        .max(Vec3::ZERO);
    Some(M2Sunlight {
        direction: -sun_direction,
        ambient,
        diffuse,
    })
}

/// Produces stock's warm camera-relative character light when Glue omits one.
#[must_use]
pub fn glue_character_sunlight(camera: WorldCamera) -> M2Sunlight {
    let mut direction = camera.position() - camera.target();
    if direction.length() > 0.000_01 {
        direction = direction.normalize();
        direction = (direction + Vec3::new(0.0, 0.0, 0.35)).normalize();
    }
    M2Sunlight {
        direction,
        ambient: GLUE_CHARACTER_AMBIENT,
        diffuse: GLUE_CHARACTER_DIFFUSE,
    }
}
