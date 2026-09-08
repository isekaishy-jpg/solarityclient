//! Exact 48-byte MFOG volumes and their separate exterior/liquid banks.

use glam::Vec3;

/// One authored fog end, start ratio and packed ARGB color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldModelFogBank {
    pub(super) end: f32,
    pub(super) start_ratio: f32,
    pub(super) color: u32,
}

impl WorldModelFogBank {
    /// Captures one already-validated native bank triplet.
    #[must_use]
    pub const fn new(end: f32, start_ratio: f32, color: u32) -> Self {
        Self {
            end,
            start_ratio,
            color,
        }
    }
    /// Returns the unmodified bank end and start multiplier.
    #[must_use]
    pub const fn range(self) -> (f32, f32) {
        (self.end, self.start_ratio)
    }

    /// Returns the packed color, including its authored alpha byte.
    #[must_use]
    pub const fn packed_color(self) -> u32 {
        self.color
    }

    /// Returns the RGB consumed by scene fog.
    #[must_use]
    pub fn color(self) -> Vec3 {
        Vec3::new(
            ((self.color >> 16) & 255) as f32,
            ((self.color >> 8) & 255) as f32,
            (self.color & 255) as f32,
        ) / 255.
    }

    /// Applies 7A0CD0's stored float opacity and packed-byte color interpolation.
    #[must_use]
    pub fn blend(self, other: Self, weight: f32) -> Self {
        let blend = |a: f32, b: f32| {
            ((f64::from(b) - f64::from(a)) * f64::from(weight) + f64::from(a)) as f32
        };
        let alpha = (weight * 255.0).round_ties_even() as i32 & 255;
        let mut color = self.color;
        if alpha == 255 {
            color = (color & 0xff00_0000) | (other.color & 0x00ff_ffff);
        } else if alpha != 0 {
            color &= 0xff00_0000;
            for shift in [0, 8, 16] {
                let a = ((self.color >> shift) & 255) as i32;
                let b = ((other.color >> shift) & 255) as i32;
                color |= ((a + (((b - a) * alpha) >> 8)) as u32 & 255) << shift;
            }
        }
        Self {
            end: blend(self.end, other.end),
            start_ratio: blend(self.start_ratio, other.start_ratio),
            color,
        }
    }
}

/// One root-local MFOG sphere; entry zero supplies the complete base palette.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldModelFog {
    pub(super) flags: u32,
    pub(super) position: Vec3,
    pub(super) inner_radius: f32,
    pub(super) outer_radius: f32,
    pub(super) banks: [WorldModelFogBank; 2],
}

impl WorldModelFog {
    /// Returns native selection and liquid-bank flags.
    #[must_use]
    pub const fn flags(self) -> u32 {
        self.flags
    }

    /// Returns the WMO-local fog center.
    #[must_use]
    pub const fn position(self) -> Vec3 {
        self.position
    }

    /// Returns the full-strength and zero-strength sphere radii.
    #[must_use]
    pub const fn radii(self) -> (f32, f32) {
        (self.inner_radius, self.outer_radius)
    }

    /// Returns the dry and underwater end/ratio/color banks, in native order.
    #[must_use]
    pub const fn banks(self) -> [WorldModelFogBank; 2] {
        self.banks
    }
}

/// The base MFOG banks after the first camera group's ordered local overlays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldModelFogPalette {
    flags: u32,
    banks: [WorldModelFogBank; 2],
}

impl WorldModelFogPalette {
    /// Captures the selected native flags and both fully blended banks.
    #[must_use]
    pub const fn new(flags: u32, banks: [WorldModelFogBank; 2]) -> Self {
        Self { flags, banks }
    }
    /// Returns the nearest admitted local volume's flags, or entry zero's flags.
    #[must_use]
    pub const fn flags(self) -> u32 {
        self.flags
    }

    /// Returns independently blended dry and underwater banks.
    #[must_use]
    pub const fn banks(self) -> [WorldModelFogBank; 2] {
        self.banks
    }
}

/// Samples 7A1150's exact four-index palette in WMO-local coordinates.
/// Returns `None` for an absent base volume, invalid index, or nonfinite point.
#[must_use]
pub fn sample_world_model_fog(
    volumes: &[WorldModelFog],
    indices: [u8; 4],
    position: Vec3,
) -> Option<WorldModelFogPalette> {
    if !position.is_finite() {
        return None;
    }
    let base = *volumes.first()?;
    let mut palette = WorldModelFogPalette {
        flags: base.flags,
        banks: base.banks,
    };
    // Native's one-based max heap bubbles equal insertions toward the root,
    // selects the right child on equal distances, and keeps the last key on
    // equal pop comparisons. A stable distance sort has different tie order.
    let mut heap = [(0_f32, 0_usize); 5];
    let mut count = 0;
    for index in indices.into_iter().filter(|index| *index != 0) {
        let volume = volumes.get(usize::from(index))?;
        let d = volume.position.as_dvec3() - position.as_dvec3();
        let distance = ((d.z * d.z + d.y * d.y) + d.x * d.x).sqrt();
        if distance >= f64::from(volume.outer_radius) || volume.flags & 1 != 0 {
            continue;
        }
        let distance = distance as f32;
        count += 1;
        let mut slot = count;
        while slot > 1 && distance >= heap[slot / 2].0 {
            heap[slot] = heap[slot / 2];
            slot /= 2;
        }
        heap[slot] = (distance, usize::from(index));
    }
    while count > 0 {
        let (distance, index) = heap[1];
        let last = heap[count];
        count -= 1;
        let mut slot = 1;
        while slot * 2 <= count {
            let mut child = slot * 2;
            if child < count && heap[child].0 <= heap[child + 1].0 {
                child += 1;
            }
            if heap[child].0 <= last.0 {
                break;
            }
            heap[slot] = heap[child];
            slot = child;
        }
        heap[slot] = last;
        let volume = volumes[index];
        let distance = f64::from(distance.min(volume.outer_radius).max(0.0));
        let inner = f64::from(volume.inner_radius);
        let weight = if distance < inner {
            1.
        } else {
            (1. - (distance - inner) / (f64::from(volume.outer_radius) - inner)) as f32
        };
        palette.banks = std::array::from_fn(|i| palette.banks[i].blend(volume.banks[i], weight));
        if count == 0 {
            palette.flags = volume.flags;
        }
    }
    Some(palette)
}

#[cfg(test)]
#[path = "../../tests/stock_seed/world_model_fog_native.rs"]
mod tests;
