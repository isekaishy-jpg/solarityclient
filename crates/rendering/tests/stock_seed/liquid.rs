//! Liquid presentation comparisons against the fingerprinted original client.

use std::error::Error;
use std::num::NonZeroU32;

use solarity_rendering::{
    LiquidDepthCoordinates, LiquidDepthTexture, LiquidDepthTextureKind, LiquidTextureTimeline,
    liquid_magma_surface_transform, liquid_water_surface_transform,
};

/// Native matrix functions preserve authored angle conversion and unsigned scroll periods.
#[test]
fn liquid_texture_transforms_match_original_matrix_instructions() -> Result<(), Box<dyn Error>> {
    let fixture = include_bytes!("../fixtures/liquid_texture_transforms.bin");
    assert_eq!(fixture.len(), 46 * 84);
    for record in fixture.as_chunks::<84>().0 {
        let value = |index| f32::from_bits(read_word(record, index));
        let actual = if read_word(record, 0) == 0 {
            liquid_water_surface_transform(value(1), value(2))
        } else {
            liquid_magma_surface_transform([value(2), value(3)], value(1), read_word(record, 4))?
        };
        for (index, value) in actual.to_cols_array().into_iter().enumerate() {
            let expected = read_word(record, index + 5);
            assert_eq!(
                value.to_bits(),
                expected,
                "input {:?}, matrix element {index}",
                &record[..20]
            );
        }
    }
    Ok(())
}

/// Original 79E3C0 output covers every authored depth byte in both banks.
#[test]
fn liquid_depth_coordinates_match_all_native_lookup_entries() {
    let fixture = include_bytes!("../fixtures/liquid_depth_coordinates.bin");
    assert_eq!(fixture.len(), 512 * 4);
    for (bank, mode) in [LiquidDepthCoordinates::River, LiquidDepthCoordinates::Ocean]
        .into_iter()
        .enumerate()
    {
        for depth in 0..=u8::MAX {
            let offset = (bank * 256 + usize::from(depth)) * 4;
            let expected = read_word(fixture, offset / 4);
            assert_eq!(
                mode.coordinate(depth).to_bits(),
                expected,
                "{mode:?} {depth}"
            );
        }
    }
}

/// Original 8A1D60 includes exact ties, non-divisible periods, and clock wrap.
#[test]
fn liquid_texture_frames_match_native_resident_sequence_selection() -> Result<(), Box<dyn Error>> {
    let fixture = include_bytes!("../fixtures/liquid_texture_frames.bin");
    assert_eq!(fixture.len(), 409 * 16);
    for bytes in fixture.as_chunks::<16>().0 {
        let words: [u32; 4] = std::array::from_fn(|index| read_word(bytes, index));
        let [count, period_ms, time_ms, expected] = words;
        let timeline = LiquidTextureTimeline::new(
            NonZeroU32::new(count).ok_or("empty native sequence")?,
            period_ms,
        );
        assert_eq!(timeline.frame_index(time_ms), expected, "{words:?}");
    }
    Ok(())
}

/// Complete native texture callbacks cover gradients, river HSV tail, and WMO split.
#[test]
fn liquid_depth_textures_match_native_pixel_callbacks() {
    const RECORD_SIZE: usize = 36 + 2048;
    let fixture = include_bytes!("../fixtures/liquid_depth_textures.bin");
    assert_eq!(fixture.len(), 12 * RECORD_SIZE);
    for record in fixture.as_chunks::<RECORD_SIZE>().0 {
        let word = |index| read_word(record, index);
        let kind = match word(0) {
            0 => LiquidDepthTextureKind::River,
            1 => LiquidDepthTextureKind::Ocean,
            2 => LiquidDepthTextureKind::WorldModel,
            _ => unreachable!(),
        };
        let (color_start, alpha_start) = if kind == LiquidDepthTextureKind::River {
            (1, 7)
        } else {
            (3, 5)
        };
        let colors = [word(color_start), word(color_start + 1)];
        let alphas = [word(alpha_start), word(alpha_start + 1)]
            .map(|bits| (f32::from_bits(bits) * 255.0).round_ties_even() as u8);
        let texture = LiquidDepthTexture::prepare(kind, colors, alphas);
        for (index, (actual, expected)) in texture
            .pixels_rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .zip(record[36..].as_chunks::<4>().0)
            .enumerate()
        {
            assert_eq!(
                *actual,
                [expected[2], expected[1], expected[0], expected[3]],
                "{kind:?} {colors:?} pixel {index}"
            );
        }
    }
}

/// Reads one word after each test has validated its complete fixture extent.
fn read_word(bytes: &[u8], index: usize) -> u32 {
    let offset = index * 4;
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}
