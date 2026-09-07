//! Original setup and sampling capture, including rejected position blends.

use super::{RemoteMovementBlend, RemoteMovementPose};
use glam::Vec3;
use solarity_ecs::WorldTransform;

fn pose(words: &[u32]) -> RemoteMovementPose {
    RemoteMovementPose {
        transform: WorldTransform::new(
            Vec3::new(
                f32::from_bits(words[0]),
                f32::from_bits(words[1]),
                f32::from_bits(words[2]),
            ),
            f32::from_bits(words[3]),
        ),
        pitch: f32::from_bits(words[4]),
        transport_guid: 0,
    }
}

#[test]
fn lookahead_matches_original_position_and_angle_samples() -> Result<(), Box<dyn std::error::Error>>
{
    let mut count = 0;
    for line in include_str!("../fixtures/remote-blend-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let columns: Vec<Vec<u32>> = line
            .split(" | ")
            .map(|column| {
                column
                    .split_whitespace()
                    .map(|word| u32::from_str_radix(word, 16))
                    .collect::<Result<Vec<_>, _>>()
            })
            .collect::<Result<Vec<_>, _>>()?;
        let input = &columns[0];
        let current = pose(&columns[1]);
        let endpoint = pose(&columns[2]);
        let mut analytic = pose(&columns[3]);
        let expected = &columns[4];
        let mut blend = RemoteMovementBlend::new(current, endpoint, 0, input[1], input[0])
            .ok_or("same-space nonzero blend was rejected")?;
        blend.sample(
            current.transform.position(),
            input[2],
            input[3],
            &mut analytic,
        );
        let p = analytic.transform.position();
        let actual = [
            p.x,
            p.y,
            p.z,
            analytic.transform.orientation(),
            analytic.pitch,
        ];
        for (index, (actual, expected)) in actual.into_iter().zip(expected).enumerate() {
            assert!(
                (actual - f32::from_bits(*expected)).abs() <= 0.000_001,
                "row {count} axis {index}: {actual:?} vs {:?}",
                f32::from_bits(*expected)
            );
        }
        assert_eq!(u32::from(blend.flags), expected[5], "row {count}");
        count += 1;
    }
    assert_eq!(count, 720);
    Ok(())
}
