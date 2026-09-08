//! 606F90's surfaced/submerged edges and native final-pitch requests.

use super::PlayerCameraInput;
use solarity_systems::PlayerCameraLiquidState;

#[derive(Clone, Copy)]
pub(in crate::application) struct WaterSettings {
    pub collision: bool,
    final_pitch: [f32; 2],
}

impl Default for WaterSettings {
    fn default() -> Self {
        Self {
            collision: true,
            final_pitch: [5.0; 2],
        }
    }
}

impl WaterSettings {
    pub fn read(number: &impl Fn(&str) -> Option<f32>) -> Self {
        Self {
            collision: number("cameraWaterCollision").unwrap_or(1.0) != 0.0,
            final_pitch: [
                number("cameraSurfaceFinalPitch").unwrap_or(5.0),
                number("cameraSubmergeFinalPitch").unwrap_or(5.0),
            ],
        }
    }

    fn transition(
        self,
        previous: PlayerCameraLiquidState,
        current: PlayerCameraLiquidState,
    ) -> Option<f32> {
        if !self.collision {
            return None;
        }
        let degrees = match (previous, current) {
            (
                PlayerCameraLiquidState::Submerged { .. },
                PlayerCameraLiquidState::Surface { .. },
            ) => self.final_pitch[0],
            (
                PlayerCameraLiquidState::Surface { .. },
                PlayerCameraLiquidState::Submerged { .. },
            ) => self.final_pitch[1],
            _ => return None,
        };
        let radians = (f64::from(degrees) * f64::from(0.017_453_292_f32)) as f32;
        (radians != 0.0).then_some(radians)
    }
}

impl PlayerCameraInput {
    pub(in crate::application) fn water_transition(
        &mut self,
        state: PlayerCameraLiquidState,
        settings: WaterSettings,
        pitch_speed: f32,
        time: u32,
    ) {
        let previous = self.liquid;
        self.liquid = state;
        let Some(goal) = settings.transition(previous, state) else {
            return;
        };
        if self.free_look() {
            self.follow[0].current = goal;
            self.follow[0].cancel();
        } else {
            self.follow[0].request(goal, 0.0, 1.0, pitch_speed, time);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn water_transition_requests_match_original_camera_block()
    -> Result<(), Box<dyn std::error::Error>> {
        let state = |value: u32| match value {
            0 => PlayerCameraLiquidState::Absent,
            1 => PlayerCameraLiquidState::Surface { depth: 1.0 },
            _ => PlayerCameraLiquidState::Submerged { depth: 2.0 },
        };
        for line in include_str!("../../../tests/fixtures/camera-water-pitch-native.txt")
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
            let settings = WaterSettings {
                collision: v[2] != 0,
                final_pitch: [f32::from_bits(v[4]), f32::from_bits(v[5])],
            };
            let goal = settings.transition(state(v[0]), state(v[1]));
            let mut camera =
                PlayerCameraInput::new(solarity_ecs::PlayerViewState::new(5.0, 0.317, 0.0, 2), 0.0);
            camera.liquid = state(v[0]);
            camera.set_free_look(v[3] != 0, 0.0);
            camera.water_transition(state(v[1]), settings, 45.0, 1000);
            assert_eq!(camera.follow[0].current.to_bits(), groups[1][0], "{line}");
            if v[3] == 0 {
                assert_eq!(
                    goal.map(f32::to_bits).into_iter().collect::<Vec<_>>(),
                    groups[2],
                    "{line}"
                );
                if let Some(goal) = goal {
                    camera.sample_follow(2000);
                    assert_eq!(camera.follow[0].current, goal, "{line}");
                }
            }
        }
        Ok(())
    }
}
