//! Fixed shader vertex layout produced directly by effect geometry jobs.
//! POD derivation rejects padding; little-endian uploads borrow the complete stream.

/// Fixed 24-byte ribbon vertex matching stock's `CGxVertexPCT0` payload.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct M2RibbonRenderVertex {
    pub(super) position: [f32; 3],
    pub(super) color_bgra: [u8; 4],
    pub(super) texture_coordinates: [f32; 2],
}

impl M2RibbonRenderVertex {
    /// Size of one explicitly serialized ribbon vertex.
    pub const BYTE_SIZE: usize = 24;

    /// Returns the already transformed world-space strip position.
    #[must_use]
    pub const fn position(self) -> [f32; 3] {
        self.position
    }

    /// Returns stock's packed color in little-endian BGRA byte order.
    #[must_use]
    pub const fn color_bgra(self) -> [u8; 4] {
        self.color_bgra
    }

    /// Returns the age-progressed coordinates inside the selected atlas cell.
    #[must_use]
    pub const fn texture_coordinates(self) -> [f32; 2] {
        self.texture_coordinates
    }

    pub(crate) fn to_bytes(self) -> [u8; Self::BYTE_SIZE] {
        let mut bytes = [0_u8; Self::BYTE_SIZE];
        let mut offset = 0;
        for component in self.position {
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
