//! Independent native ground trajectory captures.
use solarity_ecs::WorldMovementSpeeds;
use solarity_systems::MovementGroundTrajectory;

#[test]
fn anchored_ground_motion_matches_original_executable() -> Result<(), Box<dyn std::error::Error>> {
    let speeds = WorldMovementSpeeds::new([
        2.5,
        7.,
        4.5,
        4.72,
        2.5,
        7.,
        4.5,
        std::f32::consts::PI,
        std::f32::consts::PI,
    ]);
    for (index, line) in include_str!("fixtures/movement-ground-trajectory-native.txt")
        .lines()
        .enumerate()
        .skip(1)
    {
        let tokens: Vec<u32> = line
            .split_whitespace()
            .enumerate()
            .map(|(index, v)| {
                if index == 3 {
                    v.parse()
                } else {
                    u32::from_str_radix(v, 16)
                }
            })
            .collect::<Result<_, _>>()?;
        let hex = |i| tokens[i];
        let trajectory =
            MovementGroundTrajectory::new(hex(0), hex(1) & 8 != 0, f32::from_bits(hex(2)), speeds)?;
        let sample = trajectory.sample(tokens[3]);
        let actual = [
            trajectory.direction().x,
            trajectory.direction().y,
            trajectory.speed(),
            sample.displacement.x,
            sample.displacement.y,
            sample.displacement.z,
            sample.orientation,
        ];
        for (field, value) in actual.into_iter().enumerate() {
            assert_eq!(
                value.to_bits(),
                hex(field + 4),
                "case {index} field {field}: {line}"
            );
        }
    }
    Ok(())
}
#[test]
fn yaw_including_stationary_falling_matches_original() -> Result<(), Box<dyn std::error::Error>> {
    for line in include_str!("fixtures/movement-yaw-native.txt").lines() {
        if line.starts_with('#') {
            continue;
        }
        let fields: Vec<_> = line.split_whitespace().collect();
        let word = |index: usize| u32::from_str_radix(fields[index], 16);
        let trajectory = solarity_systems::MovementYawTrajectory::new(
            word(0)?,
            word(1)? & 8 != 0,
            f32::from_bits(word(2)?),
            f32::from_bits(word(3)?),
        )?;
        assert_eq!(
            trajectory.sample(fields[4].parse()?).to_bits(),
            word(5)?,
            "{line}"
        );
    }
    Ok(())
}
