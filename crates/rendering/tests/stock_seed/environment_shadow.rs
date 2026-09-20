//! Cached region publication, shared Vulkan images, and quality transitions.

use std::error::Error;

use glam::{Mat4, Vec3};
use solarity_rendering::{
    M2PreparedDraw, TerrainPreparedDraw, VulkanRenderer, WorldEnvironmentM2Caster,
    WorldEnvironmentShadowFrame, WorldEnvironmentShadowState, WorldFrameScene,
    WorldPrimaryShadowFrame, WorldShadowProjection, WorldShadowQuality,
};

/// Keeps an off-camera primary map empty while the environment map shadows terrain.
/// Repeated same-quality owners also exercise cache invalidation without resizing.
#[allow(clippy::too_many_arguments)]
pub(super) fn compare_cache(
    renderer: &mut VulkanRenderer,
    scene: WorldFrameScene<'_>,
    terrain: TerrainPreparedDraw,
    caster: M2PreparedDraw,
    bones: &[Mat4],
    center: Vec3,
    eye: Vec3,
) -> Result<(), Box<dyn Error>> {
    let mut serial_images = Vec::new();
    for workers in [1, 4] {
        let recording_cpu = crate::support::recording_cpu_with_workers(workers)?;
        let mut image_index = 0;
        for value in [3, 4, 5, 3] {
            let quality = WorldShadowQuality::from_cvar(value).ok_or("environment quality")?;
            let primary =
                WorldShadowProjection::primary(quality, center + Vec3::X * 100., eye, -Vec3::Z)?;
            let casters = [WorldEnvironmentM2Caster {
                draw: caster,
                maps: 7,
            }];
            for casting in [true, false] {
                let mut state = WorldEnvironmentShadowState::new(quality);
                for frame_index in 0..30 {
                    let updates = state.advance(center)?;
                    let environment =
                        WorldEnvironmentShadowFrame::new(&state, updates, eye, -Vec3::Z)?
                            .with_casters(if casting { &casters } else { &[] }, &[]);
                    renderer.request_frame_capture()?;
                    renderer.present_world_frame(
                        &mut &recording_cpu,
                        scene
                            .with_primary_shadows(WorldPrimaryShadowFrame::new(primary, &[]))
                            .with_environment_shadows(environment),
                        bones,
                        &[terrain],
                        &[],
                        &[],
                        &[],
                        &[],
                        &[],
                        &[],
                        &[],
                    )?;
                    let image = renderer
                        .take_captured_frame()?
                        .ok_or("environment shadow capture")?;
                    if workers == 1 {
                        serial_images.push(image.rgba8().to_vec());
                    } else {
                        assert_eq!(
                            image.rgba8(),
                            serial_images[image_index],
                            "worker count changed shadow pixels at frame {image_index}"
                        );
                    }
                    image_index += 1;
                    // 874890 publishes its near map only after all nine cached regions.
                    let expected = if casting && (value == 5 || frame_index >= 8) {
                        45_u8
                    } else {
                        64
                    };
                    let pixel = &image.rgba8()[(32 * 64 + 32) * 4..(32 * 64 + 32) * 4 + 3];
                    assert!(
                        pixel[1].abs_diff(expected) <= 2,
                        "environment quality={value}, casting={casting}, frame={frame_index}: {pixel:?}, expected green={expected}"
                    );
                }
            }
        }
    }
    crate::world_model_shadow::compare_environment_casters(renderer, scene, terrain, center, eye)
}
