//! Ordered ordinary-player mouse orbit state from build-12340 Camera.cpp.

use solarity_ecs::PlayerViewState;

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
    flags: u32,
    starts: [u32; 2],
    stops: [u32; 2],
    deadlines: [u32; 2],
}

impl CameraZoom {
    fn new(distance: f32) -> Self {
        Self {
            distance,
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
            self.distance = if direction == 0 {
                (f64::from(self.distance) - delta).max(0.) as f32
            } else {
                (f64::from(self.distance) + delta).min(maximum) as f32
            };
        }
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
    view: PlayerViewState,
    zoom: CameraZoom,
    flags: u32,
    world_yaw: f32,
}

impl PlayerCameraInput {
    pub(super) fn new(view: PlayerViewState, facing: f32) -> Self {
        Self {
            view,
            zoom: CameraZoom::new(view.distance()),
            flags: 0,
            world_yaw: wrap_yaw(facing + view.yaw_offset_radians()),
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

    pub(super) fn free_look(&self) -> bool {
        self.flags & 1 != 0
    }

    pub(super) fn set_free_look(&mut self, enabled: bool, facing: f32) {
        if enabled == self.free_look() {
            return;
        }
        if enabled {
            self.world_yaw = wrap_yaw(facing + self.view.yaw_offset_radians());
        } else {
            self.view = self.view(facing);
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
            self.view.pitch_radians(),
            if self.free_look() {
                self.world_yaw - facing
            } else {
                self.view.yaw_offset_radians()
            },
            self.view.view(),
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
        let pitch = (self.view.pitch_radians() + pitch).clamp(-1.553_343, 1.553_343);
        self.view = PlayerViewState::new(
            self.view.distance(),
            pitch,
            self.view.yaw_offset_radians(),
            self.view.view(),
        );
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
