//! Model callback overrides applied after scene-light collection.

use crate::{M2LocalLightCount, M2LocalLightState};

use super::{M2PointLight, M2Sunlight};

/// Stock M2 callback replacement of collected lighting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum M2LightOverride {
    /// `0x004E2730` replaces directional accumulation and preserves point lights.
    Directional(M2Sunlight),
    /// `0x00834900` resets the entire accumulator, as for default Glue ghosts.
    All(M2Sunlight),
}

impl M2LightOverride {
    /// Returns the replacement directional slot.
    #[must_use]
    pub const fn sunlight(self) -> M2Sunlight {
        match self {
            Self::Directional(sunlight) | Self::All(sunlight) => sunlight,
        }
    }

    /// Counts scene points surviving the callback and hardware slot limit.
    #[must_use]
    pub fn point_light_count(self, available: usize) -> usize {
        match self {
            Self::Directional(_) => available.min(3),
            Self::All(_) => 0,
        }
    }

    /// Returns the shader permutation count, including the directional slot.
    #[must_use]
    pub fn local_light_count(self, available_points: usize) -> M2LocalLightCount {
        match self.point_light_count(available_points) {
            0 => M2LocalLightCount::One,
            1 => M2LocalLightCount::Two,
            2 => M2LocalLightCount::Three,
            _ => M2LocalLightCount::Four,
        }
    }

    /// Prepares the override and retained scene points for the GPU.
    #[must_use]
    pub fn local_lights(self, points: &[M2PointLight]) -> [M2LocalLightState; 4] {
        let mut lights = [M2LocalLightState::disabled(); 4];
        lights[0] = self.sunlight().local_light_state();
        for (index, point) in points
            .iter()
            .take(self.point_light_count(points.len()))
            .enumerate()
        {
            lights[index + 1] = point.local_light_state();
        }
        lights
    }
}
