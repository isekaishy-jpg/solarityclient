//! Native scene point-light publication and bounded per-model selection.

use super::M2PointLight;
use glam::{Mat4, Vec3};
use thiserror::Error;

const EMPTY: usize = usize::MAX;
const GRID_WIDTH: usize = 64;
const GRID_SCALE: f64 = 0.05_f32 as f64;

/// Invalid input at the resident scene-light boundary.
#[derive(Clone, Copy, Debug, Error, PartialEq)]
pub enum ScenePointLightError {
    /// A sampled source cannot be assigned a spatial cell.
    #[error("scene point-light position is nonfinite")]
    NonFinitePosition,
    /// A queried model must supply a finite center and nonnegative radius.
    #[error("scene point-light query has invalid bounds")]
    InvalidBounds,
}

/// Four nearest candidate IDs, in original 834F60 insertion order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScenePointLightSelection {
    indices: [Option<usize>; 4],
    squared_distances: [f32; 4],
}

impl ScenePointLightSelection {
    /// Returns IDs from the current frame's publication bank, nearest first.
    #[must_use]
    pub const fn indices(self) -> [Option<usize>; 4] {
        self.indices
    }
    /// Returns stored native squared distances for the selected IDs.
    #[must_use]
    pub const fn squared_distances(self) -> [f32; 4] {
        self.squared_distances
    }
}

/// Allocation-reusing 64-by-64 native light grid with 20-unit cells.
///
/// Publication inserts at each cell's head (834C70). Queries visit X then Y,
/// wrap both coordinates, and apply 834F60's four-candidate insertion rules.
/// A fresh frame publishes only sources updated by the current scene tick.
pub struct ScenePointLights {
    heads: Box<[usize; GRID_WIDTH * GRID_WIDTH]>,
    next: Vec<usize>,
    points: Vec<M2PointLight>,
}

impl Default for ScenePointLights {
    fn default() -> Self {
        Self {
            heads: Box::new([EMPTY; GRID_WIDTH * GRID_WIDTH]),
            next: Vec::new(),
            points: Vec::new(),
        }
    }
}

impl ScenePointLights {
    /// Retires last frame's publication without releasing reusable storage.
    pub fn clear(&mut self) {
        self.heads.fill(EMPTY);
        self.next.clear();
        self.points.clear();
    }

    /// Publishes one current animated source in native update order.
    ///
    /// # Errors
    /// Rejects a nonfinite source position.
    pub fn publish(&mut self, light: M2PointLight) -> Result<usize, ScenePointLightError> {
        let position = light.position();
        if !position.is_finite() {
            return Err(ScenePointLightError::NonFinitePosition);
        }
        let cell = cell(f64::from(position.y) * GRID_SCALE) * GRID_WIDTH
            + cell(f64::from(position.x) * GRID_SCALE);
        let index = self.points.len();
        self.points.push(light);
        self.next.push(self.heads[cell]);
        self.heads[cell] = index;
        Ok(index)
    }

    /// Returns current sources in publication order.
    #[must_use]
    pub fn points(&self) -> &[M2PointLight] {
        &self.points
    }

    /// Selects and uploads native 8A38B0's first three water point lights.
    /// M2 float diffuse colors also cross its byte-normalization boundary.
    ///
    /// # Errors
    /// Returns the same spatial validation failures as [`Self::query`].
    pub fn liquid_lighting(
        &self,
        center: Vec3,
        radius: f32,
        view: Mat4,
        lighting: crate::LiquidLighting,
    ) -> Result<crate::LiquidLighting, ScenePointLightError> {
        let selected = self.query(center, radius)?;
        let mut points = [crate::LiquidPointLight::new(Vec3::ZERO, Vec3::ZERO, Vec3::ZERO); 3];
        let mut count = 0;
        for index in selected.indices().into_iter().flatten().take(3) {
            let point = self.points[index];
            points[count] = crate::LiquidPointLight::new(
                view.transform_point3(point.position()),
                point.diffuse() * (1.0_f32 / 255.0),
                Vec3::new(0.0, 0.7, 0.03),
            );
            count += 1;
        }
        Ok(lighting.with_point_lights(&points[..count]))
    }

    /// Selects the native spatial candidates for one model's lighting center.
    ///
    /// CM2 queries have radius zero; procedural water can supply a batch radius.
    /// The spatial cell rectangle is the admission boundary, including aliases
    /// after 1,280 units. There is no additional spherical range cutoff.
    ///
    /// # Errors
    /// Rejects nonfinite bounds and negative radii.
    pub fn query(
        &self,
        center: Vec3,
        radius: f32,
    ) -> Result<ScenePointLightSelection, ScenePointLightError> {
        if !center.is_finite() || !radius.is_finite() || radius < 0. {
            return Err(ScenePointLightError::InvalidBounds);
        }
        let center = center.as_dvec3();
        let radius = f64::from(radius);
        let first_x = cell((center.x - radius) * GRID_SCALE - 0.5);
        let last_x = cell((center.x + radius) * GRID_SCALE + 0.5);
        let first_y = cell((center.y - radius) * GRID_SCALE - 0.5);
        let last_y = cell((center.y + radius) * GRID_SCALE + 0.5);
        let mut result = ScenePointLightSelection {
            indices: [None; 4],
            squared_distances: [0.; 4],
        };
        let mut count = 0;
        let mut x = first_x;
        loop {
            let mut y = first_y;
            loop {
                let mut index = self.heads[y * GRID_WIDTH + x];
                while index != EMPTY {
                    let delta = self.points[index].position().as_dvec3() - center;
                    let distance = (delta.z * delta.z + delta.y * delta.y) + delta.x * delta.x;
                    if count < 4 || distance < f64::from(result.squared_distances[3]) {
                        let mut slot = count.min(3);
                        while slot > 0 && f64::from(result.squared_distances[slot - 1]) >= distance
                        {
                            result.indices[slot] = result.indices[slot - 1];
                            result.squared_distances[slot] = result.squared_distances[slot - 1];
                            slot -= 1;
                        }
                        result.indices[slot] = Some(index);
                        result.squared_distances[slot] = distance as f32;
                        count = (count + 1).min(4);
                    }
                    index = self.next[index];
                }
                if y == last_y {
                    break;
                }
                y = (y + 1) & 63;
            }
            if x == last_x {
                break;
            }
            x = (x + 1) & 63;
        }
        Ok(result)
    }
}

fn cell(value: f64) -> usize {
    (value.floor() as i64 & 63) as usize
}

#[cfg(test)]
#[path = "../../tests/stock_seed/scene_point_lights_native.rs"]
mod tests;
