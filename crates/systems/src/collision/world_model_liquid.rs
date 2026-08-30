//! Point sampling over placed build-12340 WMO liquid surfaces.

use std::sync::Arc;

use glam::{Mat4, Vec2, Vec3};
use solarity_asset::{DecodedWorldModel, DecodedWorldModelGroup};
use thiserror::Error;

use super::world_model::{WorldModelCollisionError, placement_transform, transformed_bounds};

const LIQUID_TILE_SIZE: f32 = 4.166_666_5;
const SAMPLE_TOLERANCE: f32 = 0.0001;

/// Stock liquid information at one placed-WMO world-space point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldModelLiquidSample {
    height: f32,
    liquid_type: u32,
    fishable: bool,
}

impl WorldModelLiquidSample {
    /// Returns the interpolated absolute surface height.
    #[must_use]
    pub const fn height(self) -> f32 {
        self.height
    }

    /// Returns the `LiquidType.dbc` identifier.
    #[must_use]
    pub const fn liquid_type(self) -> u32 {
        self.liquid_type
    }

    /// Returns whether the sampled MLIQ tile has flag `0x40`.
    #[must_use]
    pub const fn is_fishable(self) -> bool {
        self.fishable
    }
}

/// Invalid placed-WMO liquid geometry or point-query input.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum WorldModelLiquidError {
    /// The owning MODF or game-object transform is invalid.
    #[error(transparent)]
    Placement(#[from] WorldModelCollisionError),
    /// A prepared surface or group bound is NaN or infinite.
    #[error("world-model liquid geometry is not finite")]
    NonFiniteGeometry,
    /// The query's world X or Y coordinate is NaN or infinite.
    #[error("world-model liquid query point is not finite")]
    NonFinitePoint,
    /// The optional reference height is NaN or infinite.
    #[error("world-model liquid reference height is not finite")]
    NonFiniteReferenceHeight,
}

/// One shared WMO generation's liquid surfaces under an owning transform.
pub struct PlacedWorldModelLiquid {
    model: Arc<DecodedWorldModel>,
    draws: Vec<WorldModelLiquidDraw>,
}

impl PlacedWorldModelLiquid {
    /// Builds explicit MLIQ tiles and valid WotLK implicit group planes.
    ///
    /// # Errors
    ///
    /// Returns [`WorldModelLiquidError`] when the placement or transformed
    /// authored geometry is invalid.
    pub fn prepare(
        model: Arc<DecodedWorldModel>,
        position: Vec3,
        rotation_degrees: Vec3,
        scale: f32,
    ) -> Result<Self, WorldModelLiquidError> {
        let transform = placement_transform(position, rotation_degrees, scale)?;
        let mut draws = Vec::new();
        for group in model.groups() {
            let group_bounds = transformed_bounds(group.bounds(), transform)?;
            if let Some(liquid) = group.liquid() {
                let vertex_height = usize::try_from(liquid.vertex_height())
                    .map_err(|_error| WorldModelLiquidError::NonFiniteGeometry)?;
                let tile_width = usize::try_from(liquid.tile_width())
                    .map_err(|_error| WorldModelLiquidError::NonFiniteGeometry)?;
                let tile_height = usize::try_from(liquid.tile_height())
                    .map_err(|_error| WorldModelLiquidError::NonFiniteGeometry)?;
                for x in 0..tile_width {
                    for y in 0..tile_height {
                        let tile = liquid.tiles()[x * tile_height + y];
                        if tile & 0x0f == 0x0f {
                            continue;
                        }
                        let liquid_type = group.resolve_liquid_type(model.flags(), Some(tile));
                        if liquid_type == 0 {
                            continue;
                        }
                        let vertex = |x: usize, y: usize| {
                            let source = liquid.vertices()[x * vertex_height + y];
                            transform.transform_point3(Vec3::new(
                                liquid.corner()[0] + x as f32 * LIQUID_TILE_SIZE,
                                liquid.corner()[1] + y as f32 * LIQUID_TILE_SIZE,
                                source.height(),
                            ))
                        };
                        let corners = [
                            vertex(x, y),
                            vertex(x + 1, y),
                            vertex(x + 1, y + 1),
                            vertex(x, y + 1),
                        ];
                        draws.push(WorldModelLiquidDraw::new(
                            group_bounds,
                            liquid_type,
                            tile & 0x40 != 0,
                            corners,
                        )?);
                    }
                }
            } else if model.flags() & 0x4 != 0
                && group.liquid_type() != 0
                && group.flags() & 0x1000 != 0
            {
                let liquid_type = group.resolve_liquid_type(model.flags(), None);
                if liquid_type != 0 {
                    draws.push(implicit_group_draw(
                        group,
                        group_bounds,
                        liquid_type,
                        transform,
                    )?);
                }
            }
        }
        Ok(Self { model, draws })
    }

    /// Returns the canonical shared WMO root generation.
    #[must_use]
    pub fn model(&self) -> &Arc<DecodedWorldModel> {
        &self.model
    }
}

/// Main-thread placed-WMO liquid scene.
#[derive(Default)]
pub struct WorldModelLiquidScene {
    instances: Vec<PlacedWorldModelLiquid>,
}

