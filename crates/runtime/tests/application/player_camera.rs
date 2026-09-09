//! Native mouse, zoom and pivot state histories.

use super::{
    CameraZoom, PlayerCameraInput, PlayerCameraMouseSettings, PlayerCameraZoomSettings,
    mouse_angles,
};
use glam::Vec3;
use solarity_ecs::{PlayerViewState, WorldTransform};
use solarity_systems::{
    CameraSubjectGeometry, PlayerCameraContacts, PlayerCameraObstructionSettings,
    PlayerCameraSceneQuery, PlayerCameraVolumeError, PlayerCameraVolumeKind,
    resolve_camera_subject_height, resolve_player_camera_obstruction, resolve_player_camera_pose,
};

/// Complete original mouse execution covers pivot admission, reversal and ordinary orbit.
#[test]
fn pivot_mouse_events_match_stock_pitch_banks() -> Result<(), Box<dyn std::error::Error>> {
    let mut count = 0;
    for line in concat!(
        include_str!("../fixtures/camera-pivot-native.txt"),
        include_str!("../fixtures/camera-pivot-edges-native.txt")
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
        let expected = &groups[1];
        let mut camera = PlayerCameraInput::new(
            PlayerViewState::new(5.55, f32::from_bits(v[0]), 0.7, 2),
            0.0,
        );
        camera.flags = v[2];
        camera.pivot.current = f32::from_bits(v[1]);
        camera.pivot.set_goal(camera.pivot.current);
        camera.pivot.active = v[2] & 0x8000000 != 0;
        let settings = PlayerCameraMouseSettings {
            yaw_speed: 180.0,
            pitch_speed: 90.0,
            invert_yaw: v[7] != 0,
            invert_pitch: v[8] != 0,
            pivot: super::PlayerCameraPivotSettings {
                enabled: v[4] != 0,
                maximum_yaw_delta: f32::from_bits(v[9]),
                minimum_pitch_delta: f32::from_bits(v[10]),
                return_speed: 45.0,
            },
        };
        camera.motion(
            [f32::from_bits(v[5]), f32::from_bits(v[6])],
            settings,
            v[3],
            1000,
        );
        for (actual, expected) in [camera.view(0.0).pitch_radians(), camera.pivot_pitch()]
            .into_iter()
            .zip(&expected[..2])
        {
            assert!(
                (actual - f32::from_bits(*expected)).abs() <= f32::EPSILON * 2.0,
                "pitch bank {actual} != {}; {line}",
                f32::from_bits(*expected)
            );
        }
        assert_eq!(
            camera.pivot.active,
            expected[2] & 0x8000000 != 0,
            "pivot return; {line}"
        );
        for sample in expected[9..].as_chunks::<3>().0 {
            camera.sample_follow(sample[0]);
            assert!(
                (camera.pivot_pitch() - f32::from_bits(sample[1])).abs() <= f32::EPSILON * 2.0,
                "pivot return at {}; {line}",
                sample[0]
            );
            assert_eq!(camera.pivot.active, sample[2] & 0x8000000 != 0, "{line}");
        }
        count += 1;
    }
    assert_eq!(count, 3792);
    Ok(())
}

/// The same mouse gesture selects stationary-eye pitch or inward orbit from its yaw component.
#[test]
fn ground_contact_preserves_both_drag_behaviors() -> Result<(), Box<dyn std::error::Error>> {
    let mut camera = PlayerCameraInput::new(PlayerViewState::new(2.0, -0.6, 0.0, 2), 0.0);
    camera.set_free_look(true, 0.0);
    let settings = PlayerCameraMouseSettings {
        yaw_speed: 180.0,
        pitch_speed: 90.0,
        invert_yaw: false,
        invert_pitch: false,
        pivot: Default::default(),
    };
    camera.contacts(
        PlayerCameraContacts {
            anchor: false,
            orbit: true,
        },
        0,
        settings.pivot,
        1000,
    );
    let height = resolve_camera_subject_height(CameraSubjectGeometry::new(Some(1.75), 2.0, 1.0))?;
    let transform = WorldTransform::new(Vec3::new(1340.0, -4380.0, 28.0), 0.0);
    let before = resolve_player_camera_pose(transform, camera.view(0.0), height)?;
    camera.motion([0.0, -20.0], settings, 0, 1000);
    let planted = resolve_player_camera_pose(transform, camera.view(0.0), height)?
        .with_view_pitch_offset(camera.pivot_pitch())?;
    assert_eq!(before.eye(), planted.eye());
    assert!(planted.forward().z > before.forward().z);
    assert_eq!(camera.view(0.0).pitch_radians(), -0.6);

    // Reversal consumes the accumulated view angle, then resumes ordinary orbit.
    camera.motion([0.0, 20.0], settings, 0, 1000);
    assert!(camera.pivot_pitch().abs() < 0.000_001);
    camera.motion([100.0, -20.0], settings, 0, 1000);
    assert!(camera.view(0.0).pitch_radians() < -0.6);
    assert!(camera.pivot_pitch().abs() < 0.000_001);
    Ok(())
}

