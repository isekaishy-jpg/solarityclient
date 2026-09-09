//! Terrain publication and retirement across pending transfers and frame-slot reuse.

use std::error::Error;

use glam::{Mat4, Vec3};
use solarity_rendering::{
    BlpTextureHandle, TerrainLayerCount, TerrainSceneUniform, TerrainTextureSet,
    TerrainTileMeshPlan, VulkanRenderer,
};

/// Reloads the same immutable ADT with fresh handles and identical visible pixels.
pub(super) fn reload_in_flight(
    renderer: &mut VulkanRenderer,
    plan: &TerrainTileMeshPlan,
    texture: BlpTextureHandle,
) -> Result<(), Box<dyn Error>> {
    renderer.retire_terrain_plans(std::iter::once(plan))?;
    let pipeline = renderer.prepare_terrain_pipeline(TerrainLayerCount::One)?;
    let eye = Vec3::new(-16., -16., 20.);
    let scene = TerrainSceneUniform::new(
        Mat4::orthographic_rh(-10., 10., -10., 10., 0.1, 100.),
        Mat4::look_at_rh(eye, eye - Vec3::Z, Vec3::Y),
        Vec3::ONE,
        Vec3::ZERO,
        Vec3::Z,
    );
    let mut previous = None;
    for generation in 0..9 {
        let mesh = renderer.upload_terrain_mesh(plan)?;
        let atlas = renderer.upload_terrain_material(plan)?;
        let request = TerrainTextureSet::new(atlas, &[texture])?;
        let set = renderer.prepare_terrain_texture_sets(std::slice::from_ref(&request))?[0];
        if let Some((old_mesh, old_atlas, old_set)) = previous {
            assert_ne!(mesh, old_mesh);
            assert_ne!(atlas, old_atlas);
            assert_ne!(set, old_set);
        }
        let draw = renderer.prepare_terrain_draw(mesh, pipeline, set, &request, plan, 0)?;
        // Generation zero retires before its first draw. Intermediate generations
        // retire immediately after submission, without a capture or host idle wait.
        if generation > 0 {
            if generation == 8 {
                renderer.request_frame_capture()?;
            }
            renderer.present_terrain(scene, &[draw])?;
        }
        renderer.retire_terrain_plans(std::iter::once(plan))?;
        assert!(renderer.terrain_mesh_info(mesh).is_none());
        assert!(renderer.terrain_material_info(atlas).is_none());
        assert!(renderer.terrain_texture_set_info(set).is_none());
        assert!(
            renderer
                .prepare_terrain_draw(mesh, pipeline, set, &request, plan, 0)
                .is_err()
        );
        previous = Some((mesh, atlas, set));
    }
    let frame = renderer
        .take_captured_frame()?
        .ok_or("missing reloaded terrain capture")?;
    for y in [24, 32, 40] {
        for x in [24, 32, 40] {
            let pixel = &frame.rgba8()[(y * 64 + x) * 4..(y * 64 + x) * 4 + 3];
            assert_eq!(pixel, &[0, 255, 0], "reloaded terrain pixel {x}/{y}");
        }
    }
    Ok(())
}
