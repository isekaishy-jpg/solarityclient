//! Native world-to-minimap projection and masked terrain tile geometry.

use solarity_asset::{AssetPath, TerrainMap, TerrainTileIndex};
use thiserror::Error;

use crate::{
    UiRenderBlend, UiRenderMask, UiRenderQuad, UiRenderSource, UiTextureAddressMode,
    UiTextureResidency,
};

const TILE_SIZE: f32 = 533.333_3;
const MAP_ORIGIN: f32 = 32.0 * TILE_SIZE;
const IMAGE_COORDINATES: [[f32; 2]; 4] = [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]];

/// One native minimap viewport centered on a world position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MinimapView {
    world_center: [f32; 2],
    bounds: [f32; 4],
    screen_center: [f32; 2],
    scale: [f32; 2],
    heading: f32,
    sin: f32,
    cos: f32,
}

impl MinimapView {
    /// Creates a viewport from server X/Y, radius in yards, map heading, and
    /// left/bottom/right/top UI edges. Heading zero keeps north (+X) up and
    /// west (+Y) left; a positive heading rotates the map clockwise beneath
    /// its fixed mask, matching FUN_00581E80.
    ///
    /// # Errors
    /// Returns [`MinimapViewError`] for non-finite coordinates or heading,
    /// non-positive radius/extent, or a scale outside finite float range.
    pub fn new(
        world_center: [f32; 2],
        radius: f32,
        heading: f32,
        bounds: [f32; 4],
    ) -> Result<Self, MinimapViewError> {
        if !world_center.into_iter().all(f32::is_finite) || !heading.is_finite() {
            return Err(MinimapViewError::InvalidPosition);
        }
        if !radius.is_finite() || radius <= 0.0 {
            return Err(MinimapViewError::InvalidRadius);
        }
        if !bounds.into_iter().all(f32::is_finite)
            || bounds[0] >= bounds[2]
            || bounds[1] >= bounds[3]
        {
            return Err(MinimapViewError::InvalidBounds);
        }
        let half_extent = [(bounds[2] - bounds[0]) * 0.5, (bounds[3] - bounds[1]) * 0.5];
        let scale = half_extent.map(|size| size / radius);
        let screen_center = [bounds[0] + half_extent[0], bounds[1] + half_extent[1]];
        if !scale
            .into_iter()
            .all(|value| value.is_finite() && value > 0.0)
            || !screen_center.into_iter().all(f32::is_finite)
        {
            return Err(MinimapViewError::InvalidBounds);
        }
        let (sin, cos) = heading.sin_cos();
        Ok(Self {
            world_center,
            bounds,
            screen_center,
            scale,
            heading,
            sin,
            cos,
        })
    }

    /// Projects a world X/Y point to the bottom-left UI coordinate system.
    #[must_use]
    pub fn project(&self, world: [f32; 2]) -> [f32; 2] {
        let north = world[0] - self.world_center[0];
        let east = self.world_center[1] - world[1];
        [
            self.screen_center[0] + (east * self.cos + north * self.sin) * self.scale[0],
            self.screen_center[1] + (north * self.cos - east * self.sin) * self.scale[1],
        ]
    }

    /// Selects stock's two-by-two outdoor neighborhood, independent of zoom.
    /// FUN_007F5930 uses half a terrain tile on each side of the player;
    /// FUN_007F5760 admits corners in (0,0), (1,0), (1,1), (0,1) order.
    #[must_use]
    pub fn terrain_tiles(&self) -> [TerrainTileIndex; 4] {
        let half_tile = TILE_SIZE * 0.5;
        let first = TerrainMap::tile_at_world_position(
            self.world_center[0] + half_tile,
            self.world_center[1] + half_tile,
        );
        let x = first.x().min(62);
        let y = first.y().min(62);
        [(0, 0), (1, 0), (1, 1), (0, 1)].map(|(dx, dy)| {
            TerrainTileIndex::clamped(i32::from(x + dx), i32::from(y + dy))
        })
    }