/// 605D60's requested-distance query keeps a shortened eye in the pivot mode.
#[test]
fn continued_upward_drag_retains_ground_contact_at_the_shortened_distance()
-> Result<(), Box<dyn std::error::Error>> {
    let subject = Vec3::new(1340.0, -4380.0, 28.0);
    let transform = WorldTransform::new(subject, 0.0);
    let height = resolve_camera_subject_height(CameraSubjectGeometry::new(
        Some(1.75 - 0.097_222_224),
        2.0,
        1.0,
    ))?;
    let mut camera = PlayerCameraInput::new(PlayerViewState::new(5.55, -0.6, 0.0, 2), 0.0);
    camera.zoom.distance = 2.7;
    camera.set_free_look(true, 0.0);
    let mouse = PlayerCameraMouseSettings {
        yaw_speed: 180.0,
        pitch_speed: 90.0,
        invert_yaw: false,
        invert_pitch: false,
        pivot: Default::default(),
    };
    let planted_eye = resolve_player_camera_pose(transform, camera.view(0.0), height)?.eye();
    for frame in 0..20 {
        let time = 1000 + frame * 16;
        camera.sample_follow(time);
        let pose = resolve_player_camera_pose(transform, camera.view(0.0), height)?;
        let obstruction = resolve_player_camera_obstruction(
            pose,
            16.0 / 9.0,
            PlayerCameraObstructionSettings {
                water_collision: false,
                distance_target: Some(camera.distance_target()),
                height_target: Some(height.value()),
                ..Default::default()
            },
            |query| ground_scene(subject, query),
        )?;
        assert!(
            obstruction.contacts().orbit,
            "contact lost on frame {frame}"
        );
        camera.contacts(obstruction.contacts(), 0, mouse.pivot, time);
        camera.motion([0.0, -20.0], mouse, 0, time);
        let tilted = obstruction
            .pose()
            .with_view_pitch_offset(camera.pivot_pitch())?;
        assert_eq!(tilted.eye(), planted_eye);
        assert_eq!(camera.view(0.0).pitch_radians(), -0.6);
        assert!(!camera.pivot.active);
    }
    Ok(())
}

/// 606F90 selects 5FEF10 when contact resumes, stopping an earlier return lane.
#[test]
fn renewed_ground_contact_stops_pivot_return() {
    let mut camera = PlayerCameraInput::new(PlayerViewState::new(5.55, -0.6, 0.0, 2), 0.0);
    let settings = super::PlayerCameraPivotSettings::default();
    camera.pivot.current = -0.25;
    camera.contacts(PlayerCameraContacts::default(), 0, settings, 1000);
    camera.sample_follow(1050);
    assert!(camera.pivot.active);
    let angle = camera.pivot_pitch();
    assert!(angle > -0.25 && angle < 0.0);
    camera.contacts(
        PlayerCameraContacts {
            anchor: false,
            orbit: true,
        },
        0,
        settings,
        1050,
    );
    camera.sample_follow(3000);
    assert_eq!(camera.pivot_pitch(), angle);
    assert!(!camera.pivot.active);
}

/// Controlled horizontal ground uses the real Systems camera-volume clipper.
fn ground_scene(
    subject: Vec3,
    query: PlayerCameraSceneQuery<'_>,
) -> Result<Option<f32>, PlayerCameraVolumeError> {
    match query {
        PlayerCameraSceneQuery::Segment { start, end, .. } => Ok((start.z >= subject.z
            && end.z < subject.z)
            .then(|| (start.z - subject.z) / (start.z - end.z))),
        PlayerCameraSceneQuery::Volume { volume, kind } => {
            if kind == PlayerCameraVolumeKind::Water {
                return Ok(None);
            }
            volume.triangle_retreat([
                subject + Vec3::new(-100.0, -100.0, 0.0),
                subject + Vec3::new(100.0, -100.0, 0.0),
                subject + Vec3::new(0.0, 100.0, 0.0),
            ])
        }
    }
}

