//! External native camera-anchor, ray, and retreat regression.

use glam::Vec3;
use solarity_ecs::{PlayerViewState, WorldTransform};
use solarity_systems::{
    CameraSubjectGeometry, MountCameraGeometry, PlayerCameraHeightState, PlayerCameraLiquidState,
    PlayerCameraObstructionSettings, PlayerCameraSceneQuery, resolve_camera_subject_height,
    resolve_mounted_player_camera_pose, resolve_player_camera_obstruction,
};

/// Native primary constraints replay through the public mounted-player camera.
#[test]
fn primary_constraints_match_original_execution() -> Result<(), Box<dyn std::error::Error>> {
    for line in concat!(
        include_str!("../fixtures/camera-primary-native.txt"),
        include_str!("../fixtures/camera-primary-targets-native.txt")
    )
    .lines()
    .filter(|line| !line.starts_with('#'))
    {
        let groups = line
            .split('|')
            .map(|group| {
                group
                    .split_whitespace()
                    .map(|word| u32::from_str_radix(word, 16))
                    .collect::<Result<Vec<_>, _>>()
            })
            .collect::<Result<Vec<_>, _>>()?;
        let v = &groups[0];
        let f = |i: usize| f32::from_bits(v[i]);
        let point = |i: usize| Vec3::new(f(i), f(i + 1), f(i + 2));
        // The public player controller admits heights >= 5/6. Lower
        // native values belong to camera modes outside this controller.
        if f(10) < 0.833_333_3 {
            continue;
        }
        let base = resolve_camera_subject_height(CameraSubjectGeometry::new(
            Some(f(10) - 0.097_222_224),
            2.0,
            1.0,
        ))?;
        let mut heights = PlayerCameraHeightState::new(base);
        heights.set_mounted(true, 0.0)?;
        heights.update_mount(MountCameraGeometry::new(None, Some(f(11))), 0.0)?;
        let forward = point(3);
        let pitch = (-forward.z).atan2(forward.truncate().length());
        let yaw = forward.y.atan2(forward.x);
        let pose = resolve_mounted_player_camera_pose(
            WorldTransform::new(point(0), yaw),
            PlayerViewState::new(f(9), pitch, 0.0, 2),
            heights.sample(10_000.0)?,
        )?;
        let settings = PlayerCameraObstructionSettings {
            water_collision: v[12] != 0,
            subject_liquid: match v[13] {
                0 => PlayerCameraLiquidState::Absent,
                1 => PlayerCameraLiquidState::Surface { depth: f(14) },
                _ => PlayerCameraLiquidState::Submerged { depth: f(14) },
            },
            minimum_subject_height: (f(15) >= 0.0).then(|| f(15) * 0.75),
            distance_target: v.get(19).copied().map(f32::from_bits),
            height_target: v.get(20).copied().map(f32::from_bits),
        };
        let mut calls = Vec::new();
        let mut ray_index = 0;
        let resolved = resolve_player_camera_obstruction(pose, 16.0 / 9.0, settings, |query| {
            Ok::<_, std::convert::Infallible>(match query {
                PlayerCameraSceneQuery::Segment { start, end, water } => {
                    calls.extend(
                        start
                            .to_array()
                            .into_iter()
                            .chain(end.to_array())
                            .map(f32::to_bits),
                    );
                    calls.push(if water { 0x120171 } else { 0x100171 });
                    let fraction = if ray_index == 0
                        && f64::from(f(10).max(settings.height_target.unwrap_or(f(10))))
                            - f64::from(0.2_f32)
                            > f64::from(0.000_000_953_674_3_f32)
                    {
                        f(16)
                    } else {
                        f(17)
                    };
                    ray_index += 1;
                    (fraction >= 0.0).then_some(fraction)
                }
                PlayerCameraSceneQuery::Volume { .. } => (f(18) >= 0.0).then(|| f(18)),
            })
        })?;
        for (index, actual) in [resolved.distance(), resolved.height()]
            .into_iter()
            .enumerate()
        {
            assert!(
                (actual - f32::from_bits(groups[1][index])).abs() < 0.000_01,
                "result {index}: {actual}; {line}"
            );
        }
        let eye = resolved.pose().eye();
        let contacts = resolved.contacts();
        assert_eq!(
            (u32::from(contacts.orbit) * 0x10000) | (u32::from(contacts.anchor) * 0x20000),
            groups[1][6],
            "contact flags; {line}"
        );
        for (index, actual) in eye.to_array().into_iter().enumerate() {
            assert!(
                (actual - f32::from_bits(groups[1][3 + index])).abs() < 0.000_2,
                "eye {index}: {actual}; {line}"
            );
        }
        assert_eq!(calls.len(), groups[2].len(), "{line}");
        for (index, (&actual, &expected)) in calls.iter().zip(&groups[2]).enumerate() {
            if index % 7 == 6 {
                assert_eq!(actual, expected, "{line}");
            } else {
                assert!(
                    (f32::from_bits(actual) - f32::from_bits(expected)).abs() < 0.000_2,
                    "trace {index}: {}; {line}",
                    f32::from_bits(actual)
                );
            }
        }
    }
    Ok(())
}
