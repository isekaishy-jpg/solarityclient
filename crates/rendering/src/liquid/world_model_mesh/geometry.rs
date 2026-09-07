//! Native regular grid generation and per-cell clipping admission.

use solarity_asset::{DecodedWorldModel, DecodedWorldModelGroup, WorldModelLiquid};

use super::clipping::{ClipVertex, Polygon};
use super::plan::{
    WorldModelLiquidDepthColumn, WorldModelLiquidMeshError, WorldModelLiquidSurface,
};
use crate::liquid::LiquidRenderVertex;

const GRID_STEP: f32 = 4.166_666_5;
const SURFACE_SCALE: f32 = 0.240_000_01;

/// Prepares both native factory paths, leaving instance placement to the GPU.
pub(super) fn prepare(
    model: &DecodedWorldModel,
    group: &DecodedWorldModelGroup,
    liquid: &WorldModelLiquid,
    surface: WorldModelLiquidSurface,
) -> Result<(Vec<LiquidRenderVertex>, Vec<u16>), WorldModelLiquidMeshError> {
    if liquid.vertices().len() > usize::from(u16::MAX) + 1 {
        return Err(WorldModelLiquidMeshError::VertexCapacity);
    }
    let width = liquid.vertex_width() as usize;
    let corner = liquid.corner();
    let mut vertices = Vec::with_capacity(liquid.vertices().len());
    let mut y = corner[1];
    for row in liquid.vertices().chunks_exact(width) {
        let mut x = corner[0];
        for vertex in row {
            vertices.push(render_vertex(
                ClipVertex {
                    position: [x, y, vertex.height()],
                    attributes: [vertex.texture_s() as u16, vertex.texture_t() as u16],
                },
                corner,
                surface,
            ));
            // 7A7CC0 spills each increment; clipped cells below use a multiply.
            x += GRID_STEP;
        }
        y += GRID_STEP;
    }
    let mut indices = regular_indices(liquid);
    for (index, &tile) in liquid.tiles().iter().enumerate() {
        if tile & 0x0f == 0x0f || tile & 0x80 == 0 {
            continue;
        }
        let x = index % liquid.tile_width() as usize;
        let y = index / liquid.tile_width() as usize;
        let cell = [(x, y), (x, y + 1), (x + 1, y + 1), (x + 1, y)].map(|(x, y)| {
            let vertex = liquid.vertices()[y * width + x];
            ClipVertex {
                position: [
                    grid_position(x, corner[0]),
                    grid_position(y, corner[1]),
                    vertex.height(),
                ],
                attributes: [vertex.texture_s() as u16, vertex.texture_t() as u16],
            }
        });
        let mut polygon = Polygon::new(cell);
        let start = usize::from(group.portal_reference_start());
        let end = start + usize::from(group.portal_reference_count());
        for reference in &model.portal_references()[start..end] {
            let neighbor = &model.groups()[usize::from(reference.group_index())];
            // Only a decoded MLIQ supplies defined neighbor liquid bounds.
            let Some(adjacent) = neighbor.liquid() else {
                continue;
            };
            let origin = adjacent.corner();
            if origin[0] >= cell[2].position[0]
                || origin[1] >= cell[2].position[1]
                || grid_position(adjacent.tile_width() as usize, origin[0]) < cell[0].position[0]
                || grid_position(adjacent.tile_height() as usize, origin[1]) < cell[0].position[1]
            {
                continue;
            }
            let portal = model.portals()[usize::from(reference.portal_index())];
            let [nx, ny, nz] = portal.normal();
            polygon.clip(
                [nx, ny, nz, portal.distance()],
                reference.side(),
                matches!(surface, WorldModelLiquidSurface::Magma { .. }),
            );
        }
        let strip = polygon.strip();
        if strip.is_empty() {
            continue;
        }
        if vertices.len() + strip.len() > usize::from(u16::MAX) + 1 {
            return Err(WorldModelLiquidMeshError::VertexCapacity);
        }
        indices.push(vertices.len() as u16);
        for vertex in strip {
            indices.push(vertices.len() as u16);
            vertices.push(render_vertex(vertex, corner, surface));
        }
        indices.push((vertices.len() - 1) as u16);
    }
    Ok((vertices, indices))
}

/// Native 7A7F60 computes cell coordinates in extended precision, then spills.
fn grid_position(index: usize, origin: f32) -> f32 {
    (index as f64 * f64::from(GRID_STEP) + f64::from(origin)) as f32
}

/// Native 7A7B00 selects the attribute interpretation from the factory format.
fn render_vertex(
    vertex: ClipVertex,
    corner: [f32; 3],
    surface: WorldModelLiquidSurface,
) -> LiquidRenderVertex {
    let (color, depth, uv) = match surface {
        WorldModelLiquidSurface::Water {
            depth,
            depth_column,
            color,
        } => (
            color,
            [
                match depth_column {
                    WorldModelLiquidDepthColumn::Exterior => 0.0,
                    WorldModelLiquidDepthColumn::Interior => 1.0,
                },
                depth.map_or(0.0, |bank| bank.coordinate(vertex.attributes[0] as u8)),
            ],
            [
                (vertex.position[0] - corner[0]) * SURFACE_SCALE,
                (vertex.position[1] - corner[1]) * SURFACE_SCALE,
            ],
        ),
        WorldModelLiquidSurface::Magma { color } => (
            color,
            [0.0; 2],
            vertex
                .attributes
                .map(|value| f32::from(value as i16) / 256.0),
        ),
    };
    LiquidRenderVertex::new(vertex.position, [0.0, 0.0, 1.0], color, depth, uv)
}

/// Native 7A7920 emits a degenerate pair around every contiguous row segment.
fn regular_indices(liquid: &WorldModelLiquid) -> Vec<u16> {
    let width = liquid.tile_width() as usize;
    let stride = liquid.vertex_width() as usize;
    let mut indices = Vec::new();
    if width == 0 {
        return indices;
    }
    for (y, row) in liquid.tiles().chunks_exact(width).enumerate() {
        let mut last = None;
        for (x, &tile) in row.iter().enumerate() {
            if tile & 0x0f != 0x0f && tile & 0x80 == 0 {
                let top = (y * stride + x) as u16;
                let bottom = ((y + 1) * stride + x) as u16;
                if last.is_none() {
                    indices.extend([top, top, bottom]);
                }
                indices.extend([top + 1, bottom + 1]);
                last = Some(bottom + 1);
            } else if let Some(index) = last.take() {
                indices.push(index);
            }
        }
        if let Some(index) = last {
            indices.push(index);
        }
    }
    indices
}
