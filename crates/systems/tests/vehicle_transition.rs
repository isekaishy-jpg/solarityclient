//! Original-instruction captures for passenger transition clocks and model motion.

use glam::Vec3;
use solarity_systems::{VehiclePassengerPhase, VehiclePassengerTransition, VehicleTransitionInput};

#[test]
fn passenger_transition_timing_and_motion_match_native() -> Result<(), Box<dyn std::error::Error>> {
    let mut count = 0;
    for line in include_str!("fixtures/vehicle-transition-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let values = line
            .split_whitespace()
            .map(|word| u32::from_str_radix(word, 16))
            .collect::<Result<Vec<_>, _>>()?;
        let floats = values[5..26]
            .iter()
            .copied()
            .map(f32::from_bits)
            .collect::<Vec<_>>();
        let vector = |at| Vec3::from_slice(&floats[at..at + 3]);
        let phase = match values[0] {
            1 => VehiclePassengerPhase::EnterDelay,
            2 => VehiclePassengerPhase::Entering,
            4 => VehiclePassengerPhase::ExitDelay,
            5 => VehiclePassengerPhase::Exiting,
            _ => return Err("unexpected native phase".into()),
        };
        let mut transition = VehiclePassengerTransition::new(VehicleTransitionInput {
            phase,
            has_parent: values[1] != 0,
            parameters: floats[..7].try_into()?,
            origin: vector(7),
            target: vector(10),
            unit_position: vector(13),
            parent_velocity: vector(16),
            yaw: floats[19],
            previous_yaw: floats[20],
            start_ms: values[3],
        });
        let initialized = [
            transition.end_ms(),
            transition.gravity().to_bits(),
            transition.arc_scale().to_bits(),
            transition.target_yaw().to_bits(),
        ];
        assert_eq!(
            initialized,
            values[26..30],
            "initialization case {count}: {line}"
        );
        let pose = transition.sample(values[4], values[2], vector(7), vector(10), floats[19]);
        let sampled = [
            pose.fraction.to_bits(),
            pose.position.x.to_bits(),
            pose.position.y.to_bits(),
            pose.position.z.to_bits(),
            pose.yaw.to_bits(),
        ];
        assert_eq!(sampled, values[30..35], "pose case {count}: {line}");
        count += 1;
    }
    assert_eq!(count, 2560);
    Ok(())
}
