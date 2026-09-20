//! Page upload matches the previous scalar wire encoding and rejects invalid ranges.

use super::{M2BonePaletteSource, write_palette_bytes};
use glam::Mat4;

/// Synthetic noncontiguous pages exercise the same renderer contract as worker jobs.
struct Pages<'a> {
    pages: &'a [&'a [Mat4]],
    length: usize,
}

impl M2BonePaletteSource for Pages<'_> {
    fn len(&self) -> usize {
        self.length
    }
    fn palette_count(&self) -> usize {
        self.pages.len()
    }
    fn palette(&self, index: usize) -> &[Mat4] {
        self.pages[index]
    }
}

/// The native shader consumes 16 little-endian column values per matrix.
fn scalar_bytes(matrices: impl Iterator<Item = Mat4>) -> Vec<u8> {
    matrices
        .flat_map(|matrix| {
            matrix
                .to_cols_array()
                .into_iter()
                .flat_map(f32::to_le_bytes)
        })
        .collect()
}

/// Preserves page order, exact float bits, sky offsets and untouched trailing capacity.
#[test]
fn disjoint_palettes_and_sky_preserve_exact_shader_bytes() -> Result<(), Box<dyn std::error::Error>>
{
    let first = [Mat4::from_translation(glam::Vec3::new(7., -4., 2.))];
    let second = [
        Mat4::from_cols_array(&std::array::from_fn(|index| {
            f32::from_bits(0x7f800000 + index as u32)
        })),
        Mat4::IDENTITY,
    ];
    let sky = [Mat4::from_scale(glam::Vec3::new(2., 3., 4.))];
    let pages = Pages {
        pages: &[&first, &[], &second],
        length: 3,
    };
    let mut bytes = vec![0x55; 5 * 64];
    write_palette_bytes(&pages, &sky, &mut bytes)?;
    assert_eq!(
        &bytes[..4 * 64],
        scalar_bytes(first.into_iter().chain(second).chain(sky))
    );
    assert_eq!(&bytes[4 * 64..], &[0x55; 64]);
    Ok(())
}

/// Empty shader descriptors still receive one zero matrix.
#[test]
fn empty_palettes_initialize_only_the_required_descriptor_matrix()
-> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = [0x55; 128];
    write_palette_bytes(&[], &[], &mut bytes)?;
    assert_eq!(&bytes[..64], &[0; 64]);
    assert_eq!(&bytes[64..], &[0x55; 64]);
    Ok(())
}

/// A malformed source cannot overrun its declared world region or destination.
#[test]
fn palette_count_mismatch_and_short_destination_are_errors() {
    let matrix = [Mat4::IDENTITY];
    let mut bytes = [0x55; 128];
    for length in [0, 2, usize::MAX] {
        assert!(
            write_palette_bytes(
                &Pages {
                    pages: &[&matrix],
                    length
                },
                &[],
                &mut bytes
            )
            .is_err()
        );
    }
    assert!(write_palette_bytes(&matrix, &matrix, &mut bytes[..127]).is_err());
    assert!(write_palette_bytes(&[], &[], &mut bytes[..63]).is_err());
}
