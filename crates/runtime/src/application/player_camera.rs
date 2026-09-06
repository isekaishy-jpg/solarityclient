//! Ordered ordinary-player mouse orbit state from build-12340 Camera.cpp.

use solarity_ecs::PlayerViewState;

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
    flags: u32,
    world_yaw: f32,
}

impl PlayerCameraInput {
    pub(super) fn new(view: PlayerViewState, facing: f32) -> Self {
        Self {
            view,
            flags: 0,
            world_yaw: wrap_yaw(facing + view.yaw_offset_radians()),
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
            self.view.distance(),
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
