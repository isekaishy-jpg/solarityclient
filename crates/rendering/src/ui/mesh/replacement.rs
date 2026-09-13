//! Source-local geometry publication with independently ordered draw batches.

use super::{
    ClipIndex, RETAINED_VERTEX_REVISION_LIMIT, UiMeshPlan, UiMeshPlanError, UiRenderBatch,
    UiRenderQuad, UiRenderVertex, UiVertexRevision, next_identity, validate_quad,
};
use std::ops::Range;

impl UiMeshPlan {
    /// Validates the complete candidate before allocating or publishing any data.
    /// Only the changed owner's vertices are written; other draw offsets survive.
    pub(super) fn splice_object_source_run(
        &mut self,
        object_index: usize,
        old_batches: Range<usize>,
        quads: &[UiRenderQuad],
    ) -> Result<bool, UiMeshPlanError> {
        let count = quads.len();
        let vertex_count = count
            .checked_mul(4)
            .and_then(|count| u32::try_from(count).ok())
            .ok_or(UiMeshPlanError::Capacity { domain: "vertex" })?;
        let _bytes = (vertex_count as usize)
            .checked_mul(UiRenderVertex::BYTE_SIZE)
            .ok_or(UiMeshPlanError::Capacity {
                domain: "vertex byte",
            })?;
        let mut replacements: Vec<UiRenderBatch> = Vec::new();
        let mut vertices = Vec::with_capacity(count * 4);
        for (offset, quad) in quads.iter().enumerate() {
            validate_quad(quad)?;
            if quad.object_index() != object_index {
                return Ok(false);
            }
            if let Some(batch) = replacements.last_mut()
                && batch.can_append(quad)
            {
                batch.append_quad();
            } else {
                replacements.push(UiRenderBatch::from_quad(quad, 0, offset as u32));
            }
            for corner in 0..4 {
                vertices.push(UiRenderVertex::new(
                    quad.positions()[corner],
                    quad.texture_coordinates()[corner],
                    quad.colors()[corner],
                ));
            }
        }
        let old = if old_batches.is_empty() {
            None
        } else {
            let start = self.batches[old_batches.start].first_quad() as usize;
            let mut end = start;
            for batch in &self.batches[old_batches.clone()] {
                if batch.first_quad() as usize != end || batch.quad_count() == 0 {
                    return Ok(false);
                }
                end += batch.quad_count() as usize;
            }
            Some((start, end - start))
        };
        let maximum_run = replacements
            .iter()
            .map(|batch| batch.quad_count() as usize)
            .max()
            .unwrap_or(0);
        maximum_run
            .checked_mul(6)
            .and_then(|count| u32::try_from(count).ok())
            .ok_or(UiMeshPlanError::Capacity { domain: "index" })?;
        let (start, capacity) = self
            .storage
            .replace(old, count, self.object_indices.len())?;
        self.ensure_canonical_quad_indices(maximum_run)?;
        if let Some((old_start, old_count)) = old {
            self.object_indices[old_start..old_start + old_count].fill(usize::MAX);
        }
        if count != 0 {
            let extent = self.object_indices.len().max(start + capacity);
            self.object_indices.resize(extent, usize::MAX);
            self.vertices
                .resize(extent * 4, UiRenderVertex::new([0.; 2], [0.; 2], [0.; 4]));
            self.vertices[start * 4..(start + count) * 4].copy_from_slice(&vertices);
            self.object_indices[start..start + count].fill(object_index);
        }
        for batch in &mut replacements {
            batch.set_first_quad(batch.first_quad() + start as u32);
        }
        let clips = replacements
            .iter()
            .map(|batch| ClipIndex::new(&self.vertices, batch))
            .collect::<Vec<_>>();
        let new_batch_count = replacements.len();
        let old_batch_count = old_batches.len();
        self.object_batches.entry(object_index).or_default();
        for (&owner, batches) in &mut self.object_batches {
            let first = batches.partition_point(|index| *index < old_batches.start);
            let end = batches.partition_point(|index| *index < old_batches.end);
            for index in &mut batches[end..] {
                *index = *index - old_batch_count + new_batch_count;
            }
            if owner == object_index {
                batches.splice(
                    first..end,
                    old_batches.start..old_batches.start + new_batch_count,
                );
            }
        }
        self.clip_indices.splice(old_batches.clone(), clips);
        self.batches.splice(old_batches, replacements);
        self.rebuild_transform_batches();
        // Quad ownership follows draw order, independently of physical allocation.
        let slots = self.object_quads.entry(object_index).or_default();
        slots.clear();
        for &index in &self.object_batches[&object_index] {
            let batch = &self.batches[index];
            let first = batch.first_quad() as usize;
            slots.extend(first..first + batch.quad_count() as usize);
        }
        let maximum_run = self
            .batches
            .iter()
            .map(|batch| batch.quad_count() as usize)
            .max()
            .unwrap_or(0);
        self.indices.truncate(maximum_run * 6);
        let previous = self.identity;
        self.identity = next_identity();
        if self.vertex_revisions.len() == RETAINED_VERTEX_REVISION_LIMIT {
            self.vertex_revisions.pop_front();
        }
        self.vertex_revisions.push_back(UiVertexRevision {
            from: previous,
            to: self.identity,
            byte_range: (
                start * 4 * UiRenderVertex::BYTE_SIZE,
                (start + count) * 4 * UiRenderVertex::BYTE_SIZE,
            ),
        });
        Ok(true)
    }
}
