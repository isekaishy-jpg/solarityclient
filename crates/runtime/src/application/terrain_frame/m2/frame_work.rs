//! One ordered frame work list shared by model animation, visibility and shadows.
//!
//! Residency owns resources. It is not a list of models to prepare every frame.
//! Immutable scenery enters a distance query; update/light owners enter the
//! required list. Exact camera, portal and shadow tests refine their union.

use super::distance::SceneryDistance;
use glam::Vec3;

#[derive(Clone, Copy, Debug, PartialEq)]
struct StaticEntry {
    index: usize,
    scenery: SceneryDistance,
}

const LEAF_SIZE: usize = 16;

struct StaticNode {
    minimum: Vec3,
    maximum: Vec3,
    entries: std::ops::Range<usize>,
    /// The first node after this entire subtree, for allocation-free queries.
    end_node: usize,
}

#[derive(Default)]
struct StaticClass {
    entries: Vec<StaticEntry>,
    nodes: Vec<StaticNode>,
}

impl StaticClass {
    fn build(&mut self) {
        self.nodes.clear();
        if !self.entries.is_empty() {
            self.build_node(0..self.entries.len());
        }
    }

    fn build_node(&mut self, entries: std::ops::Range<usize>) {
        let mut minimum = Vec3::splat(f32::INFINITY);
        let mut maximum = Vec3::splat(f32::NEG_INFINITY);
        for entry in &self.entries[entries.clone()] {
            minimum = minimum.min(entry.scenery.center());
            maximum = maximum.max(entry.scenery.center());
        }
        let node = self.nodes.len();
        self.nodes.push(StaticNode {
            minimum,
            maximum,
            entries: entries.clone(),
            end_node: 0,
        });
        if entries.len() > LEAF_SIZE {
            let size = maximum - minimum;
            let axis = if size.x >= size.y && size.x >= size.z {
                0
            } else if size.y >= size.z {
                1
            } else {
                2
            };
            let middle = entries.len() / 2;
            self.entries[entries.clone()].select_nth_unstable_by(middle, |left, right| {
                left.scenery.center()[axis]
                    .total_cmp(&right.scenery.center()[axis])
                    .then(left.index.cmp(&right.index))
            });
            let split = entries.start + middle;
            self.build_node(entries.start..split);
            self.build_node(split..entries.end);
        }
        self.nodes[node].end_node = self.nodes.len();
    }

    fn select(
        &self,
        camera: Vec3,
        detail: f32,
        shadows: bool,
        radius: f64,
        work: &mut M2FrameWork,
    ) {
        let bounded = camera.is_finite() && radius.is_finite();
        let mut next = 0;
        while let Some(node) = self.nodes.get(next) {
            if bounded
                && (0..3).any(|axis| {
                    f64::from(node.maximum[axis]) < f64::from(camera[axis]) - radius
                        || f64::from(node.minimum[axis]) > f64::from(camera[axis]) + radius
                })
            {
                next = node.end_node;
                continue;
            }
            next += 1;
            if node.entries.len() > LEAF_SIZE {
                continue;
            }
            #[cfg(test)]
            {
                work.static_distance_tests += node.entries.len();
            }
            for entry in &self.entries[node.entries.clone()] {
                if entry.scenery.opacity(camera, detail) != 0.0
                    || (shadows && entry.scenery.admits_shadow(camera, detail))
                {
                    work.indices.push(entry.index);
                }
            }
        }
    }
}

/// Registered alongside placement topology, never rebuilt just for camera motion.
#[derive(Default)]
pub(super) struct M2FrameWorkIndex {
    static_members: Vec<StaticEntry>,
    pending_members: Vec<StaticEntry>,
    by_class: [StaticClass; 5],
    required: Vec<usize>,
}

