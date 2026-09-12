//! Stock sun/moon glare inputs, independent of the celestial disc passes.

mod state;
pub(crate) use state::GlareState;
mod lighting;
pub use lighting::WorldGlareLighting;

use crate::{BlpTextureHandle, WorldCameraFrame, WorldCelestialBody};

/// The two native glare owners; the second moon has no glare object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldGlareKind {
    /// 7EE150's daytime glare.
    Sun,
    /// 7EE230's first-moon glare.
    Moon,
}

/// Frame-local inputs to 7EF6E0, before deferred GPU visibility is applied.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldGlareEnvironment {
    /// Normalized realm day, in 0..=1.
    pub day: f32,
    /// Elapsed world time in seconds since the previous update.
    pub elapsed_seconds: f32,
    /// Native procedural-cloud alpha sampled toward the sun and first moon.
    pub cloud_alpha: [f32; 2],
    /// Camera depth below an admitted liquid surface; absence means dry.
    pub liquid_depth: Option<f32>,
    /// Selected global sky weight, or maximum admitted local sky weight.
    pub skybox_weight: f32,
}

/// Camera and authored resources retained through one world submission.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldGlareFrame {
    /// Final world camera used by both occlusion probes and visible billboards.
    pub camera: WorldCameraFrame,
    /// Sun and first moon, including the disc size used by the occlusion probe.
    pub bodies: [WorldCelestialBody; 2],
    /// Linear sunGlare/moonGlare BLP handles.
    pub textures: [BlpTextureHandle; 2],
    /// Fresh packed ARGB colors from the celestial palette/weather update.
    pub colors: [u32; 2],
    /// Non-GPU attenuation and clock inputs.
    pub environment: WorldGlareEnvironment,
}
