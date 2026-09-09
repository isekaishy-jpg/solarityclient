//! Native obstruction regression at close ground-level orbit distances.

use std::error::Error;

use glam::Vec3;
use solarity_ecs::{PlayerViewState, WorldTransform};
use solarity_systems::{
    CameraSubjectGeometry, PlayerCameraObstructionSettings, PlayerCameraSceneQuery,
    PlayerCameraVolumeError, PlayerCameraVolumeKind, resolve_camera_subject_height,
    resolve_player_camera_obstruction, resolve_player_camera_pose,
};

/// The original scalar banks prevent a false one-ninth retreat in the close zoom band.
#[test]
fn ground_camera_retains_native_distance_and_orientation() -> Result<(), Box<dyn Error>> {
    let mut checked = 0;
    for row in include_str!("../fixtures/camera-ground-native.txt")
        .lines()
        .filter(|row| !row.starts_with('#') && !row.is_empty())
    {
        let words = row
            .split('|')
            .map(|part| {
                part.split_whitespace()
                    .map(|word| u32::from_str_radix(word, 16).map(f32::from_bits))
                    .collect::<Result<Vec<_>, _>>()
            })
            .collect::<Result<Vec<_>, _>>()?;
        let input = &words[0];
        let expected = &words[1];
        let subject = Vec3::from_slice(&input[..3]);
        let height = resolve_camera_subject_height(CameraSubjectGeometry::new(
            Some(input[6] - 0.097_222_224),
            2.0,
            1.0,
        ))?;
        let pose = resolve_player_camera_pose(
            WorldTransform::new(subject, input[3]),
            PlayerViewState::new(input[5], input[4], 0.0, 2),
            height,
        )?;
        let result = resolve_player_camera_obstruction(
            pose,
            16.0 / 9.0,
            PlayerCameraObstructionSettings {
                water_collision: false,
                ..Default::default()
            },
            |query| {
                Ok::<_, PlayerCameraVolumeError>(match query {
                    PlayerCameraSceneQuery::Segment { start, end, .. } => {
                        if start.z >= subject.z && end.z < subject.z {
                            Some((start.z - subject.z) / (start.z - end.z))
                        } else {
                            None
                        }
                    }
                    PlayerCameraSceneQuery::Volume { volume, kind } => {
                        if kind == PlayerCameraVolumeKind::Water {
                            None
                        } else {
                            volume.triangle_retreat([
                                subject + Vec3::new(-100.0, -100.0, 0.0),
                                subject + Vec3::new(100.0, -100.0, 0.0),
                                subject + Vec3::new(0.0, 100.0, 0.0),
                            ])?
                        }
                    }
                })
            },
        )?;
        assert!(
            (result.distance() - expected[0]).abs() < 0.000_02,
            "distance {} != {}; {row}",
            result.distance(),
            expected[0]
        );
        assert!(
            (result.height() - expected[1]).abs() < 0.000_002,
            "height; {row}"
        );
        assert!(
            result
                .pose()
                .eye()
                .abs_diff_eq(Vec3::from_slice(&expected[3..6]), 0.000_25),
            "eye; {row}"
        );
        assert_eq!(result.pose().forward(), pose.forward());
        checked += 1;
    }
    assert_eq!(checked, 54);
    Ok(())
}
