//! Native WMO liquid-volume admission (`7A09D0`, `7AEB90`, `7C8360`).

use glam::Vec3;
use solarity_asset::{DecodedWorldModelGroup, LiquidTypeCatalog};
use thiserror::Error;

use super::query::{SubmergedLiquid, bilinear_height};
use crate::collision::{
    PlacedWorldModelCollision, WorldModelCollisionError, movement_collection::transform_point,
};

/// Invalid scene input or a missing required liquid behavior definition.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SubmergedLiquidError {
    /// A world point or selected WMO group is invalid.
    #[error(transparent)]
    WorldModel(#[from] WorldModelCollisionError),
    /// Native 7C8360 dereferences this LiquidType row after tile admission.
    #[error("world-model liquid type {id} is missing")]
    MissingLiquidType {
        /// Authored group liquid identifier.
        id: u32,
    },
}

impl PlacedWorldModelCollision {
    /// Visits admitted MOGI volumes in root order, returning the first wet group.
    /// The owning scene must apply the placed root's exclusion flag and order.
    ///
    /// # Errors
    /// Rejects non-finite points and missing admitted LiquidType rows.
    pub fn submerged_liquid(
        &self,
        point: Vec3,
        liquids: &LiquidTypeCatalog,
    ) -> Result<Option<SubmergedLiquid>, SubmergedLiquidError> {
        if !point.is_finite() {
            return Err(WorldModelCollisionError::NonFiniteSegment.into());
        }
        if !contains(self.root_bounds, point) {
            return Ok(None);
        }
        let local = transform_point(self.inverse_transform, point);
        if !contains(self.model.bounds().map(Vec3::from_array), local) {
            return Ok(None);
        }
        for (index, info) in self.model.group_info().iter().enumerate() {
            if info.flags() & 0x2000 != 0 || !contains(info.bounds().map(Vec3::from_array), local) {
                continue;
            }
            if let Some(mut sample) = group_liquid(
                &self.model.groups()[index],
                self.model.flags(),
                local,
                liquids,
            )? {
                // 4C2300 mutates its input point as well as its output. 7A0AE5
                // therefore republishes the transformed surface's world Z.
                sample.surface_height = transform_point(
                    self.transform,
                    Vec3::new(local.x, local.y, sample.surface_height),
                )
                .z;
                sample.depth = sample.surface_height - point.z;
                return Ok(Some(sample));
            }
        }
        Ok(None)
    }

    /// Queries only the camera's registered group, without general volume gates.
    /// Both returned height and the camera depth subtraction use WMO-local Z
    /// in 790920; registration already chose the owning group.
    ///
    /// # Errors
    /// Rejects invalid points/groups and missing admitted LiquidType rows.
    pub fn registered_submerged_liquid(
        &self,
        group: usize,
        point: Vec3,
        liquids: &LiquidTypeCatalog,
    ) -> Result<Option<SubmergedLiquid>, SubmergedLiquidError> {
        if !point.is_finite() {
            return Err(WorldModelCollisionError::NonFiniteSegment.into());
        }
        let group = self
            .model
            .groups()
            .get(group)
            .ok_or(WorldModelCollisionError::InvalidGroup { group_index: group })?;
        group_liquid(
            group,
            self.model.flags(),
            transform_point(self.inverse_transform, point),
            liquids,
        )
    }

    /// Queries the unit's registered group and transforms the surface to world Z
    /// as 7A1A30 does before publishing the spatial record consumed by 77F1E0.
    ///
    /// # Errors
    /// Rejects invalid points/groups and missing admitted LiquidType rows.
    pub fn registered_unit_liquid(
        &self,
        group: usize,
        point: Vec3,
        liquids: &LiquidTypeCatalog,
    ) -> Result<Option<SubmergedLiquid>, SubmergedLiquidError> {
        let Some(mut sample) = self.registered_submerged_liquid(group, point, liquids)? else {
            return Ok(None);
        };
        let local = transform_point(self.inverse_transform, point);
        sample.surface_height = transform_point(
            self.transform,
            Vec3::new(local.x, local.y, sample.surface_height),
        )
        .z;
        sample.depth = sample.surface_height - point.z;
        Ok(Some(sample))
    }
}

/// Inclusive native 75B5B0 point-box test; no expanded render tolerance.
fn contains(bounds: [Vec3; 2], point: Vec3) -> bool {
    (0..3).all(|axis| point[axis] >= bounds[0][axis] && point[axis] <= bounds[1][axis])
}

/// Samples the authored quad with native float stores and liquid-specific epsilon.
fn group_liquid(
    group: &DecodedWorldModelGroup,
    root_flags: u16,
    point: Vec3,
    liquids: &LiquidTypeCatalog,
) -> Result<Option<SubmergedLiquid>, SubmergedLiquidError> {
    let liquid_type = group.resolve_liquid_type(root_flags);
    if liquid_type == 0 {
        return Ok(None);
    }
    let Some(grid) = group
        .liquid()
        .filter(|_| group.flags() & 0x1000 != 0)
        .filter(|grid| grid.tile_width() != 0 && grid.tile_height() != 0)
    else {
        // 7C84E6 has no point-height comparison for an implicit liquid volume.
        return Ok(Some(SubmergedLiquid {
            liquid_type,
            surface_height: f32::MAX,
            depth: f32::MAX - point.z,
        }));
    };
    let scale = f64::from(f32::from_bits(0x3e75c290));
    let x = (f64::from(point.x) - f64::from(grid.corner()[0])) * scale;
    let y = ((f64::from(point.y) - f64::from(grid.corner()[1])) * scale) as f32;
    // X reaches floor before its stored f32 rounding; Y reaches floor after it.
    let column = x.floor();
    let row = f64::from(y).floor();
    if column < 0.0
        || row < 0.0
        || column >= f64::from(grid.tile_width())
        || row >= f64::from(grid.tile_height())
    {
        return Ok(None);
    }
    let column = column as usize;
    let row = row as usize;
    if grid.tiles()[row * grid.tile_width() as usize + column] & 0x0f == 0x0f {
        return Ok(None);
    }
    let definition = liquids
        .entry(liquid_type)
        .ok_or(SubmergedLiquidError::MissingLiquidType { id: liquid_type })?;
    let stride = grid.vertex_width() as usize;
    let first = row * stride + column;
    let heights = [first, first + 1, first + stride, first + stride + 1]
        .map(|index| grid.vertices()[index].height());
    let fractions = [(x as f32 - column as f32), (y - row as f32)];
    let height = bilinear_height(heights, fractions);
    let epsilon = if definition.flags() & 4 != 0 {
        f64::from(0.01_f32)
    } else {
        0.0
    };
    Ok(
        (f64::from(point.z) < height + epsilon).then_some(SubmergedLiquid {
            liquid_type,
            surface_height: height as f32,
            depth: height as f32 - point.z,
        }),
    )
}
