//! Checked palette upload into a caller-owned mapped bone range.

use super::M2BonePaletteSource;
use crate::VulkanError;
use glam::Mat4;

/// Writes ordered world pages followed by sky bones, preserving shader column order.
/// The supplied range must contain at least one matrix for an empty descriptor.
pub(crate) fn write_palette_bytes(
    source: &(impl M2BonePaletteSource + ?Sized),
    sky: &[Mat4],
    destination: &mut [u8],
) -> Result<(), VulkanError> {
    let world_bytes = source
        .len()
        .checked_mul(64)
        .ok_or(VulkanError::WorldFrameCapacity)?;
    let total_bytes = sky
        .len()
        .checked_mul(64)
        .and_then(|bytes| world_bytes.checked_add(bytes))
        .ok_or(VulkanError::WorldFrameCapacity)?;
    let destination = destination
        .get_mut(..total_bytes.max(64))
        .ok_or(VulkanError::WorldFrameCapacity)?;
    let mut offset: usize = 0;
    for index in 0..source.palette_count() {
        let palette = source.palette(index);
        let end = palette
            .len()
            .checked_mul(64)
            .and_then(|bytes| offset.checked_add(bytes))
            .filter(|end| *end <= world_bytes)
            .ok_or(VulkanError::WorldFrameCapacity)?;
        copy_palette(palette, &mut destination[offset..end]);
        offset = end;
    }
    if offset != world_bytes {
        return Err(VulkanError::WorldFrameCapacity);
    }
    copy_palette(sky, &mut destination[world_bytes..total_bytes]);
    if total_bytes == 0 {
        destination.fill(0);
    }
    Ok(())
}

/// Glam's POD representation is column-major; big-endian hosts encode explicitly.
fn copy_palette(palette: &[Mat4], destination: &mut [u8]) {
    #[cfg(target_endian = "little")]
    destination.copy_from_slice(bytemuck::cast_slice(palette));
    #[cfg(target_endian = "big")]
    for (matrix, bytes) in palette.iter().zip(destination.chunks_exact_mut(64)) {
        for (value, bytes) in matrix.to_cols_array().iter().zip(bytes.chunks_exact_mut(4)) {
            bytes.copy_from_slice(&value.to_le_bytes());
        }
    }
}
