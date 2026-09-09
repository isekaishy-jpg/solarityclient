//! Native horizon projection (`791170`) and immutable map-wide mesh ownership.

use std::sync::Arc;

use glam::Vec3;
use solarity_asset::TerrainLowDetail;

use super::TerrainLowDetailMesh;
use crate::{WorldCamera, WorldCameraError, WorldCameraFrame, WorldFrustum, WorldScreenWindow};

/// All authored WDL tiles, shared by terrain generations and fenced GPU readers.
#[derive(Debug, PartialEq)]
pub struct TerrainLowDetailMap {
    tiles: Vec<TerrainLowDetailMesh>,
}

impl TerrainLowDetailMap {
    /// Builds the map once; changing the resident ADT does not rebuild this bank.
    #[must_use]
    pub fn new(map: &TerrainLowDetail) -> Self {
        Self {
            tiles: map.tiles().iter().map(TerrainLowDetailMesh::new).collect(),
        }
    }

    /// Returns the immutable tile bank in native MAOF order.
    pub fn tiles(&self) -> &[TerrainLowDetailMesh] {
        &self.tiles
    }
}

/// One borrowed horizon scene with stock's separate near/far projection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldLowDetailFrame<'a> {
    map: &'a Arc<TerrainLowDetailMap>,
    camera: WorldCameraFrame,
    frustum: WorldFrustum,
    fog_color: Vec3,
}

impl<'a> WorldLowDetailFrame<'a> {
    /// Uses `791170`'s farclip-minus-50 near plane and `ADEECC`'s fixed factor one.
    ///
    /// # Errors
    /// Returns camera validation failures for a non-perspective source or invalid far range.
    pub fn new(
        map: &'a Arc<TerrainLowDetailMap>,
        camera: WorldCameraFrame,
        fog_color: Vec3,
    ) -> Result<Self, WorldCameraError> {
        let source = camera.camera();
        let fov = source
            .vertical_field_of_view_radians()
            .ok_or(WorldCameraError::FieldOfView)?;
        let camera = WorldCamera::new(
            source.position(),
            source.target(),
            source.up(),
            fov,
            source.far_clip() - 50.0,
            source.far_clip(),
        )
        .frame(camera.aspect_ratio())?;
        let frustum = WorldFrustum::new(camera, WorldScreenWindow::FULL)?;
        Ok(Self {
            map,
            camera,
            frustum,
            fog_color,
        })
    }

    pub(crate) const fn map(self) -> &'a Arc<TerrainLowDetailMap> {
        self.map
    }

    /// Returns the horizon camera for the fixed-function projection replacement.
    pub const fn camera(self) -> WorldCameraFrame {
        self.camera
    }

    /// Tests the native tile bounds before submitting its two face banks.
    ///
    /// # Errors
    /// Returns malformed bounds errors from the common world frustum.
    pub fn is_visible(self, tile: &TerrainLowDetailMesh) -> Result<bool, WorldCameraError> {
        // 7D5E70 skips an entirely marked tile, including its second bank.
        if tile.unculled_index_count() == 0 {
            return Ok(false);
        }
        let [minimum, maximum] = tile.bounds().map(Vec3::from_array);
        let half = (maximum - minimum) * 0.5;
        self.frustum.intersects_box(
            (minimum + maximum) * 0.5,
            Vec3::X * half.x,
            Vec3::Y * half.y,
            Vec3::Z * half.z,
        )
    }

    /// Packs staged view/projection and the fully fogged native fragment color.
    pub(crate) fn push_bytes(self) -> [u8; 96] {
        let projection = self.camera.projection();
        let coefficients = [
            projection.x_axis.x,
            projection.y_axis.y,
            projection.z_axis.z,
            projection.w_axis.z,
        ];
        let mut bytes = [0; 96];
        for (slot, value) in bytes.as_chunks_mut::<4>().0.iter_mut().zip(
            self.camera
                .view()
                .to_cols_array()
                .into_iter()
                .chain(coefficients)
                .chain(self.fog_color.extend(1.0).to_array()),
        ) {
            slot.copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }
}