impl M2FrameWorkIndex {
    /// Dynamic compaction relocates leaf references without changing spatial
    /// partitions. A removed static member requires the ordinary rebuild below.
    pub(super) fn remap_static(&mut self, remap: &[usize]) {
        if self
            .static_members
            .iter()
            .any(|entry| remap[entry.index] == usize::MAX)
        {
            return;
        }
        for entry in &mut self.static_members {
            entry.index = remap[entry.index];
        }
        for class in &mut self.by_class {
            for entry in &mut class.entries {
                entry.index = remap[entry.index];
            }
        }
    }

    /// Effects occupy an ordered dynamic tail. Replacing that tail changes no
    /// scenery bounds, distance classes or spatial search nodes.
    pub(super) fn replace_effect_tail(&mut self, first: usize, end: usize) {
        let retained = self.required.partition_point(|index| *index < first);
        self.required.truncate(retained);
        self.required.extend(first..end);
    }

    pub(super) fn rebuild(&mut self, scenery: &[Option<SceneryDistance>], lights: &[bool]) {
        self.pending_members.clear();
        self.required.clear();
        for (index, (&scenery, &has_lights)) in scenery.iter().zip(lights).enumerate() {
            if !has_lights
                && let Some(scenery) = scenery
                && scenery.center().is_finite()
            {
                self.pending_members.push(StaticEntry { index, scenery });
            } else {
                // Moving models, light owners and unclassified inputs retain
                // all existing callback/attachment behavior, even off screen.
                self.required.push(index);
            }
        }
        // Effect publication often changes only the dynamic tail. Keep the
        // static search order when the exact static membership is unchanged.
        if self.pending_members == self.static_members {
            return;
        }
        std::mem::swap(&mut self.pending_members, &mut self.static_members);
        solarity_profiling::profile_event_value!(
            "m2.topology.static_spatial_rebuilt",
            self.static_members.len()
        );
        for class in &mut self.by_class {
            class.entries.clear();
        }
        for &entry in &self.static_members {
            self.by_class[entry.scenery.category()].entries.push(entry);
        }
        for class in &mut self.by_class {
            class.build();
        }
    }

    /// The broad query includes both ordinary fade distance and shadow distance.
    /// Camera visibility alone must never remove an offscreen shadow caster.
    pub(super) fn select(&self, camera: Vec3, detail: f32, shadows: bool, work: &mut M2FrameWork) {
        work.indices.clear();
        work.indices.extend_from_slice(&self.required);
        work.cursor = 0;
        #[cfg(test)]
        {
            work.static_distance_tests = 0;
        }
        let radii = SceneryDistance::frame_query_radii(detail);
        for (class, radius) in self.by_class.iter().zip(radii) {
            class.select(camera, detail, shadows, radius, work);
        }
        // All sources are disjoint. Restore original placement order so shared
        // random streams, parent poses, transparent ties and effects stay stable.
        work.indices.sort_unstable();
    }
}

#[derive(Default)]
pub(super) struct M2FrameWork {
    indices: Vec<usize>,
    cursor: usize,
    #[cfg(test)]
    static_distance_tests: usize,
}

impl M2FrameWork {
    /// Immutable candidates can be captured before the ordered cursor advances.
    pub(super) fn selected_indices(&self) -> &[usize] {
        &self.indices[self.cursor..]
    }

    /// Upper bound before exact camera/portal admission, excluding distant
    /// residents. Newly queued effects have a separate ordered-tail reservation.
    pub(super) fn remaining_count(&self) -> usize {
        self.indices.len() - self.cursor
    }

    pub(super) fn next_index(&self) -> Option<usize> {
        self.indices.get(self.cursor).copied()
    }

    pub(super) fn next(&mut self) -> Option<usize> {
        let index = self.next_index()?;
        self.cursor += 1;
        Some(index)
    }

    /// New effects become work only after every ordinary parent has prepared.
    /// Refreshing the tail neither repeats nor reorders completed placements.
    pub(super) fn publish_effect_tail(&mut self, first: usize, count: usize) {
        debug_assert!(self.next_index().is_none_or(|index| index >= first));
        self.indices.truncate(self.cursor);
        self.indices.extend(first..count);
    }
}

#[cfg(test)]
#[path = "../../../../tests/application/m2_frame_work.rs"]
mod tests;
