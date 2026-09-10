//! Original-code captures cover justification and final font projection.

use super::TextOrigin;

#[test]
fn fractional_text_origins_match_native_translation() -> Result<(), std::num::ParseFloatError> {
    for row in include_str!("../fixtures/font_translation_native.txt")
        .lines()
        .filter(|row| !row.starts_with('#'))
    {
        let row: Vec<f64> = row
            .split_whitespace()
            .map(str::parse)
            .collect::<Result<_, _>>()?;
        let [
            _,
            height,
            left,
            bottom,
            width,
            box_height,
            line_height,
            horizontal,
            vertical,
            native_x,
            native_y,
        ] = row[..]
        else {
            panic!("native fixture columns");
        };
        let pixels = height / 768.;
        let anchor_x = width * horizontal * 0.5;
        let anchor_y = match vertical as u8 {
            0 => 0.,
            1 => (line_height - box_height) * 0.5,
            2 => line_height - box_height,
            _ => panic!("native justification"),
        };
        let owner = [left, bottom + box_height];
        let origin = TextOrigin::new([anchor_x, anchor_y], pixels);
        let delta = origin.offset(owner, 1.);
        assert!(
            ((owner[0] + anchor_x + delta[0]) * pixels - (native_x + 1.)).abs() < 0.00001,
            "{row:?}"
        );
        assert!(
            ((owner[1] + anchor_y + delta[1]) * pixels - (native_y - 1.)).abs() < 0.00001,
            "{row:?}"
        );
    }
    Ok(())
}

/// Original font projection plus the D3D9-to-Vulkan sample-grid conversion.
#[test]
fn text_sample_positions_match_native_font_projection() -> Result<(), std::num::ParseFloatError> {
    for row in include_str!("../fixtures/font_projection_native.txt")
        .lines()
        .filter(|row| !row.starts_with('#'))
    {
        let row: Vec<f64> = row
            .split_whitespace()
            .map(str::parse)
            .collect::<Result<_, _>>()?;
        let [_, height, x, y, native_x, native_y] = row[..] else {
            panic!("native projection fixture columns");
        };
        let pixels = height / 768.;
        for scale in [0.5, 1., 2.] {
            let owner = [x / pixels, y / pixels];
            let delta = TextOrigin::new([0.; 2], pixels).offset(owner, scale);
            let projected = [
                (owner[0] + delta[0]) * pixels,
                height - (owner[1] + delta[1]) * pixels,
            ];
            // D3D9's pixel (i,j) is sampled at (i,j); Vulkan samples it at
            // (i+0.5,j+0.5). Preserve the native texture coordinate there.
            assert!((projected[0] - (native_x + 0.5)).abs() < 0.0001, "{row:?}");
            assert!((projected[1] - (native_y + 0.5)).abs() < 0.0001, "{row:?}");
        }
    }
    Ok(())
}
