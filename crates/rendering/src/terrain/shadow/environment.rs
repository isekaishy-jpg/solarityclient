//! Original 874890 environment refresh ownership and partial-map viewports.

use glam::Vec3;
use std::sync::atomic::{AtomicU64, Ordering};

use super::{WorldShadowProjectionError, WorldShadowQuality};

/// The three environment extents defined by original B1D520.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldEnvironmentShadowMap {
    /// Forty world units around the published center.
    Near,
    /// One hundred sixty world units around the published center.
    Middle,
    /// Six hundred forty world units around the published center.
    Far,
}

impl WorldEnvironmentShadowMap {
    /// Original near-to-far texture order.
    pub const ALL: [Self; 3] = [Self::Near, Self::Middle, Self::Far];

    /// Half extent used by both caster and receiver projections.
    #[must_use]
    pub const fn radius(self) -> f32 {
        match self {
            Self::Near => 40.,
            Self::Middle => 160.,
            Self::Far => 640.,
        }
    }

    pub(super) fn depth_bias(self) -> f32 {
        // Original 7BAC10 multiplies 2/4/8 by the float constant 0.2.
        match self {
            Self::Near => 0.4,
            Self::Middle => 0.8,
            Self::Far => 1.6,
        }
    }
}

/// One region rendered into an environment map's unpublished texture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldEnvironmentShadowUpdate {
    /// Fixed center throughout a cached refresh cycle.
    pub center: Vec3,
    /// Half extent of the complete map in world units.
    pub radius: f32,
    /// Light-space left, right, bottom, and top bounds of this update.
    pub crop: [f32; 4],
    /// Normalized left, right, top, and bottom texture coordinates.
    pub viewport: [f32; 4],
    /// Destination texture, independent of the currently published receiver.
    pub buffer: usize,
    /// Original collector mask: static environment or all exterior casters.
    pub caster_mask: u32,
}

impl WorldEnvironmentShadowUpdate {
    /// Original 681F60 clamps the normalized viewport, then 6A99E0 rounds its
    /// pixel extent. Texture targets truncate top-down Y independently.
    #[must_use]
    pub fn pixel_viewport(self, size: u32) -> [u32; 4] {
        let [left, right, top, bottom] = self.viewport.map(|value| f64::from(value.clamp(0., 1.)));
        let size = f64::from(size);
        let x = (left * size + 0.5).trunc();
        let inverted_y = ((1. - bottom) * size + 0.5).trunc();
        [
            x as u32,
            (top * size).trunc() as u32,
            (right * size - x + 0.5).trunc() as u32,
            ((1. - top) * size - inverted_y + 0.5).trunc() as u32,
        ]
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct CachedMap {
    published: Vec3,
    pending: Vec3,
    phase: u32,
    buffer: usize,
}

/// Persistent three-map refresh state, owned alongside the cached GPU images.
#[derive(Clone, Debug)]
pub struct WorldEnvironmentShadowState {
    cache_id: u64,
    quality: WorldShadowQuality,
    maps: [CachedMap; 3],
}

impl WorldEnvironmentShadowState {
    pub(crate) const fn cache_id(&self) -> u64 {
        self.cache_id
    }
    /// Returns the quality whose texture and publication policy this owner follows.
    #[must_use]
    pub const fn quality(&self) -> WorldShadowQuality {
        self.quality
    }

    /// Starts with 875760's empty maps and zero published centers.
    #[must_use]
    pub fn new(quality: WorldShadowQuality) -> Self {
        static NEXT_CACHE: AtomicU64 = AtomicU64::new(1);
        Self {
            cache_id: NEXT_CACHE.fetch_add(1, Ordering::Relaxed),
            quality,
            maps: [CachedMap {
                published: Vec3::ZERO,
                pending: Vec3::ZERO,
                phase: 0,
                buffer: 0,
            }; 3],
        }
    }

    /// Returns the complete receiver map's center and texture index.
    #[must_use]
    pub fn published_maps(&self) -> [(Vec3, usize); 3] {
        self.maps.map(|map| (map.published, map.buffer))
    }

    /// Advances exactly once for a submitted world frame.
    ///
    /// Cached qualities publish only after all nine or twenty-five regions
    /// have been rendered. The caller must submit every returned region before
    /// using `published_maps`; discarded frames must not advance this state.
    ///
    /// # Errors
    /// Rejects nonfinite world centers without changing the refresh state.
    pub fn advance(
        &mut self,
        center: Vec3,
    ) -> Result<[Option<WorldEnvironmentShadowUpdate>; 3], WorldShadowProjectionError> {
        if !center.is_finite() {
            return Err(WorldShadowProjectionError::NonFinite);
        }
        let mut updates = [None; 3];
        let Some(size) = self.quality.texture_size() else {
            return Ok(updates);
        };
        if self.quality.shader_mode() < 2 {
            return Ok(updates);
        }
        for (index, map) in self.maps.iter_mut().enumerate() {
            let radius = [40., 160., 640.][index];
            if self.quality == WorldShadowQuality::Cascaded {
                let grid = [2., 4., 16.][index];
                map.pending = (center / grid).floor() * grid;
                map.published = map.pending;
                updates[index] = Some(WorldEnvironmentShadowUpdate {
                    center: map.pending,
                    radius,
                    crop: [-radius, radius, -radius, radius],
                    viewport: [0., 1., 0., 1.],
                    buffer: 0,
                    caster_mask: 0xd,
                });
                continue;
            }
            let distance = map.published.as_dvec3() - center.as_dvec3();
            if map.phase == 0 && distance.length_squared() <= [4., 16., 1024.][index] {
                continue;
            }
            if map.phase == 0 {
                map.pending = center;
            }
            let grid = if index < 2 { 3 } else { 5 };
            let crop = crop(radius, size, grid, map.phase);
            let scale = 0.5 / f64::from(radius);
            // x87 retains multiplication and addition until each float store.
            let viewport = [
                (f64::from(crop[0]) * scale + 0.5) as f32,
                (f64::from(crop[1]) * scale + 0.5) as f32,
                (0.5 - f64::from(crop[3]) * scale) as f32,
                (0.5 - f64::from(crop[2]) * scale) as f32,
            ];
            let buffer = 1 - map.buffer;
            updates[index] = Some(WorldEnvironmentShadowUpdate {
                center: map.pending,
                radius,
                crop,
                viewport,
                buffer,
                caster_mask: 8,
            });
            map.phase += 1;
            if map.phase == grid * grid {
                map.published = map.pending;
                map.phase = 0;
                map.buffer = buffer;
            }
        }
        Ok(updates)
    }
}

fn crop(radius: f32, size: u32, grid: u32, phase: u32) -> [f32; 4] {
    let width = f64::from(radius) * f64::from(if grid == 3 { 2.0_f32 / 3.0 } else { 0.4 });
    let lower = |cell| f64::from(cell) * width - f64::from(radius);
    let x = lower(phase % grid);
    let y = lower(phase / grid);
    // Original rounds only the upper bounds outward. The last cell can extend
    // beyond the map by a texel; clamping it would change the native projection.
    let upper = |lower: f64| {
        let edge = (lower + width) as f32;
        ((f64::from(size) / (f64::from(radius) * 2.) * f64::from(edge)).ceil() / f64::from(size)
            * f64::from(radius)
            * 2.) as f32
    };
    [x as f32, upper(x), y as f32, upper(y)]
}

#[cfg(test)]
#[path = "environment_tests.rs"]
mod tests;
