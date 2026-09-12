//! Packed upload bytes must preserve the original explicit serialization ABI.

use super::*;

#[test]
fn packed_wmo_upload_preserves_every_float_bit_and_index() -> Result<(), Box<dyn std::error::Error>>
{
    let patterns = [
        0, 0x80000000, 0x3f800000, 0xbf800000, 0x00000001, 0x007fffff, 0x7f7fffff, 0x7f800000,
        0xff800000, 0x7fc12345, 0xffa54321,
    ];
    let vertices = (0..patterns.len())
        .map(|offset| {
            let values: [f32; 18] = std::array::from_fn(|index| {
                f32::from_bits(patterns[(offset + index) % patterns.len()])
            });
            WorldModelRenderVertex::new(
                std::array::from_fn(|index| values[index]),
                std::array::from_fn(|index| values[index + 3]),
                [
                    std::array::from_fn(|index| values[index + 6]),
                    std::array::from_fn(|index| values[index + 8]),
                ],
                std::array::from_fn(|index| values[index + 10]),
                std::array::from_fn(|index| values[index + 14]),
            )
        })
        .collect();
    let mut plan = WorldModelMeshPlan {
        path: AssetPath::new("World\\UploadFixture.wmo")?,
        root_flags: 0,
        ambient_color: [0; 4],
        vertices,
        indices: vec![0, 1, 255, 65535, 65536, u32::MAX],
        materials: Vec::new(),
        draws: Vec::new(),
        shadow_draws: Vec::new(),
        groups: Vec::new(),
    };
    assert_eq!(plan.vertex_upload_bytes().as_ref(), plan.vertex_bytes());
    assert_eq!(plan.index_upload_bytes().as_ref(), plan.index_bytes());
    #[cfg(target_endian = "little")]
    {
        assert!(matches!(
            plan.vertex_upload_bytes(),
            std::borrow::Cow::Borrowed(_)
        ));
        assert!(matches!(
            plan.index_upload_bytes(),
            std::borrow::Cow::Borrowed(_)
        ));
    }
    plan.vertices.clear();
    plan.indices.clear();
    assert!(plan.vertex_upload_bytes().is_empty());
    assert!(plan.index_upload_bytes().is_empty());
    Ok(())
}