    /// Projects one archive-resolved terrain tile. Each tile retains its own
    /// image UVs, while all tiles share the fixed mask and viewport scissor.
    #[must_use]
    pub fn terrain_quad(
        &self,
        object_index: usize,
        tile: TerrainTileIndex,
        image: AssetPath,
        mask: AssetPath,
    ) -> UiRenderQuad {
        // Computing both edges from the integer grid keeps adjacent tiles'
        // common edge bit-identical even far from the world origin.
        let north = MAP_ORIGIN - f32::from(tile.y()) * TILE_SIZE;
        let south = MAP_ORIGIN - f32::from(tile.y() + 1) * TILE_SIZE;
        let west = MAP_ORIGIN - f32::from(tile.x()) * TILE_SIZE;
        let east = MAP_ORIGIN - f32::from(tile.x() + 1) * TILE_SIZE;
        self.map_quad(
            object_index,
            image,
            mask,
            [[north, west], [south, west], [north, east], [south, east]],
        )
    }

    /// Projects texture corners already transformed into world X/Y. This is
    /// also the composition boundary for WMO group tiles resolved by runtime.
    #[must_use]
    pub fn map_quad(
        &self,
        object_index: usize,
        image: AssetPath,
        mask: AssetPath,
        world_corners: [[f32; 2]; 4],
    ) -> UiRenderQuad {
        self.image_quad(object_index, image, IMAGE_COORDINATES)
            .with_positions(world_corners.map(|world| self.project(world)))
            .with_mask(UiRenderMask::new(mask, self.bounds))
    }

    /// Places the native player texture at the map center. Dimensions are UI
    /// units after inherited scale; facing is relative to world north. A map
    /// following that facing keeps the arrow upright.
    #[must_use]
    pub fn player_quad(
        &self,
        object_index: usize,
        image: AssetPath,
        dimensions: [f32; 2],
        facing: f32,
    ) -> UiRenderQuad {
        // CSimpleTexture rotation (FUN_00483120) changes sampling around
        // (0.5, 0.5), with unit-radius corners, inside the unchanged region.
        let (sin, cos) = (facing - self.heading - std::f32::consts::FRAC_PI_4).sin_cos();
        let coordinates = [
            [0.5 + sin, 0.5 - cos],
            [0.5 - cos, 0.5 - sin],
            [0.5 + cos, 0.5 + sin],
            [0.5 - sin, 0.5 + cos],
        ];
        let [half_width, half_height] = dimensions.map(|value| value * 0.5);
        let positions = [
            [-half_width, half_height],
            [-half_width, -half_height],
            [half_width, half_height],
            [half_width, -half_height],
        ]
        .map(|[x, y]| [self.screen_center[0] + x, self.screen_center[1] + y]);
        self.image_quad(object_index, image, coordinates)
            .with_positions(positions)
    }

    fn image_quad(
        &self,
        object_index: usize,
        image: AssetPath,
        coordinates: [[f32; 2]; 4],
    ) -> UiRenderQuad {
        UiRenderQuad::new(
            object_index,
            UiRenderSource::Texture(image),
            UiRenderBlend::Alpha,
            UiTextureAddressMode::Clamp,
            UiTextureAddressMode::Clamp,
            UiTextureResidency::NonBlocking,
            false,
            self.bounds,
            coordinates,
            [[1.0; 4]; 4],
        )
        .with_clip(self.bounds)
    }
}

/// Invalid inputs to the native minimap projection.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum MinimapViewError {
    /// World position or map heading is not finite.
    #[error("minimap world position or heading is not finite")]
    InvalidPosition,
    /// Visible radius must be finite and positive.
    #[error("minimap radius must be finite and positive")]
    InvalidRadius,
    /// The viewport cannot define a finite positive projection scale.
    #[error("minimap viewport must have finite positive dimensions and scale")]
    InvalidBounds,
}
