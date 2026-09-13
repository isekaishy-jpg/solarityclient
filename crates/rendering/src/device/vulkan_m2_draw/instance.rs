//! Packed instance records and exact compatibility boundaries for direct batches.

use super::M2PreparedDraw;
use crate::{M2DrawPushConstants, M2MaterialUniform};

impl M2PreparedDraw {
    /// Packed std430 material and palette selection, with no per-draw alignment
    /// padding. The same stream serves single draws and instanced submissions.
    pub const INSTANCE_BYTE_SIZE: usize =
        M2MaterialUniform::BYTE_SIZE + M2DrawPushConstants::BYTE_SIZE;

    /// Serializes one instance without relying on Rust's object representation.
    pub fn instance_bytes(self) -> [u8; Self::INSTANCE_BYTE_SIZE] {
        let mut bytes = [0; Self::INSTANCE_BYTE_SIZE];
        bytes[..M2MaterialUniform::BYTE_SIZE].copy_from_slice(&self.material().to_bytes());
        bytes[M2MaterialUniform::BYTE_SIZE..].copy_from_slice(&self.push_constants().to_bytes());
        bytes
    }

    /// Opaque adjacent packets may share a draw only when their fixed resources
    /// and lighting are identical. Transparent/effect ordering stays individual.
    pub fn can_instance_with(self, other: Self) -> bool {
        !self.effect_interleave()
            && !other.effect_interleave()
            && self.same_geometry(other)
            && self.scene_index() == other.scene_index()
            && self.light_bank() == other.light_bank()
            && self.material().fog_color() == other.material().fog_color()
    }

    /// Shadow passes have one scene and admit only opaque/alpha-test silhouettes.
    pub(crate) fn can_instance_shadow_with(self, other: Self) -> bool {
        self.shadow_material().is_some()
            && self.shadow_material() == other.shadow_material()
            && self.same_geometry(other)
    }

    /// Vertex/index ranges and compiled material resources define one GPU draw.
    fn same_geometry(self, other: Self) -> bool {
        self.mesh() == other.mesh()
            && self.pipeline() == other.pipeline()
            && self.texture_set() == other.texture_set()
            && self.first_index() == other.first_index()
            && self.index_count() == other.index_count()
    }
}
