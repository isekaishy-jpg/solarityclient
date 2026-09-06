//! Ordinary-player orbit, timed zoom and camera follow from build-12340.

use solarity_ecs::PlayerViewState;

mod follow;
pub(super) use follow::FollowSettings as PlayerCameraFollowSettings;

#[derive(Clone, Copy)]
pub(super) struct PlayerCameraZoomSettings {
    pub speed: f32,
    pub maximum: f32,
    pub maximum_factor: f32,
}

impl Default for PlayerCameraZoomSettings {
    fn default() -> Self {
        Self {
            speed: 8.33,
            maximum: 15.,
            maximum_factor: 1.,
        }
    }
}

/// Ordinary distance input banks from 5FF950/5FFA60 and 6000E0. Saved-view
/// interpolation and special-subject limits belong to separate camera modes.
#[derive(Clone, Copy)]
struct CameraZoom {
    distance: f32,
    target: f32,
    recovery: Option<(u32, f32)>,
    flags: u32,
    starts: [u32; 2],
    stops: [u32; 2],
    deadlines: [u32; 2],
}

impl CameraZoom {
    fn new(distance: f32) -> Self {
        Self {
            distance,
            target: distance,
            recovery: None,
            flags: 0,
            starts: [0; 2],
            stops: [0; 2],
            deadlines: [0; 2],
        }
    }

    fn request(&mut self, inward: bool, amount: f32, time: u32, speed: f32) {
        // The original temporarily selects x87 truncation, stores an i64,
        // then retains its low u32. A zero interval means an indefinite hold.
        let duration = f64::from(amount) / f64::from(speed) * 1_000.;
        let duration =
            if (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&duration) {
                duration as i64 as u32
            } else {
                0
            };
        let direction = usize::from(!inward);
        let opposite = 1 - direction;
        if self.flags & (1 << (opposite * 2)) != 0 {
            self.stops[opposite] = time;
            self.flags |= 2 << (opposite * 2);
        }
        let held = 1 << (direction * 2);
        if self.deadlines[direction] != 0 && self.flags & held != 0 {
            self.deadlines[direction] = self.deadlines[direction].wrapping_add(duration);
            return;
        }
        if self.flags & held == 0 {
            self.flags |= held;
            self.starts[direction] = time;
        }
        self.deadlines[direction] = if duration == 0 {
            0
        } else {
            time.wrapping_add(duration)
        };
    }

    fn sample(&mut self, time: u32, settings: PlayerCameraZoomSettings) {
        let maximum =
            (f64::from(settings.maximum) * f64::from(settings.maximum_factor)).clamp(0., 50.);
        for direction in 0..2 {
            let held = 1 << (direction * 2);
            let stopped = held << 1;
            if self.flags & held == 0 {
                continue;
            }
            let deadline = self.deadlines[direction];
            if deadline != 0 && time.wrapping_sub(deadline) as i32 >= 0 {
                self.stops[direction] = deadline;
                self.flags |= stopped;
            }
            let elapsed = if self.flags & stopped != 0 {
                let elapsed = self.stops[direction].wrapping_sub(self.starts[direction]) as i32;
                self.flags &= !(held | stopped);
                elapsed.max(0) as u32
            } else {
                let elapsed = time.wrapping_sub(self.starts[direction]) as i32;
                if elapsed < 0 {
                    continue;
                }
                self.starts[direction] = time;
                elapsed as u32
            };
            let delta = f64::from(elapsed) * f64::from(settings.speed) * f64::from(0.001_f32);
            self.target = if direction == 0 {
                (f64::from(self.target) - delta).max(0.) as f32
            } else {
                (f64::from(self.target) + delta).min(maximum) as f32
            };
            if self.recovery.is_none() {
                self.distance = self.target;
            }
        }
        self.sample_recovery(time);
    }

    /// 606F90 retains the requested distance and restarts the 603D30 lane
    /// after an obstruction removes more than one ninth of a world unit.
    fn obstructed(&mut self, distance: f32, time: u32) {
        if f64::from(self.distance) - f64::from(distance) > f64::from(0.111_111_11_f32) {
            self.distance = distance + 0.111_112_066_f32;
            self.recovery = Some((time, self.distance));
        }
    }

    fn sample_recovery(&mut self, time: u32) {
        if (f64::from(self.target) - f64::from(self.distance)).abs() < f64::from(f32::EPSILON * 2.)
        {
            self.target = self.distance;
            self.recovery = None;
            return;
        }
        let Some((start, anchor)) = self.recovery else {
            return;
        };
        let elapsed = time.wrapping_sub(start);
        if (elapsed as i32) < 0 {
            return;
        }
        let fraction = f64::from(elapsed) * f64::from(0.001_f32) / 2.;
        self.distance = if fraction < 1. {
            let fraction = f64::from(fraction as f32);
            ((1. - (fraction * f64::from(std::f32::consts::PI)).cos())
                * 0.5
                * (f64::from(self.target) - f64::from(anchor))
                + f64::from(anchor)) as f32
        } else {
            self.target
        };
    }
}

#[derive(Clone, Copy)]
pub(super) struct PlayerCameraMouseSettings {
    pub yaw_speed: f32,
    pub pitch_speed: f32,
    pub invert_yaw: bool,
    pub invert_pitch: bool,
}

/// Free look retains a world-space yaw while the subject can turn independently.
#[derive(Clone, Copy)]
pub(super) struct PlayerCameraInput {
    view_slot: u8,
    zoom: CameraZoom,
    flags: u32,
    world_yaw: f32,
    follow: [follow::FollowAngle; 2],
}

