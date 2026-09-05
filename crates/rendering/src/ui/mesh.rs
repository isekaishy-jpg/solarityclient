//! Allocation-conscious conversion from ordered UI quads to indexed mesh runs.

use super::UiRenderState;
use super::{
    UiMeshPlanError, UiRenderBatch, UiRenderQuad, UiRenderSource, UiRenderTransform, UiRenderVertex,
};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};

const RETAINED_VERTEX_REVISION_LIMIT: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct UiVertexRevision {
    from: u64,
    to: u64,
    byte_range: (usize, usize),
}

/// Upload-ready UI geometry with adjacent compatible quads already batched.
#[derive(Clone, Debug, PartialEq)]
pub struct UiMeshPlan {
    identity: u64,
    logical_extent: [f32; 2],
    vertices: Vec<UiRenderVertex>,
    indices: Vec<u32>,
    batches: Vec<UiRenderBatch>,
    object_indices: Vec<usize>,
    object_quads: HashMap<usize, Vec<usize>>,
    object_batches: HashMap<usize, Vec<usize>>,
    vertex_revisions: VecDeque<UiVertexRevision>,
}

impl UiMeshPlan {
    /// Converts exact-size ordered quads into one shared vertex allocation and
    /// a canonical quad-index prefix shared by every material batch.
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
        let _vertex_byte_capacity = vertex_capacity
            .checked_mul(UiRenderVertex::BYTE_SIZE)
            .ok_or(UiMeshPlanError::Capacity {
                domain: "vertex byte",
            })?;
        let retained_index_capacity = index_capacity.min(256 * 6);
        let _index_byte_capacity = retained_index_capacity
            .checked_mul(size_of::<u32>())
            .ok_or(UiMeshPlanError::Capacity {
                domain: "index byte",
            })?;
        let mut plan = Self {
            identity: next_identity(),
            logical_extent,
            vertices: Vec::with_capacity(vertex_capacity),
            indices: Vec::with_capacity(retained_index_capacity),
            batches: Vec::with_capacity(quad_count),
            object_indices: Vec::with_capacity(quad_count),
            object_quads: HashMap::new(),
            object_batches: HashMap::new(),
            vertex_revisions: VecDeque::new(),
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
        vertices
            .len()
            .checked_mul(UiRenderVertex::BYTE_SIZE)
            .ok_or(UiMeshPlanError::Capacity {
                domain: "vertex byte",
            })?;
        indices
            .len()
            .checked_mul(size_of::<u32>())
            .ok_or(UiMeshPlanError::Capacity {
                domain: "index byte",
            })?;
        Ok(Self {
            identity: next_identity(),
            logical_extent,
            vertices,
            indices,
            batches: vec![super::UiRenderBatch::from_indexed(
                source,
                index_count,
                clip,
            )],
            object_indices: Vec::new(),
            object_quads: HashMap::new(),
            object_batches: HashMap::from([(0, vec![0])]),
            vertex_revisions: VecDeque::new(),
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

    /// Returns sources of one object's retained batches in draw order.
    ///
    /// A source may appear in multiple runs. This also distinguishes an
    /// object's glyph-only slots from its independently retained decorations.
    pub fn sources_for_object(&self, object_index: usize) -> impl Iterator<Item = &UiRenderSource> {
        self.object_batches
            .get(&object_index)
            .into_iter()
            .flatten()
            .map(|&index| self.batches[index].source())
    }

    /// Returns the smallest contiguous quad window intersecting a batch clip.
    ///
    /// ScrollFrame documents remain fully resident so scrolling never uploads
    /// new vertices. The prepared draw can nevertheless skip off-screen pages
    /// by selecting this translated, scissor-visible window with `baseVertex`.
    /// Batches without a clip retain their authored complete range.
    #[must_use]
    pub fn clipped_batch_quad_range(&self, batch_index: usize) -> Option<(u32, u32)> {
        let batch = self.batches.get(batch_index)?;
        let Some([clip_left, clip_bottom, clip_right, clip_top]) = batch.clip() else {
            return Some((batch.first_quad(), batch.quad_count()));
        };
        let [translate_x, translate_y] = batch.translation();
        let first_quad = batch.first_quad() as usize;
        let quad_count = batch.quad_count() as usize;
        let mut first_visible = None;
        let mut last_visible = 0;
        for relative_quad in 0..quad_count {
            let vertex = (first_quad + relative_quad) * 4;
            let vertices = &self.vertices[vertex..vertex + 4];
            let left = vertices
                .iter()
                .map(|vertex| vertex.position()[0])
                .fold(f32::INFINITY, f32::min)
                + translate_x;
            let right = vertices
                .iter()
                .map(|vertex| vertex.position()[0])
                .fold(f32::NEG_INFINITY, f32::max)
                + translate_x;
            let bottom = vertices
                .iter()
                .map(|vertex| vertex.position()[1])
                .fold(f32::INFINITY, f32::min)
                + translate_y;
            let top = vertices
                .iter()
                .map(|vertex| vertex.position()[1])
                .fold(f32::NEG_INFINITY, f32::max)
                + translate_y;
            if right <= clip_left || left >= clip_right || top <= clip_bottom || bottom >= clip_top
            {
                continue;
            }
            first_visible.get_or_insert(relative_quad);
            last_visible = relative_quad + 1;
        }
        let first_visible = first_visible.unwrap_or(0);
        Some((
            batch.first_quad() + first_visible as u32,
            (last_visible - first_visible) as u32,
        ))
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

    /// Returns the merged vertex byte span changed since a retained generation.
    ///
    /// The bounded journal is an optimization hint: a renderer falls back to
    /// comparing complete payloads when the requested ancestor has expired or
    /// came from an independently built plan.
    pub(crate) fn vertex_update_range_since(&self, identity: u64) -> Option<(usize, usize)> {
        let first = self
            .vertex_revisions
            .iter()
            .position(|revision| revision.from == identity)?;
        let mut cursor = identity;
        let mut range: Option<(usize, usize)> = None;
        for revision in self.vertex_revisions.iter().skip(first) {
            if revision.from != cursor {
                return None;
            }
            range = Some(range.map_or(revision.byte_range, |range| {
                (
                    range.0.min(revision.byte_range.0),
                    range.1.max(revision.byte_range.1),
                )
            }));
            cursor = revision.to;
            if cursor == self.identity {
                return range;
            }
        }
        None
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
        let Some(slots) = self.object_quads.get(&object_index) else {
            return Ok(colors.is_empty());
        };
        if slots.len() != colors.len() {
            return Ok(false);
        }
        let mut changed_vertices: Option<(usize, usize)> = None;
        for (&slot, quad_colors) in slots.iter().zip(colors) {
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
                changed_vertices = Some(
                    changed_vertices.map_or((vertex_index, vertex_index + 1), |(start, end)| {
                        (start.min(vertex_index), end.max(vertex_index + 1))
                    }),
                );
            }
        }
        if let Some((start, end)) = changed_vertices {
            let previous = self.identity;
            let current = next_identity();
            self.identity = current;
            if self.vertex_revisions.len() == RETAINED_VERTEX_REVISION_LIMIT {
                self.vertex_revisions.pop_front();
            }
            self.vertex_revisions.push_back(UiVertexRevision {
                from: previous,
                to: current,
                byte_range: (
                    start * UiRenderVertex::BYTE_SIZE,
                    end * UiRenderVertex::BYTE_SIZE,
                ),
            });
        }
        Ok(true)
    }

    /// Replaces one complete contiguous source run while retaining other payloads.
    ///
    /// The replacement may resize the run or change its material/state batches.
    /// The old object/source must occupy one uninterrupted range; replacement
    /// quads must share one object and source, in the owner's desired draw order.
    /// Other batches retain their order, vertices, transforms, and opacity;
    /// following quad offsets move with the resized range. Replacement bounds
    /// are absolute presentation coordinates, as for fixed-slot replacement.
    /// Empty replacements and interrupted source ranges return `false` without
    /// mutation so their owner can publish a complete topology change instead.
    ///
    /// # Errors
    ///
    /// Returns [`UiMeshPlanError`] for invalid replacement vertices or counts
    /// outside the renderer's index/vertex ABI, without changing the mesh.
    pub fn replace_object_source_run(
        &mut self,
        object_index: usize,
        previous_source: &UiRenderSource,
        quads: &[UiRenderQuad],
    ) -> Result<bool, UiMeshPlanError> {
        let Some(first) = quads.first() else {
            return Ok(false);
        };
        if first.object_index() != object_index {
            return Ok(false);
        }
        let mut matching = self
            .object_batches
            .get(&object_index)
            .into_iter()
            .flatten()
            .copied()
            .filter(|&index| self.batches[index].source() == previous_source);
        let Some(batch_index) = matching.next() else {
            return Ok(false);
        };
        let mut old_batch_end = batch_index + 1;
        for index in matching {
            if index != old_batch_end {
                return Ok(false);
            }
            old_batch_end += 1;
        }
        let old = &self.batches[batch_index];
        let start = old.first_quad() as usize;
        let last = &self.batches[old_batch_end - 1];
        let old_end = last.first_quad() as usize + last.quad_count() as usize;
        let old_count = old_end - start;
        if old_count == 0 {
            return Ok(false);
        }
        let new_count = quads.len();
        let total_quads = self
            .object_indices
            .len()
            .checked_sub(old_count)
            .and_then(|count| count.checked_add(new_count))
            .ok_or(UiMeshPlanError::Capacity { domain: "quad" })?;
        let vertex_count = total_quads
            .checked_mul(4)
            .ok_or(UiMeshPlanError::Capacity { domain: "vertex" })?;
        u32::try_from(vertex_count).map_err(|_| UiMeshPlanError::Capacity { domain: "vertex" })?;
        vertex_count
            .checked_mul(UiRenderVertex::BYTE_SIZE)
            .ok_or(UiMeshPlanError::Capacity {
                domain: "vertex byte",
            })?;
        let _index_count = new_count
            .checked_mul(6)
            .and_then(|count| u32::try_from(count).ok())
            .ok_or(UiMeshPlanError::Capacity { domain: "index" })?;
        let mut replacements: Vec<UiRenderBatch> = Vec::new();
        let mut vertices = Vec::with_capacity(new_count * 4);
        for (offset, quad) in quads.iter().enumerate() {
            validate_quad(quad)?;
            if quad.object_index() != object_index || quad.source() != first.source() {
                return Ok(false);
            }
            if let Some(batch) = replacements.last_mut()
                && batch.can_append(quad)
            {
                batch.append_quad();
            } else {
                replacements.push(UiRenderBatch::from_quad(quad, 0, (start + offset) as u32));
            }
            let [left, bottom, right, top] = quad.bounds();
            let positions = [[left, top], [left, bottom], [right, top], [right, bottom]];
            let coordinates = quad.texture_coordinates();
            let colors = quad.colors();
            for corner in 0..4 {
                vertices.push(UiRenderVertex::new(
                    positions[corner],
                    coordinates[corner],
                    colors[corner],
                ));
            }
        }
        let new_batch_count = replacements.len();
        let maximum_new_run = replacements
            .iter()
            .map(|batch| batch.quad_count() as usize)
            .max()
            .unwrap_or(0);
        self.ensure_canonical_quad_indices(maximum_new_run)?;
        self.vertices.splice(start * 4..old_end * 4, vertices);
        self.object_indices
            .splice(start..old_end, std::iter::repeat_n(object_index, new_count));
        for (&owner, slots) in &mut self.object_quads {
            let first = slots.partition_point(|&slot| slot < start);
            let end = slots.partition_point(|&slot| slot < old_end);
            for slot in &mut slots[end..] {
                *slot = *slot - old_count + new_count;
            }
            if owner == object_index {
                slots.splice(first..end, start..start + new_count);
            }
        }
        let old_batch_count = old_batch_end - batch_index;
        for (&owner, batches) in &mut self.object_batches {
            let first = batches.partition_point(|&index| index < batch_index);
            let end = batches.partition_point(|&index| index < old_batch_end);
            for index in &mut batches[end..] {
                *index = *index - old_batch_count + new_batch_count;
            }
            if owner == object_index {
                batches.splice(first..end, batch_index..batch_index + new_batch_count);
            }
        }
        self.batches
            .splice(batch_index..old_batch_end, replacements);
        for batch in &mut self.batches[batch_index + new_batch_count..] {
            batch.set_first_quad((batch.first_quad() as usize - old_count + new_count) as u32);
        }
        let maximum_run = self
            .batches
            .iter()
            .map(|batch| batch.quad_count() as usize)
            .max()
            .unwrap_or(0);
        self.indices.truncate(maximum_run * 6);
        // Moving following runs invalidates offsets from earlier byte revisions.
        // The renderer compares the retained payload and uploads its changed span.
        self.vertex_revisions.clear();
        self.identity = next_identity();
        Ok(true)
    }

    /// Replaces complete vertices for one object's topology-stable source slots.
    ///
    /// Objects may own several sources at once (for example, an EditBox backdrop
    /// plus glyph-atlas text). Material, draw-state, and source-local quad counts
    /// must remain unchanged. This lets bounded text replace only its atlas
    /// vertices without rebuilding decorations, unrelated batches, or indices.
    /// Replacement bounds are absolute presentation coordinates: they replace
    /// accumulated object translation, while retaining the source's scroll or
    /// slider transform and any independently translated decorations.
    pub fn replace_object_source_quads(
        &mut self,
        object_index: usize,
        source: &UiRenderSource,
        quads: &[UiRenderQuad],
    ) -> Result<bool, UiMeshPlanError> {
        let Some(object_slots) = self.object_quads.get(&object_index) else {
            return Ok(quads.is_empty());
        };
        let slots = object_slots
            .iter()
            .copied()
            .filter(|&slot| {
                self.batch_for_slot(slot)
                    .is_some_and(|batch| batch.source() == source)
            })
            .collect::<Vec<_>>();
        if slots.len() != quads.len() {
            return Ok(false);
        }
        let mut batch_index = slots.first().map_or(0, |&first_slot| {
            self.batches.partition_point(|batch| {
                batch.first_quad() as usize + batch.quad_count() as usize <= first_slot
            })
        });
        for (slot, quad) in slots.iter().copied().zip(quads) {
            validate_quad(quad)?;
            if quad.object_index() != object_index || quad.source() != source {
                return Ok(false);
            }
            while self.batches.get(batch_index).is_some_and(|batch| {
                slot >= batch.first_quad() as usize + batch.quad_count() as usize
            }) {
                batch_index += 1;
            }
            let Some(batch) = self.batches.get(batch_index) else {
                return Ok(false);
            };
            if slot < batch.first_quad() as usize || !batch.can_replace(quad) {
                return Ok(false);
            }
        }

        // Every slot for this object/source was validated above. Its freshly
        // resolved bounds supersede prior animation deltas; other sources
        // still use their old vertices and therefore retain those deltas.
        if let Some(batch_indices) = self.object_batches.get(&object_index) {
            for &index in batch_indices {
                if self.batches[index].source() == source {
                    self.batches[index].reset_object_translation();
                }
            }
        }
        let mut changed_vertices: Option<(usize, usize)> = None;
        for (slot, quad) in slots.iter().copied().zip(quads) {
            let [left, bottom, right, top] = quad.bounds();
            let positions = [[left, top], [left, bottom], [right, top], [right, bottom]];
            let coordinates = quad.texture_coordinates();
            let colors = quad.colors();
            for corner in 0..4 {
                let vertex_index = slot * 4 + corner;
                let vertex =
                    UiRenderVertex::new(positions[corner], coordinates[corner], colors[corner]);
                if self.vertices[vertex_index] == vertex {
                    continue;
                }
                self.vertices[vertex_index] = vertex;
                changed_vertices = Some(
                    changed_vertices.map_or((vertex_index, vertex_index + 1), |(start, end)| {
                        (start.min(vertex_index), end.max(vertex_index + 1))
                    }),
                );
            }
        }
        if let Some((start, end)) = changed_vertices {
            let previous = self.identity;
            let current = next_identity();
            self.identity = current;
            if self.vertex_revisions.len() == RETAINED_VERTEX_REVISION_LIMIT {
                self.vertex_revisions.pop_front();
            }
            self.vertex_revisions.push_back(UiVertexRevision {
                from: previous,
                to: current,
                byte_range: (
                    start * UiRenderVertex::BYTE_SIZE,
                    end * UiRenderVertex::BYTE_SIZE,
                ),
            });
        }
        Ok(true)
    }

    fn batch_for_slot(&self, slot: usize) -> Option<&UiRenderBatch> {
        let batch_index = self.batches.partition_point(|batch| {
            batch.first_quad() as usize + batch.quad_count() as usize <= slot
        });
        self.batches.get(batch_index).filter(|batch| {
            slot >= batch.first_quad() as usize
                && slot < batch.first_quad() as usize + batch.quad_count() as usize
        })
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
        bytemuck::cast_slice(&self.vertices)
    }

    /// Serializes direct unsigned 32-bit indices for Vulkan upload.
    #[must_use]
    pub fn index_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.indices)
    }

    /// Validates and appends one counter-clockwise two-triangle quad.
    fn push_quad(&mut self, quad: UiRenderQuad) -> Result<(), UiMeshPlanError> {
        validate_quad(&quad)?;
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
            self.vertices.push(vertex);
        }
        let appended = if let Some(batch) = self.batches.last_mut()
            && batch.can_append(&quad)
        {
            batch.append_quad();
            true
        } else {
            self.batches
                .push(UiRenderBatch::from_quad(&quad, 0, first_quad));
            false
        };
        let required_quad_count = self
            .batches
            .last()
            .map_or(0, |batch| batch.quad_count() as usize);
        self.ensure_canonical_quad_indices(required_quad_count)?;
        if !appended {
            let object_index = quad.object_index();
            self.object_batches
                .entry(object_index)
                .or_default()
                .push(self.batches.len() - 1);
        }
        self.object_quads
            .entry(quad.object_index())
            .or_default()
            .push(self.object_indices.len());
        self.object_indices.push(quad.object_index());
        Ok(())
    }

    /// Extends the reusable zero-based index prefix only when a larger
    /// contiguous material run appears. Draws select their own vertex range
    /// with `baseVertex`, so duplicating six absolute indices for every UI
    /// quad wastes CPU serialization, upload bandwidth, and device memory.
    fn ensure_canonical_quad_indices(
        &mut self,
        required_quad_count: usize,
    ) -> Result<(), UiMeshPlanError> {
        let current_quad_count = self.indices.len() / 6;
        for quad_index in current_quad_count..required_quad_count {
            let base_vertex = u32::try_from(
                quad_index
                    .checked_mul(4)
                    .ok_or(UiMeshPlanError::Capacity { domain: "vertex" })?,
            )
            .map_err(|_source| UiMeshPlanError::Capacity { domain: "vertex" })?;
            let indices = [
                base_vertex,
                base_vertex + 1,
                base_vertex + 2,
                base_vertex + 2,
                base_vertex + 1,
                base_vertex + 3,
            ];
            self.indices.extend_from_slice(&indices);
        }
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
