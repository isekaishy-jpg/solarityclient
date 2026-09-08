//! Native layer/frame callbacks constrain the shared texture/glyph ordering.
use super::{UiFrameStrata, UiPresentationPacketKey};

#[test]
fn packet_order_matches_native_layer_frame_callbacks() -> Result<(), Box<dyn std::error::Error>> {
    for row in include_str!("../fixtures/ui_frame_layer_native.txt")
        .lines()
        .filter(|row| !row.starts_with('#'))
    {
        let values = row
            .split_whitespace()
            .map(str::parse::<usize>)
            .collect::<Result<Vec<_>, _>>()?;
        let expected = values[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|v| (v[0], v[1]))
            .collect::<Vec<_>>();
        let mut keys = Vec::new();
        for frame in (0..4).rev().filter(|frame| values[1] & (1 << frame) == 0) {
            for layer in (0..5).rev().filter(|layer| values[0] & (1 << layer) != 0) {
                keys.push(UiPresentationPacketKey {
                    strata: UiFrameStrata::Medium,
                    frame_level: 2,
                    frame_sequence: frame,
                    draw_rank: layer as i16 * 10 + (3 - frame) as i16 % 3,
                    draw_sub_level: 7 - frame as i16 * 4,
                });
            }
        }
        keys.sort();
        assert_eq!(
            keys.iter()
                .map(|key| (key.draw_rank as usize / 10, key.frame_sequence))
                .collect::<Vec<_>>(),
            expected,
            "{row}"
        );
    }
    Ok(())
}