impl PlayerCameraInput {
    pub(super) fn new(view: PlayerViewState, facing: f32) -> Self {
        Self {
            view_slot: view.view(),
            zoom: CameraZoom::new(view.distance()),
            flags: 0,
            world_yaw: wrap_yaw(facing + view.yaw_offset_radians()),
            follow: [
                follow::FollowAngle::new(view.pitch_radians()),
                follow::FollowAngle::new(view.yaw_offset_radians()),
            ],
        }
    }

    pub(super) fn zoom(
        &mut self,
        inward: bool,
        amount: f32,
        time: u32,
        settings: PlayerCameraZoomSettings,
    ) {
        self.zoom.request(inward, amount, time, settings.speed);
    }

    pub(super) fn sample_zoom(&mut self, time: u32, settings: PlayerCameraZoomSettings) {
        self.zoom.sample(time, settings);
    }

    pub(super) fn obstructed(&mut self, distance: f32, time: u32) {
        self.zoom.obstructed(distance, time);
    }

    pub(super) fn follow_input(
        &mut self,
        previous: u32,
        held: u32,
        time: u32,
        settings: &PlayerCameraFollowSettings,
        movement_flags: u32,
    ) {
        // 5FA170/5FA450 notify the camera at ordinary held-control edges,
        // including button release. Active-axis bookkeeping is not an edge.
        if (previous ^ held) & 0x13f3 == 0 {
            return;
        }
        follow::request(
            &mut self.follow,
            self.flags,
            held,
            !follow::idle(previous) && follow::idle(held),
            time,
            settings,
            movement_flags & 0x0220_0000 != 0,
        );
    }

    pub(super) fn sample_follow(&mut self, time: u32) {
        self.follow[0].sample(time);
        if !self.free_look() {
            self.follow[1].sample(time);
        }
    }

    pub(super) fn free_look(&self) -> bool {
        self.flags & 1 != 0
    }

    pub(super) fn set_free_look(&mut self, enabled: bool, facing: f32) {
        if enabled == self.free_look() {
            return;
        }
        if enabled {
            self.world_yaw = wrap_yaw(facing + self.follow[1].current);
            for angle in &mut self.follow {
                angle.cancel();
            }
        } else {
            self.follow[1].current = self.world_yaw - facing;
            for angle in &mut self.follow {
                angle.cancel();
            }
        }
        self.flags = self.flags & !1 | u32::from(enabled);
    }

    pub(super) fn set_sticky_camera(&mut self, sticky: bool) {
        self.flags = self.flags & !0x20 | if sticky { 0x20 } else { 0 };
    }

    pub(super) fn yaw(&self) -> f32 {
        self.world_yaw
    }

    pub(super) fn view(&self, facing: f32) -> PlayerViewState {
        PlayerViewState::new(
            self.zoom.distance,
            self.follow[0].current,
            if self.free_look() {
                self.world_yaw - facing
            } else {
                self.follow[1].current
            },
            self.view_slot,
        )
    }

    /// SDL supplies pixel deltas, after the coordinate conversion performed by
    /// native 47C020. 6020B0 scales them against the original 800x600 basis.
    pub(super) fn motion(&mut self, delta: [f32; 2], settings: PlayerCameraMouseSettings) {
        if !self.free_look() || !delta.into_iter().all(f32::is_finite) {
            return;
        }
        self.flags |= 0x40;
        let [yaw, pitch] = mouse_angles(delta, settings);
        self.world_yaw = wrap_yaw(self.world_yaw - yaw);
        self.follow[0].current = (self.follow[0].current + pitch).clamp(-1.553_343, 1.553_343);
    }
}

fn mouse_angles(delta: [f32; 2], settings: PlayerCameraMouseSettings) -> [f32; 2] {
    // Preserve the native x87 product until its float store. These are image
    // constants at A1E904, A1E900, and 9E2B40, not viewport-size scaling.
    let radians = f64::from(0.017_453_292_f32);
    [
        (f64::from(delta[0]) * f64::from(0.001_25_f32) * f64::from(settings.yaw_speed) * radians)
            as f32
            * if settings.invert_yaw { -1. } else { 1. },
        (f64::from(delta[1])
            * f64::from(0.001_666_666_7_f32)
            * radians
            * f64::from(settings.pitch_speed)) as f32
            * if settings.invert_pitch { -1. } else { 1. },
    ]
}

fn wrap_yaw(value: f32) -> f32 {
    // 4C5090 takes the remainder against the double 2*pi, then adds the
    // image's float 2*pi for a negative result before the final float store.
    let wrapped = f64::from(value) % std::f64::consts::TAU;
    (if wrapped < 0. {
        wrapped + f64::from(std::f32::consts::TAU)
    } else {
        wrapped
    }) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

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
                    0 | 1 => {
                        zoom.request(action[0] == 0, f32::from_bits(action[2]), action[1], 8.33)
                    }
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
    fn zoom_histories_match_original_requests_and_ticks() -> Result<(), Box<dyn std::error::Error>>
    {
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
            },
        );
        let direction = camera.yaw();
        assert!(direction > 0.5 && direction < 0.7);
        assert!((2. + camera.view(2.).yaw_offset_radians() - direction).abs() < 0.000_001);
        assert!(camera.view(2.).pitch_radians() > PlayerViewState::default().pitch_radians());
        camera.set_free_look(false, 2.);
        assert!(!camera.free_look());
        assert!((2. + camera.view(2.).yaw_offset_radians() - direction).abs() < 0.000_001);
    }
}
