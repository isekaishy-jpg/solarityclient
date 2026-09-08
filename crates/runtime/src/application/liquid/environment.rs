//! World-light channels consumed by the three stock procedural water images.

use glam::Vec3;
use solarity_asset::WorldLightSample;
use solarity_rendering::{
    LiquidDepthTexture, LiquidDepthTextureKind, LiquidFog, LiquidLighting, WorldCameraFrame,
};

use crate::application::environment_coordinator::RuntimeWorldEnvironmentFrame;

/// Generates the native image families at one coherent exterior-light sample.
pub(in crate::application) fn liquid_depth_images(
    light: WorldLightSample,
) -> [LiquidDepthTexture; 3] {
    // 7ECD80 stores color bands 0..17 at environment +D4. 8A2BF0 reads
    // +10C/+110 for river and +114/+118 for ocean: bands 14/15 and 16/17.
    let colors = light.liquid_colors().map(pack_color);
    let alphas = light
        .liquid_alphas()
        .map(|alpha| (alpha * 255.0).round_ties_even() as u8);
    [
        LiquidDepthTexture::prepare(
            LiquidDepthTextureKind::River,
            [colors[0], colors[1]],
            [alphas[2], alphas[3]],
        ),
        LiquidDepthTexture::prepare(
            LiquidDepthTextureKind::Ocean,
            [colors[2], colors[3]],
            [alphas[0], alphas[1]],
        ),
        LiquidDepthTexture::prepare(
            LiquidDepthTextureKind::WorldModel,
            [colors[2], colors[3]],
            [alphas[0], alphas[1]],
        ),
    ]
}

/// Captures directional water lighting and fog in the final camera's view space.
pub(in crate::application) fn liquid_environment(
    environment: RuntimeWorldEnvironmentFrame,
    camera: WorldCameraFrame,
) -> (LiquidLighting, LiquidFog) {
    let light = environment.light();
    // 7EEA90 -> 834AE0 -> 834F60 retains the native light-ray direction for
    // 8A38B0. The shared environment uses surface-to-light for terrain/M2;
    // liquid's original shader performs its own negation before N.L.
    let lighting = LiquidLighting::new(
        camera
            .view()
            .transform_vector3(-environment.light_direction()),
        light.ambient_color(),
        light.diffuse_color(),
        light.specular_color(),
    );
    let fog = environment.fog();
    let (near, far) = fog.range();
    // 8A38B0 supplies (far - depth)/(far - near). Our RH view has negative
    // forward Z, so its coefficient is positive, matching the shader's view Z.
    let inverse = 1.0 / (far - near);
    (
        lighting,
        LiquidFog::new(
            Vec3::new(inverse, far * inverse, fog.exponent()),
            fog.color(),
        ),
    )
}

/// Preserves the normalized RGB-to-packed-byte boundary used by native 9851A0.
fn pack_color(color: Vec3) -> u32 {
    let [red, green, blue] = color
        .to_array()
        .map(|channel| (f64::from(channel) * 255.0).round_ties_even() as i32 as u8);
    u32::from(red) << 16 | u32::from(green) << 8 | u32::from(blue)
}
