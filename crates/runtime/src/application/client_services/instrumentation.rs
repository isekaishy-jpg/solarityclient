//! Sparse scene and residency observations belong to explicitly marked detail frames.

use super::ClientServices;

impl ClientServices {
    /// Registry walks and queue snapshots never run during ordinary capture frames.
    pub(super) fn profile_scene_context(&self) {
        if !solarity_profiling::detail_enabled() {
            return;
        }
        let _profile = solarity_profiling::detail_profile!("diagnostics.scene_snapshot");
        let (width, height) = self.platform.pixel_extent();
        solarity_profiling::profile_value!("scene.width", width);
        solarity_profiling::profile_value!("scene.height", height);
        solarity_profiling::profile_value!("scene.in_world", self.gameplay.world().is_some());
        if let Some(world) = self.gameplay.world()
            && let Ok(transform) = world.local_player_transform()
        {
            let position = transform.position();
            solarity_profiling::profile_value!("scene.player_x_f32_bits", position.x.to_bits());
            solarity_profiling::profile_value!("scene.player_y_f32_bits", position.y.to_bits());
            solarity_profiling::profile_value!("scene.player_z_f32_bits", position.z.to_bits());
        }
        if let Ok(cpu) = self.cpu.snapshot() {
            solarity_profiling::profile_value!("cpu.in_flight", cpu.in_flight());
        }
        let usage = self.renderer.resource_usage();
        solarity_profiling::profile_value!("gpu.m2_mesh_bytes", usage.m2_meshes.1);
        solarity_profiling::profile_value!("gpu.ui_mesh_bytes", usage.ui_meshes.1);
        solarity_profiling::profile_value!("gpu.glyph_page_bytes", usage.glyph_pages.1);
        solarity_profiling::profile_value!("gpu.character_atlas_bytes", usage.character_atlases.1);
        solarity_profiling::profile_value!("gpu.common_texture_bytes", usage.common_textures.1);
        solarity_profiling::profile_value!(
            "gpu.pending_retirement_batches",
            usage.pending_retirement_batches
        );
    }
}
