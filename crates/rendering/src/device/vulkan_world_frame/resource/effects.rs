//! Worker-produced effect streams already have the fixed shader vertex layout.

use super::copy_bytes;
#[cfg(target_endian = "big")]
use super::indexed_offset;
use crate::{M2ParticleRenderVertex, M2RibbonRenderVertex, VulkanError};

// These are wire-layout contracts, independent of the host pointer size.
const _: () = assert!(size_of::<M2ParticleRenderVertex>() == M2ParticleRenderVertex::BYTE_SIZE);
const _: () = assert!(size_of::<M2RibbonRenderVertex>() == M2RibbonRenderVertex::BYTE_SIZE);

/// Copies one complete validated stream into the exclusively owned mapped slot.
/// Little-endian producers require no per-element encoding or temporary buffer.
/// Big-endian hosts retain the explicit shader byte order, as palette uploads do.
pub(super) fn write_stream<T: bytemuck::Pod, const N: usize>(
    destination: *mut u8,
    offset: u64,
    total: u64,
    values: &[T],
    _encode: impl Fn(T) -> [u8; N],
) -> Result<(), VulkanError> {
    // POD alone proves absence of padding; the fixed stride is a separate contract.
    assert_eq!(size_of::<T>(), N, "effect stream matches its shader stride");
    #[cfg(target_endian = "little")]
    {
        copy_bytes(destination, offset, bytemuck::cast_slice(values), total)
    }
    #[cfg(target_endian = "big")]
    {
        for (index, &value) in values.iter().enumerate() {
            copy_bytes(
                destination,
                indexed_offset(offset, N as u64, index)?,
                &_encode(value),
                total,
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/effect_stream_upload.rs"]
mod tests;
