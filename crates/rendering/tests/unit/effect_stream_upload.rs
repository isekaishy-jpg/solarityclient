//! Shader ABI regression coverage for batched effect stream copies.

use super::write_stream;
use crate::{M2ParticleRenderVertex, M2RibbonRenderVertex, VulkanError};

/// Arbitrary payload bits exercise signed zero, infinities and NaNs without
/// arithmetic canonicalization, alongside the packed BGRA byte lanes.
fn verify_stream<T: bytemuck::Pod, const N: usize>(
    encode: impl Fn(T) -> [u8; N] + Copy,
) -> Result<(), VulkanError> {
    for count in [0, 1, 2, 257, 4096] {
        let values: Vec<T> = (0..count)
            .map(|index| {
                let mut bytes = vec![0; N];
                let patterns = [0_u32, 0x8000_0000, 0x7f80_0000, 0xff80_0000, 0x7fc0_0123];
                for (word, lane) in bytes.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                    let bits = if index < patterns.len() {
                        patterns[index]
                    } else {
                        (index as u32).wrapping_mul(0x9e37_79b9) ^ word as u32
                    };
                    lane.copy_from_slice(&bits.to_ne_bytes());
                }
                bytemuck::pod_read_unaligned(&bytes)
            })
            .collect();
        let mut expected = vec![0xa5; 7];
        for &value in &values {
            expected.extend_from_slice(&encode(value));
        }
        expected.extend_from_slice(&[0xa5; 11]);
        let mut destination = vec![0xa5; expected.len()];
        write_stream(
            destination.as_mut_ptr(),
            7,
            destination.len() as u64,
            &values,
            encode,
        )?;
        assert_eq!(destination, expected, "stream count {count}");
    }
    Ok(())
}

#[test]
fn particle_stream_preserves_pnc0t0_bytes_and_surrounding_ranges() -> Result<(), VulkanError> {
    verify_stream::<M2ParticleRenderVertex, 36>(M2ParticleRenderVertex::to_bytes)
}

#[test]
fn ribbon_stream_preserves_pct0_bytes_and_surrounding_ranges() -> Result<(), VulkanError> {
    verify_stream::<M2RibbonRenderVertex, 24>(M2RibbonRenderVertex::to_bytes)
}

#[test]
fn index_stream_preserves_unsigned_little_endian_words() -> Result<(), VulkanError> {
    verify_stream::<u32, 4>(u32::to_le_bytes)
}

#[test]
fn stream_rejects_out_of_bounds_destination() {
    let mut destination = [0xa5; 12];
    assert!(matches!(
        write_stream(
            destination.as_mut_ptr(),
            11,
            12,
            &[0x1234_u32],
            u32::to_le_bytes
        ),
        Err(VulkanError::WorldFrameCapacity)
    ));
    assert_eq!(destination, [0xa5; 12]);
    assert!(matches!(
        write_stream(
            destination.as_mut_ptr(),
            u64::MAX,
            12,
            &[0_u32],
            u32::to_le_bytes
        ),
        Err(VulkanError::WorldFrameCapacity)
    ));
    assert_eq!(destination, [0xa5; 12]);
}
