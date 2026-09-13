//! Retained ordered bounds skip document pages without visiting their glyphs.

use super::{UiRenderBatch, UiRenderVertex};
use std::ops::Range;

/// Leaves cover a short contiguous glyph/quad block. Internal bounds let either
/// end of the visible window be found without scanning the entire document.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct ClipIndex {
    nodes: Vec<[f32; 4]>,
    leaves: usize,
    quads: usize,
}

const LEAF_QUADS: usize = 16;
const EMPTY: [f32; 4] = [
    f32::INFINITY,
    f32::INFINITY,
    f32::NEG_INFINITY,
    f32::NEG_INFINITY,
];

impl ClipIndex {
    /// Builds only when the batch's object-owned geometry changes. Transforms,
    /// opacity and scissor changes do not invalidate authored vertex bounds.
    pub(super) fn new(vertices: &[UiRenderVertex], batch: &UiRenderBatch) -> Self {
        let quads = batch.quad_count() as usize;
        if quads == 0 {
            return Self::default();
        }
        let leaves = quads.div_ceil(LEAF_QUADS).next_power_of_two();
        let mut index = Self {
            nodes: vec![EMPTY; leaves * 2],
            leaves,
            quads,
        };
        let first = batch.first_quad() as usize;
        for quad in 0..quads {
            let leaf = leaves + quad / LEAF_QUADS;
            index.nodes[leaf] = union(index.nodes[leaf], quad_bounds(vertices, first + quad));
        }
        for node in (1..leaves).rev() {
            index.nodes[node] = union(index.nodes[node * 2], index.nodes[node * 2 + 1]);
        }
        index
    }

    /// Finds exact first/last intersecting quads in original draw order. The
    /// scissor continues to clip intervening quads and partial edge glyphs.
    pub(super) fn visible_range(
        &self,
        vertices: &[UiRenderVertex],
        batch: &UiRenderBatch,
    ) -> (u32, u32) {
        let Some(bounds) = batch.clip() else {
            return (batch.first_quad(), batch.quad_count());
        };
        let clip = ClipQuery {
            bounds,
            translation: batch.translation(),
        };
        let first = batch.first_quad() as usize;
        let Some(begin) = self.edge(vertices, first, clip, 1, 0..self.leaves, false) else {
            return (batch.first_quad(), 0);
        };
        let mut end = begin;
        if let Some(last) = self.edge(vertices, first, clip, 1, 0..self.leaves, true) {
            end = last;
        }
        ((first + begin) as u32, (end - begin + 1) as u32)
    }

    /// Traverse toward one edge, pruning whole ordered subtrees by their bounds.
    fn edge(
        &self,
        vertices: &[UiRenderVertex],
        first: usize,
        clip: ClipQuery,
        node: usize,
        leaves: Range<usize>,
        reverse: bool,
    ) -> Option<usize> {
        if !intersects(*self.nodes.get(node)?, clip) {
            return None;
        }
        if leaves.len() == 1 {
            let quads =
                leaves.start * LEAF_QUADS..((leaves.start + 1) * LEAF_QUADS).min(self.quads);
            return if reverse {
                quads
                    .rev()
                    .find(|quad| intersects(quad_bounds(vertices, first + quad), clip))
            } else {
                quads
                    .into_iter()
                    .find(|quad| intersects(quad_bounds(vertices, first + quad), clip))
            };
        }
        let middle = (leaves.start + leaves.end) / 2;
        let left = (node * 2, leaves.start..middle);
        let right = (node * 2 + 1, middle..leaves.end);
        let (near, far) = if reverse {
            (right, left)
        } else {
            (left, right)
        };
        self.edge(vertices, first, clip, near.0, near.1, reverse)
            .or_else(|| self.edge(vertices, first, clip, far.0, far.1, reverse))
    }
}

/// Quad corners may be rotated; preserve the prior conservative scissor test.
fn quad_bounds(vertices: &[UiRenderVertex], quad: usize) -> [f32; 4] {
    vertices[quad * 4..quad * 4 + 4]
        .iter()
        .fold(EMPTY, |bounds, vertex| {
            let [x, y] = vertex.position();
            union(bounds, [x, y, x, y])
        })
}

fn union(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

/// Translate vertex bounds in the same arithmetic order as the original clip test.
#[derive(Clone, Copy)]
struct ClipQuery {
    bounds: [f32; 4],
    translation: [f32; 2],
}

fn intersects(bounds: [f32; 4], clip: ClipQuery) -> bool {
    let [x, y] = clip.translation;
    bounds[2] + x > clip.bounds[0]
        && bounds[0] + x < clip.bounds[2]
        && bounds[3] + y > clip.bounds[1]
        && bounds[1] + y < clip.bounds[3]
}
