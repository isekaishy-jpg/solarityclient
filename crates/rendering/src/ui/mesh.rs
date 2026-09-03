//! Allocation-conscious conversion from ordered UI quads to indexed mesh runs.

use super::{UiMeshPlanError, UiRenderBatch, UiRenderQuad, UiRenderTransform, UiRenderVertex};
use std::sync::atomic::{AtomicU64, Ordering};

/// Upload-ready UI geometry with adjacent compatible quads already batched.
#[derive(Clone, Debug, PartialEq)]
pub struct UiMeshPlan {
    identity: u64,
    logical_extent: [f32; 2],
    vertices: Vec<UiRenderVertex>,
    indices: Vec<u32>,
    vertex_bytes: Vec<u8>,
    index_bytes: Vec<u8>,
    batches: Vec<UiRenderBatch>,
    object_indices: Vec<usize>,
}

impl UiMeshPlan {
    /// Converts exact-size ordered quads into one shared vertex/index allocation.
    ///
    /// Positions remain in stock's bottom-left logical coordinate system. The
    /// Vulkan UI pipeline owns the single canvas-to-clip-space transform.
    /// Adjacent quads are merged only when every material and residency field
    /// agrees, preserving the caller's established presentation order.
    ///
    /// # Errors
    ///
    /// Returns [`UiMeshPlanError`] for invalid extents, non-finite vertex data,
    /// inverted bounds, or a mesh exceeding unsigned 32-bit draw offsets.
    pub fn prepare<I>(logical_extent: [f32; 2], quads: I) -> Result<Self, UiMeshPlanError>
    where
        I: ExactSizeIterator<Item = UiRenderQuad>,
    {
        validate_extent(logical_extent)?;
        let quad_count = quads.len();
        let vertex_capacity = quad_count
            .checked_mul(4)
            .ok_or(UiMeshPlanError::Capacity { domain: "vertex" })?;
        let index_capacity = quad_count
            .checked_mul(6)
            .ok_or(UiMeshPlanError::Capacity { domain: "index" })?;
        u32::try_from(vertex_capacity)
            .map_err(|_source| UiMeshPlanError::Capacity { domain: "vertex" })?;
        u32::try_from(index_capacity)
            .map_err(|_source| UiMeshPlanError::Capacity { domain: "index" })?;
        u32::try_from(quad_count)
            .map_err(|_source| UiMeshPlanError::Capacity { domain: "quad" })?;
        let vertex_byte_capacity = vertex_capacity
            .checked_mul(UiRenderVertex::BYTE_SIZE)
            .ok_or(UiMeshPlanError::Capacity {
                domain: "vertex byte",
            })?;
        let index_byte_capacity =
            index_capacity
                .checked_mul(size_of::<u32>())
                .ok_or(UiMeshPlanError::Capacity {
                    domain: "index byte",
                })?;
        let mut plan = Self {
            identity: next_identity(),
            logical_extent,
            vertices: Vec::with_capacity(vertex_capacity),
            indices: Vec::with_capacity(index_capacity),
            vertex_bytes: Vec::with_capacity(vertex_byte_capacity),
            index_bytes: Vec::with_capacity(index_byte_capacity),
            batches: Vec::with_capacity(quad_count),
            object_indices: Vec::with_capacity(quad_count),
        };
        for quad in quads {
            plan.push_quad(quad)?;
        }
        Ok(plan)
    }

    /// Returns the logical canvas consumed by the vertex transform.
    #[must_use]
    pub const fn logical_extent(&self) -> [f32; 2] {
        self.logical_extent
    }

    /// Returns all four-corner quads in presentation order.
    #[must_use]
    pub fn vertices(&self) -> &[UiRenderVertex] {
        &self.vertices
    }

    /// Returns six triangle-list indices per source quad.
    #[must_use]
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    /// Returns maximal adjacent material runs in presentation order.
    #[must_use]
    pub fn batches(&self) -> &[UiRenderBatch] {
        &self.batches
    }

    /// Returns live object identities parallel to the source quad order.
    #[must_use]
    pub fn object_indices(&self) -> &[usize] {
        &self.object_indices
    }

    /// Returns the process-local identity preserved by clones of this generation.
    pub(crate) const fn identity(&self) -> u64 {
        self.identity
    }

    /// Returns the immutable vertex/index generation shared by cheap draw-state clones.
    #[must_use]
    pub const fn geometry_identity(&self) -> u64 {
        self.identity
    }

    /// Changes one retained draw translation without touching serialized mesh bytes.
    pub fn set_transform_translation(
        &mut self,
        transform: UiRenderTransform,
        translation: [f32; 2],
    ) {
        for batch in &mut self.batches {
            if batch.transform() == Some(transform) {
                batch.set_translation(translation);
            }
        }
    }

    /// Moves one retained draw slot relative to its current presentation state.
    pub fn translate_transform(&mut self, transform: UiRenderTransform, delta: [f32; 2]) {
        for batch in &mut self.batches {
            if batch.transform() == Some(transform) {
                let current = batch.translation();
                batch.set_translation([current[0] + delta[0], current[1] + delta[1]]);
            }
        }
    }

