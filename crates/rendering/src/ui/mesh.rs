//! Allocation-conscious conversion from ordered UI quads to indexed mesh runs.

use super::UiRenderState;
use super::{UiMeshPlanError, UiRenderBatch, UiRenderQuad, UiRenderTransform, UiRenderVertex};
use std::collections::HashMap;
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
    object_batches: HashMap<usize, Vec<usize>>,
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
            object_batches: HashMap::new(),
        };
        for quad in quads {
            plan.push_quad(quad)?;
        }
        Ok(plan)
    }

    /// Admits one already-tessellated, single-material triangle mesh.
    ///
    /// This boundary exists for developer tooling whose UI dependency emits
    /// indexed triangles instead of the stock client's ordered quads. It does
    /// not alter the stock quad batching path.
    ///
    /// # Errors
    ///
    /// Returns [`UiMeshPlanError`] for invalid extents, non-finite vertices,
    /// out-of-range indices, or counts that cannot enter Vulkan's 32-bit ABI.
    pub fn prepare_indexed(
        logical_extent: [f32; 2],
        vertices: Vec<UiRenderVertex>,
        indices: Vec<u32>,
        source: super::UiRenderSource,
        clip: Option<[f32; 4]>,
    ) -> Result<Self, UiMeshPlanError> {
        validate_extent(logical_extent)?;
        let vertex_count = u32::try_from(vertices.len())
            .map_err(|_source| UiMeshPlanError::Capacity { domain: "vertex" })?;
        let index_count = u32::try_from(indices.len())
            .map_err(|_source| UiMeshPlanError::Capacity { domain: "index" })?;
        for (index, vertex) in vertices.iter().enumerate() {
            validate_components(index, "position", &vertex.position())?;
            validate_components(index, "texture coordinate", &vertex.texture_coordinates())?;
            validate_components(index, "color", &vertex.color())?;
        }
        if let Some(index) = indices.iter().copied().find(|index| *index >= vertex_count) {
            return Err(UiMeshPlanError::IndexOutOfRange {
                index,
                vertex_count,
            });
        }
        let mut vertex_bytes = Vec::with_capacity(
            vertices
                .len()
                .checked_mul(UiRenderVertex::BYTE_SIZE)
                .ok_or(UiMeshPlanError::Capacity {
                    domain: "vertex byte",
                })?,
        );
        for vertex in &vertices {
            vertex.append_bytes(&mut vertex_bytes);
        }
        let mut index_bytes =
            Vec::with_capacity(indices.len().checked_mul(size_of::<u32>()).ok_or(
                UiMeshPlanError::Capacity {
                    domain: "index byte",
                },
            )?);
        for index in &indices {
            index_bytes.extend_from_slice(&index.to_le_bytes());
        }
        Ok(Self {
            identity: next_identity(),
            logical_extent,
            vertices,
            indices,
            vertex_bytes,
            index_bytes,
            batches: vec![super::UiRenderBatch::from_indexed(
                source,
                index_count,
                clip,
            )],
            object_indices: Vec::new(),
            object_batches: HashMap::from([(0, vec![0])]),
        })
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

    /// Reports whether an object already owns at least one retained draw slot.
    #[must_use]
    pub fn contains_object(&self, object_index: usize) -> bool {
        self.object_batches.contains_key(&object_index)
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
                let current = batch.transform_translation();
                batch.set_translation([current[0] + delta[0], current[1] + delta[1]]);
            }
        }
    }

    /// Moves every retained draw owned by one live UI object.
    ///
    /// Object motion composes independently with ScrollFrame and Slider state,
    /// so either source can change without destroying the other contribution.
    pub fn translate_object(
        &mut self,
        object_index: usize,
        delta: [f32; 2],
    ) -> Result<(), UiMeshPlanError> {
        validate_components(object_index, "visual translation", &delta)?;
        let Some(batch_indices) = self.object_batches.get(&object_index) else {
            return Ok(());
        };
        for &batch_index in batch_indices {
            let batch = &mut self.batches[batch_index];
            let current = batch.object_translation();
            let next = [current[0] + delta[0], current[1] + delta[1]];
            validate_components(object_index, "visual translation", &next)?;
            batch.translate_object(delta);
        }
        Ok(())
    }

    /// Replaces inherited opacity for only one retained UI object.
    pub fn set_object_opacity(
        &mut self,
        object_index: usize,
        opacity: f32,
    ) -> Result<(), UiMeshPlanError> {
        if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
            return Err(UiMeshPlanError::InvalidOpacity {
                object_index,
                opacity,
            });
        }
        let Some(batch_indices) = self.object_batches.get(&object_index) else {
            return Ok(());
        };
        for &batch_index in batch_indices {
            self.batches[batch_index].set_opacity(opacity);
        }
        Ok(())
    }

    /// Replaces only retained vertex colors for one object's existing quads.
    ///
    /// Returns `false` when the candidate quad count changes topology.
    pub fn replace_object_quad_colors(
        &mut self,
        object_index: usize,
        colors: &[[[f32; 4]; 4]],
    ) -> Result<bool, UiMeshPlanError> {
        let slot_count = self
            .object_indices
            .iter()
            .filter(|owner| **owner == object_index)
            .count();
        if slot_count != colors.len() {
            return Ok(false);
        }
        let mut changed = false;
        for (slot, quad_colors) in self
            .object_indices
            .iter()
            .enumerate()
            .filter_map(|(slot, owner)| (*owner == object_index).then_some(slot))
            .zip(colors)
        {
            for (corner, &color) in quad_colors.iter().enumerate() {
                validate_components(object_index, "color", &color)?;
                let vertex_index = slot * 4 + corner;
                let previous = self.vertices[vertex_index];
                if previous.color() == color {
                    continue;
                }
                let vertex =
                    UiRenderVertex::new(previous.position(), previous.texture_coordinates(), color);
                self.vertices[vertex_index] = vertex;
                let color_offset = vertex_index * UiRenderVertex::BYTE_SIZE + 16;
                for (component, value) in color.into_iter().enumerate() {
                    let offset = color_offset + component * size_of::<f32>();
                    self.vertex_bytes[offset..offset + size_of::<f32>()]
                        .copy_from_slice(&value.to_le_bytes());
                }
                changed = true;
            }
        }
        if changed {
            self.identity = next_identity();
        }
        Ok(true)
    }

    /// Replaces opacity for one independently retained draw-state slot.
    pub fn set_state_opacity(
        &mut self,
        state: UiRenderState,
        opacity: f32,
    ) -> Result<(), UiMeshPlanError> {
        let UiRenderState::EditBoxCaret(object_index) = state;
        if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
            return Err(UiMeshPlanError::InvalidOpacity {
                object_index,
                opacity,
            });
        }
        for batch in &mut self.batches {
            if batch.state() == Some(state) {
                batch.set_opacity(opacity);
            }
        }
        Ok(())
    }

    /// Replaces the clip rectangle associated with one retained transform slot.
    pub fn set_transform_clip(
        &mut self,
        transform: UiRenderTransform,
        clip: Option<[f32; 4]>,
    ) -> Result<(), UiMeshPlanError> {
        let object_index = match transform {
            UiRenderTransform::ScrollFrame(object_index)
            | UiRenderTransform::Slider(object_index) => object_index,
        };
        if let Some(bounds) = clip {
            validate_components(object_index, "transform clip", &bounds)?;
            if bounds[2] < bounds[0] || bounds[3] < bounds[1] {
                return Err(UiMeshPlanError::InvertedBounds { object_index });
            }
        }
        for batch in &mut self.batches {
            if batch.transform() == Some(transform) {
                batch.set_clip(clip);
            }
        }
        Ok(())
    }

    /// Refreshes inherited opacity without changing immutable mesh identity.
    ///
    /// Batches never cross live-object boundaries, so one region fade remains
    /// independently addressable even when adjacent objects share a texture.
    pub fn refresh_object_opacities(
        &mut self,
        mut opacity: impl FnMut(usize) -> Option<f32>,
    ) -> Result<(), UiMeshPlanError> {
        for batch in &mut self.batches {
            let object_index = batch.object_index();
            let Some(value) = opacity(object_index) else {
                continue;
            };
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(UiMeshPlanError::InvalidOpacity {
                    object_index,
                    opacity: value,
                });
            }
            batch.set_opacity(value);
        }
        Ok(())
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
        let appended = if let Some(batch) = self.batches.last_mut()
            && batch.can_append(&quad)
        {
            batch.append_quad();
            true
        } else {
            self.batches
                .push(UiRenderBatch::from_quad(&quad, first_index, first_quad));
            false
        };
        if !appended {
            let object_index = quad.object_index();
            self.object_batches
                .entry(object_index)
                .or_default()
                .push(self.batches.len() - 1);
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
    let opacity = quad.opacity();
    if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
        return Err(UiMeshPlanError::InvalidOpacity {
            object_index: quad.object_index(),
            opacity,
        });
    }
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
