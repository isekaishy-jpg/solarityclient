//! Original-code captures cover the physical-pixel floor after justification.

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
            ((owner[0] + anchor_x + delta[0]) * pixels - native_x).abs() < 0.00001,
            "{row:?}"
        );
        assert!(
            ((owner[1] + anchor_y + delta[1]) * pixels - native_y).abs() < 0.00001,
            "{row:?}"
        );
    }
    Ok(())
}
