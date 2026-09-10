//! 7493B0 destinations include its distinct anchor and parent tilt policy.

use glam::{Mat4, Vec3};
use solarity_systems::{VehicleSeatPose, vehicle_entry_target};

#[test]
fn entry_destinations_match_original_instructions() -> Result<(), Box<dyn std::error::Error>> {
    let mut count = 0;
    for line in include_str!("fixtures/vehicle-entry-target-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let words: Vec<u32> = line
            .split_whitespace()
            .map(|word| u32::from_str_radix(word, 16))
            .collect::<Result<_, _>>()?;
        let values: Vec<f32> = words[3..].iter().copied().map(f32::from_bits).collect();
        let vector = |index| Vec3::from_slice(&values[index..index + 3]);
        let pose = VehicleSeatPose {
            passenger_yaw: values[0],
            rotation: vector(1),
            offset: vector(4),
            passenger_anchor: (words[1] != 0).then(|| vector(7)),
            passenger_scale: values[10],
            vehicle_scale: values[11],
            vehicle_position: vector(12),
            vehicle_yaw: values[15],
            attachment: (words[0] != 0).then(|| Mat4::from_cols_slice(&values[16..32])),
        };
        let actual = vehicle_entry_target(
            pose,
            Mat4::from_cols_slice(&values[32..48]),
            Mat4::from_cols_slice(&values[48..64]),
            words[2] != 0,
        );
        assert_eq!(
            actual.to_array().map(f32::to_bits),
            words[67..70],
            "case {count}"
        );
        count += 1;
    }
    assert_eq!(count, 512);
    Ok(())
}
