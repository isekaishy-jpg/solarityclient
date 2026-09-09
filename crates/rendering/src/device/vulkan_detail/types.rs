//! Texture-resolved detail chunks borrowed by one world submission.

use std::sync::Arc;

use crate::{
    BlpTextureHandle, GroundDetailMeshPlan, WorldModelBaseMip, WorldModelTextureFiltering,
};

/// A detail draw requires one texture per native batch and a valid range.
#[derive(Clone, Copy, Debug, thiserror::Error, Eq, PartialEq)]
#[error(
    "ground detail requires matching texture batches, a finite camera position, and a distance from zero through 140"
)]
pub struct GroundDetailFrameError;

/// Immutable geometry and texture identities retained across unchanged frames.
#[derive(Clone)]
pub struct GroundDetailDraw {
    plan: Arc<GroundDetailMeshPlan>,
    textures: Arc<[BlpTextureHandle]>,
    filtering: WorldModelTextureFiltering,
    base_mip: WorldModelBaseMip,
}

impl GroundDetailDraw {
    /// Joins resident textures to the plan's native texture-bucket order.
    ///
    /// # Errors
    /// Rejects a texture count that differs from the prepared batch count.
    pub fn new(
        plan: Arc<GroundDetailMeshPlan>,
        textures: Vec<BlpTextureHandle>,
        filtering: WorldModelTextureFiltering,
        base_mip: WorldModelBaseMip,
    ) -> Result<Self, GroundDetailFrameError> {
        if textures.len() != plan.batches().len() {
            return Err(GroundDetailFrameError);
        }
        Ok(Self {
            plan,
            textures: textures.into(),
            filtering,
            base_mip,
        })
    }

    /// Returns the immutable chunk generation pinned by submitted frame fences.
    #[must_use]
    pub const fn plan(&self) -> &Arc<GroundDetailMeshPlan> {
        &self.plan
    }
    pub(in crate::device) fn textures(&self) -> &[BlpTextureHandle] {
        &self.textures
    }
    pub(in crate::device) const fn filtering(&self) -> WorldModelTextureFiltering {
        self.filtering
    }
    pub(in crate::device) const fn base_mip(&self) -> WorldModelBaseMip {
        self.base_mip
    }
}

impl PartialEq for GroundDetailDraw {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.plan, &other.plan)
            && self.textures == other.textures
            && self.filtering == other.filtering
            && self.base_mip == other.base_mip
    }
}

impl std::fmt::Debug for GroundDetailDraw {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GroundDetailDraw")
            .field("origin", &self.plan.origin())
            .field("vertices", &self.plan.vertices().len())
            .field("textures", &self.textures)
            .finish_non_exhaustive()
    }
}

/// Visible detail chunks and the registered ground-effect distance for one frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundDetailFrame<'a> {
    draws: &'a [GroundDetailDraw],
    distance: f32,
    camera_position: glam::Vec3,
}

impl<'a> GroundDetailFrame<'a> {
    /// Borrows the already selected native chunk list; zero range disables draws.
    ///
    /// # Errors
    /// Rejects a nonfinite camera or values outside callback 78DB10's zero-through-140 range.
    pub fn new(
        draws: &'a [GroundDetailDraw],
        distance: f32,
        camera_position: glam::Vec3,
    ) -> Result<Self, GroundDetailFrameError> {
        if !distance.is_finite()
            || !(0.0..=140.0).contains(&distance)
            || !camera_position.is_finite()
        {
            return Err(GroundDetailFrameError);
        }
        Ok(Self {
            draws: if distance == 0.0 { &[] } else { draws },
            distance,
            camera_position,
        })
    }

    /// Returns the number of accepted texture buckets submitted by this frame.
    #[must_use]
    pub fn draw_count(self) -> usize {
        self.draws
            .iter()
            .map(|draw| draw.plan.batches().len())
            .sum()
    }
    pub(in crate::device) const fn draws(self) -> &'a [GroundDetailDraw] {
        self.draws
    }
    pub(in crate::device) const fn distance(self) -> f32 {
        self.distance
    }
    pub(in crate::device) const fn camera_position(self) -> glam::Vec3 {
        self.camera_position
    }
}