#[test]
fn collision_recovery_and_wheel_input_match_original_histories()
-> Result<(), Box<dyn std::error::Error>> {
    for line in include_str!("../../tests/fixtures/camera-obstruction-recovery-native.txt")
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
        let mut zoom = CameraZoom::new(f32::from_bits(groups[0][0]));
        for (action, expected) in groups[1]
            .as_chunks::<3>()
            .0
            .iter()
            .zip(groups[2].as_chunks::<5>().0)
        {
            match action[0] {
                0 | 1 => zoom.request(action[0] == 0, f32::from_bits(action[2]), action[1], 8.33),
                2 => zoom.sample(action[1], PlayerCameraZoomSettings::default()),
                3 => zoom.obstructed(f32::from_bits(action[2]), action[1]),
                _ => unreachable!(),
            }
            assert!(
                (zoom.distance - f32::from_bits(expected[0])).abs() < 0.000_01,
                "distance action={action:?} actual={} expected={}",
                zoom.distance,
                f32::from_bits(expected[0])
            );
            assert!((zoom.target - f32::from_bits(expected[1])).abs() < 0.000_01);
            assert_eq!(zoom.recovery.is_some(), expected[2] != 0);
            if let Some((start, anchor)) = zoom.recovery {
                assert_eq!(start, expected[3]);
                assert!((anchor - f32::from_bits(expected[4])).abs() < 0.000_01);
            }
        }
    }
    Ok(())
}

#[test]
fn zoom_histories_match_original_requests_and_ticks() -> Result<(), Box<dyn std::error::Error>> {
    for line in include_str!("../../tests/fixtures/camera-zoom-native.txt")
        .lines()
        .skip(1)
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
        let settings = PlayerCameraZoomSettings {
            speed: f32::from_bits(groups[0][0]),
            maximum: f32::from_bits(groups[0][1]),
            maximum_factor: f32::from_bits(groups[0][2]),
        };
        let mut zoom = CameraZoom::new(f32::from_bits(groups[0][3]));
        for action in groups[1].as_chunks::<3>().0 {
            if action[0] == 2 {
                zoom.sample(action[1], settings);
            } else {
                zoom.request(
                    action[0] == 0,
                    f32::from_bits(action[2]),
                    action[1],
                    settings.speed,
                );
            }
        }
        assert_eq!(
            [
                zoom.distance.to_bits(),
                zoom.flags,
                zoom.starts[0],
                zoom.starts[1],
                zoom.stops[0],
                zoom.stops[1],
                zoom.deadlines[0],
                zoom.deadlines[1]
            ],
            groups[2].as_slice(),
            "{line}"
        );
    }
    Ok(())
}

#[test]
fn mouse_angles_match_original_camera_instructions() -> Result<(), Box<dyn std::error::Error>> {
    for line in include_str!("../../tests/fixtures/camera-mouse-native.txt")
        .lines()
        .skip(1)
    {
        let words: Vec<_> = line
            .split_whitespace()
            .filter(|word| *word != "|")
            .map(|word| u32::from_str_radix(word, 16))
            .collect::<Result<_, _>>()?;
        let actual = mouse_angles(
            [f32::from_bits(words[0]), f32::from_bits(words[1])],
            PlayerCameraMouseSettings {
                yaw_speed: f32::from_bits(words[2]),
                pitch_speed: f32::from_bits(words[3]),
                invert_yaw: words[4] != 0,
                invert_pitch: words[5] != 0,
                pivot: Default::default(),
            },
        );
        assert_eq!(actual.map(f32::to_bits), [words[6], words[7]], "{line}");
    }
    Ok(())
}

#[test]
fn orbit_keeps_world_direction_when_subject_turns() {
    let mut camera = PlayerCameraInput::new(PlayerViewState::default(), 1.);
    camera.set_free_look(true, 1.);
    camera.motion(
        [100., 50.],
        PlayerCameraMouseSettings {
            yaw_speed: 180.,
            pitch_speed: 90.,
            invert_yaw: false,
            invert_pitch: false,
            pivot: Default::default(),
        },
        0,
        1000,
    );
    let direction = camera.yaw();
    assert!(direction > 0.5 && direction < 0.7);
    assert!((2. + camera.view(2.).yaw_offset_radians() - direction).abs() < 0.000_001);
    assert!(camera.view(2.).pitch_radians() > PlayerViewState::default().pitch_radians());
    camera.set_free_look(false, 2.);
    assert!(!camera.free_look());
    assert!((2. + camera.view(2.).yaw_offset_radians() - direction).abs() < 0.000_001);
}