    /// Serializes vertices without relying on Rust layout or unsafe casts.
    #[must_use]
    pub fn vertex_bytes(&self) -> &[u8] {
        &self.vertex_bytes
    }

    /// Serializes direct unsigned 32-bit indices for Vulkan upload.
    #[must_use]
    pub fn index_bytes(&self) -> &[u8] {
        &self.index_bytes
    }

    /// Validates and appends one counter-clockwise two-triangle quad.
    fn push_quad(&mut self, quad: UiRenderQuad) -> Result<(), UiMeshPlanError> {
        validate_quad(&quad)?;
        let base_vertex = u32::try_from(self.vertices.len())
            .map_err(|_source| UiMeshPlanError::Capacity { domain: "vertex" })?;
        let first_index = u32::try_from(self.indices.len())
            .map_err(|_source| UiMeshPlanError::Capacity { domain: "index" })?;
        let first_quad = u32::try_from(self.object_indices.len())
            .map_err(|_source| UiMeshPlanError::Capacity { domain: "quad" })?;
        let [left, bottom, right, top] = quad.bounds();
        let positions = [[left, top], [left, bottom], [right, top], [right, bottom]];
        let texture_coordinates = quad.texture_coordinates();
        let colors = quad.colors();
        for corner in 0..4 {
            let vertex = UiRenderVertex::new(
                positions[corner],
                texture_coordinates[corner],
                colors[corner],
            );
            vertex.append_bytes(&mut self.vertex_bytes);
            self.vertices.push(vertex);
        }
        let indices = [
            base_vertex,
            base_vertex + 1,
            base_vertex + 2,
            base_vertex + 2,
            base_vertex + 1,
            base_vertex + 3,
        ];
        for index in indices {
            self.index_bytes.extend_from_slice(&index.to_le_bytes());
        }
        self.indices.extend_from_slice(&indices);
        if let Some(batch) = self.batches.last_mut()
            && batch.can_append(&quad)
        {
            batch.append_quad();
        } else {
            self.batches
                .push(UiRenderBatch::from_quad(&quad, first_index, first_quad));
        }
        self.object_indices.push(quad.object_index());
        Ok(())
    }
}

/// Assigns nonzero process-local identities to immutable presentation generations.
fn next_identity() -> u64 {
    static NEXT_IDENTITY: AtomicU64 = AtomicU64::new(1);
    NEXT_IDENTITY.fetch_add(1, Ordering::Relaxed)
}

/// Rejects canvases that cannot define the renderer's clip transform.
fn validate_extent(extent: [f32; 2]) -> Result<(), UiMeshPlanError> {
    if extent
        .into_iter()
        .all(|value| value.is_finite() && value > 0.0)
    {
        return Ok(());
    }
    Err(UiMeshPlanError::InvalidExtent {
        width: extent[0],
        height: extent[1],
    })
}

/// Checks all caller-supplied floats before they enter persistent GPU data.
fn validate_quad(quad: &UiRenderQuad) -> Result<(), UiMeshPlanError> {
    let bounds = quad.bounds();
    validate_components(quad.object_index(), "bounds", &bounds)?;
    validate_components(quad.object_index(), "translation", &quad.translation())?;
    if let Some(clip) = quad.clip() {
        validate_components(quad.object_index(), "clip", &clip)?;
        if clip[2] < clip[0] || clip[3] < clip[1] {
            return Err(UiMeshPlanError::InvertedBounds {
                object_index: quad.object_index(),
            });
        }
    }
    if bounds[2] < bounds[0] || bounds[3] < bounds[1] {
        return Err(UiMeshPlanError::InvertedBounds {
            object_index: quad.object_index(),
        });
    }
    let texture_coordinates = quad.texture_coordinates();
    for (corner, values) in texture_coordinates.iter().enumerate() {
        validate_components(quad.object_index(), "texture coordinate", values)
            .map_err(|error| offset_component(error, corner * 2))?;
    }
    let colors = quad.colors();
    for (corner, values) in colors.iter().enumerate() {
        validate_components(quad.object_index(), "color", values)
            .map_err(|error| offset_component(error, corner * 4))?;
    }
    Ok(())
}

/// Identifies the first non-finite component in one fixed-size field segment.
fn validate_components(
    object_index: usize,
    field: &'static str,
    values: &[f32],
) -> Result<(), UiMeshPlanError> {
    let Some(component) = values.iter().position(|value| !value.is_finite()) else {
        return Ok(());
    };
    Err(UiMeshPlanError::NonFiniteComponent {
        object_index,
        field,
        component,
    })
}

/// Converts a corner-local component index into its flattened field index.
fn offset_component(error: UiMeshPlanError, offset: usize) -> UiMeshPlanError {
    match error {
        UiMeshPlanError::NonFiniteComponent {
            object_index,
            field,
            component,
        } => UiMeshPlanError::NonFiniteComponent {
            object_index,
            field,
            component: component + offset,
        },
        other => other,
    }
}