impl WorldModelLiquidScene {
    /// Creates an empty scene without allocating.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            instances: Vec::new(),
        }
    }

    /// Adds one already validated placed WMO generation.
    pub fn add(&mut self, placement: PlacedWorldModelLiquid) {
        self.instances.push(placement);
    }

    /// Returns the number of independently transformed WMO owners.
    #[must_use]
    pub fn instance_count(&self) -> usize {
        self.instances.len()
    }

    /// Samples the preferred authored WMO liquid surface at one world point.
    ///
    /// # Errors
    ///
    /// Returns [`WorldModelLiquidError`] for non-finite point or reference input.
    pub fn sample(
        &self,
        world_x: f32,
        world_y: f32,
        reference_height: Option<f32>,
    ) -> Result<Option<WorldModelLiquidSample>, WorldModelLiquidError> {
        if !world_x.is_finite() || !world_y.is_finite() {
            return Err(WorldModelLiquidError::NonFinitePoint);
        }
        if reference_height.is_some_and(|height| !height.is_finite()) {
            return Err(WorldModelLiquidError::NonFiniteReferenceHeight);
        }
        let point = Vec2::new(world_x, world_y);
        let mut selected: Option<WorldModelLiquidSample> = None;
        for draw in self.instances.iter().flat_map(|instance| &instance.draws) {
            if !contains_horizontal(draw.group_bounds, point)
                || reference_height
                    .is_some_and(|height| height < draw.group_bounds[0].z - SAMPLE_TOLERANCE)
            {
                continue;
            }
            for triangle in draw.triangles {
                let Some(height) = sample_triangle_height(triangle, point) else {
                    continue;
                };
                if selected
                    .is_some_and(|current| !prefer_height(height, current.height, reference_height))
                {
                    continue;
                }
                selected = Some(WorldModelLiquidSample {
                    height,
                    liquid_type: draw.liquid_type,
                    fishable: draw.fishable,
                });
            }
        }
        Ok(selected)
    }
}

struct WorldModelLiquidDraw {
    group_bounds: [Vec3; 2],
    liquid_type: u32,
    fishable: bool,
    triangles: [[Vec3; 3]; 2],
}

impl WorldModelLiquidDraw {
    fn new(
        group_bounds: [Vec3; 2],
        liquid_type: u32,
        fishable: bool,
        corners: [Vec3; 4],
    ) -> Result<Self, WorldModelLiquidError> {
        if corners.into_iter().any(|corner| !corner.is_finite()) {
            return Err(WorldModelLiquidError::NonFiniteGeometry);
        }
        Ok(Self {
            group_bounds,
            liquid_type,
            fishable,
            triangles: [
                [corners[0], corners[1], corners[2]],
                [corners[0], corners[2], corners[3]],
            ],
        })
    }
}

fn implicit_group_draw(
    group: &DecodedWorldModelGroup,
    group_bounds: [Vec3; 2],
    liquid_type: u32,
    transform: Mat4,
) -> Result<WorldModelLiquidDraw, WorldModelLiquidError> {
    let [minimum, maximum] = group.bounds().map(Vec3::from_array);
    WorldModelLiquidDraw::new(
        group_bounds,
        liquid_type,
        false,
        [
            transform.transform_point3(Vec3::new(minimum.x, minimum.y, maximum.z)),
            transform.transform_point3(Vec3::new(maximum.x, minimum.y, maximum.z)),
            transform.transform_point3(Vec3::new(maximum.x, maximum.y, maximum.z)),
            transform.transform_point3(Vec3::new(minimum.x, maximum.y, maximum.z)),
        ],
    )
}

fn contains_horizontal(bounds: [Vec3; 2], point: Vec2) -> bool {
    point.x >= bounds[0].x - SAMPLE_TOLERANCE
        && point.x <= bounds[1].x + SAMPLE_TOLERANCE
        && point.y >= bounds[0].y - SAMPLE_TOLERANCE
        && point.y <= bounds[1].y + SAMPLE_TOLERANCE
}

fn sample_triangle_height(vertices: [Vec3; 3], point: Vec2) -> Option<f32> {
    let [first, second, third] = vertices;
    let denominator =
        (second.y - third.y) * (first.x - third.x) + (third.x - second.x) * (first.y - third.y);
    if denominator.abs() < 0.000_001 {
        return None;
    }
    let first_weight = ((second.y - third.y) * (point.x - third.x)
        + (third.x - second.x) * (point.y - third.y))
        / denominator;
    let second_weight = ((third.y - first.y) * (point.x - third.x)
        + (first.x - third.x) * (point.y - third.y))
        / denominator;
    let third_weight = 1.0 - first_weight - second_weight;
    if first_weight < -SAMPLE_TOLERANCE
        || second_weight < -SAMPLE_TOLERANCE
        || third_weight < -SAMPLE_TOLERANCE
    {
        return None;
    }
    Some(first_weight * first.z + second_weight * second.z + third_weight * third.z)
}

fn prefer_height(candidate: f32, current: f32, reference: Option<f32>) -> bool {
    let Some(reference) = reference else {
        return candidate > current;
    };
    let candidate_above = candidate >= reference - SAMPLE_TOLERANCE;
    let current_above = current >= reference - SAMPLE_TOLERANCE;
    if candidate_above != current_above {
        return candidate_above;
    }
    if candidate_above {
        candidate < current
    } else {
        candidate > current
    }
}
