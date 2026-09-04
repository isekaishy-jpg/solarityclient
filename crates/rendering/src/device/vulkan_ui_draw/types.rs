//! Immutable UI draw data safe to consume during command recording.

use crate::device::{UiMeshHandle, UiPipelineHandle, UiTextureSetHandle};

/// One fully validated indexed UI material batch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiPreparedDraw {
    mesh: UiMeshHandle,
    pipeline: UiPipelineHandle,
    texture_set: Option<UiTextureSetHandle>,
    first_index: u32,
    index_count: u32,
    base_vertex: i32,
    translation: [f32; 2],
    opacity: f32,
    clip: Option<[f32; 4]>,
}

impl UiPreparedDraw {
    /// Constructs a packet after every renderer-local resource join is validated.
    #[allow(clippy::too_many_arguments)]
    pub(super) const fn new(
        mesh: UiMeshHandle,
        pipeline: UiPipelineHandle,
        texture_set: Option<UiTextureSetHandle>,
        first_index: u32,
        index_count: u32,
        base_vertex: i32,
        translation: [f32; 2],
        opacity: f32,
        clip: Option<[f32; 4]>,
    ) -> Self {
        Self {
            mesh,
            pipeline,
            texture_set,
            first_index,
            index_count,
            base_vertex,
            translation,
            opacity,
            clip,
        }
    }

    /// Returns the device-local UI geometry allocation identity.
    #[must_use]
    pub const fn mesh(self) -> UiMeshHandle {
        self.mesh
    }

    /// Returns the exact source/blend pipeline identity.
    #[must_use]
    pub const fn pipeline(self) -> UiPipelineHandle {
        self.pipeline
    }

    /// Returns the sampled-image set required only by texture-backed batches.
    #[must_use]
    pub const fn texture_set(self) -> Option<UiTextureSetHandle> {
        self.texture_set
    }

    /// Returns the batch's first index in the source mesh plan.
    #[must_use]
    pub const fn first_index(self) -> u32 {
        self.first_index
    }

    /// Returns the number of sequential indices submitted by this batch.
    #[must_use]
    pub const fn index_count(self) -> u32 {
        self.index_count
    }

    /// Returns the first vertex of this batch's canonical quad-index pattern.
    #[must_use]
    pub const fn base_vertex(self) -> i32 {
        self.base_vertex
    }

    /// Returns the logical draw translation applied without modifying vertices.
    #[must_use]
    pub const fn translation(self) -> [f32; 2] {
        self.translation
    }

    /// Returns the inherited region opacity applied to vertex alpha.
    #[must_use]
    pub const fn opacity(self) -> f32 {
        self.opacity
    }

    /// Returns an optional bottom-left-origin logical clip rectangle.
    #[must_use]
    pub const fn clip(self) -> Option<[f32; 4]> {
        self.clip
    }

    /// Rebinds only push-constant/scissor state for retained immutable geometry.
    pub fn set_transform_state(
        &mut self,
        translation: [f32; 2],
        opacity: f32,
        clip: Option<[f32; 4]>,
    ) {
        self.translation = translation;
        self.opacity = opacity;
        self.clip = clip;
    }
}
