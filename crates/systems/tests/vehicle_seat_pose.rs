//! Original-instruction evidence for the passenger model placement consumer.

use glam::{Mat4, Vec3};
use solarity_systems::{VehicleSeatPose, vehicle_seat_attachment, vehicle_seat_transform};

#[test]
fn animated_and_missing_seat_attachments_match_native_matrices()
-> Result<(), Box<dyn std::error::Error>> {
    let mut count = 0;
    for line in include_str!("fixtures/vehicle-seat-pose-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let values: Vec<u32> = line
            .split_whitespace()
            .map(|word| u32::from_str_radix(word, 16))
            .collect::<Result<_, _>>()?;
        let floats: Vec<f32> = values[2..].iter().copied().map(f32::from_bits).collect();
        let vector = |index| Vec3::from_slice(&floats[index..index + 3]);
        let actual = vehicle_seat_transform(VehicleSeatPose {
            passenger_yaw: floats[0],
            rotation: vector(1),
            offset: vector(4),
            passenger_anchor: (values[1] != 0).then(|| vector(7)),
            passenger_scale: floats[10],
            vehicle_scale: floats[11],
            vehicle_position: vector(12),
            vehicle_yaw: floats[15],
            attachment: (values[0] != 0).then(|| Mat4::from_cols_slice(&floats[16..32])),
        });
        assert_eq!(
            actual.to_cols_array().map(f32::to_bits),
            values[34..50],
            "case {count}"
        );
        count += 1;
    }
    assert_eq!(count, 512);
    for (index, attachment) in [
        20, 34, 19, 21, 22, 17, 23, 24, 25, 15, 16, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 0,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(vehicle_seat_attachment(index as i32), Some(attachment));
    }
    for missing in [-1, i32::MIN, 22, 255, i32::MAX] {
        assert_eq!(vehicle_seat_attachment(missing), None);
    }
    Ok(())
}
