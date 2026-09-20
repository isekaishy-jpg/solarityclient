//! Fixed shader vertex layout produced directly by effect geometry jobs.
//! POD derivation rejects padding; little-endian uploads borrow the complete stream.

/// Fixed 36-byte particle vertex matching stock's PNC0T0 streams.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct M2ParticleRenderVertex {
    pub(super) position: [f32; 3],
    pub(super) normal: [f32; 3],
    pub(super) color_bgra: [u8; 4],
    pub(super) texture_coordinates: [f32; 2],
}

impl M2ParticleRenderVertex {
    /// Size of one explicitly serialized particle vertex.
    pub const BYTE_SIZE: usize = 36;

    /// Returns the camera-facing world-space position.
    #[must_use]
    pub const fn position(self) -> [f32; 3] {
        self.position
    }

    /// Returns stock's world-up lighting normal, independent of the card plane.
    #[must_use]
    pub const fn normal(self) -> [f32; 3] {
        self.normal
    }

    /// Returns stock's packed color in little-endian BGRA byte order.
    #[must_use]
    pub const fn color_bgra(self) -> [u8; 4] {
        self.color_bgra
    }

    /// Returns coordinates inside the particle's held atlas cell.
    #[must_use]
    pub const fn texture_coordinates(self) -> [f32; 2] {
        self.texture_coordinates
    }

    pub(crate) fn to_bytes(self) -> [u8; Self::BYTE_SIZE] {
        let mut bytes = [0_u8; Self::BYTE_SIZE];
        let mut offset = 0;
        for component in self.position.into_iter().chain(self.normal) {
            bytes[offset..offset + 4].copy_from_slice(&component.to_le_bytes());
            offset += 4;
        }
        bytes[offset..offset + 4].copy_from_slice(&self.color_bgra);
        offset += 4;
        for component in self.texture_coordinates {
            bytes[offset..offset + 4].copy_from_slice(&component.to_le_bytes());
            offset += 4;
        }
        bytes
    }
}
