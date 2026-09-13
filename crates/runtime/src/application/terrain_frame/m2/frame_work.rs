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

    #[cfg(test)]
    pub(super) fn diagnostic_counts(&self) -> (usize, usize) {
        (self.cursor, self.static_distance_tests)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Mat4;

    fn prop(center: Vec3, extent: f32) -> SceneryDistance {
        SceneryDistance::new(
            Vec3::splat(-extent * 0.5),
            Vec3::splat(extent * 0.5),
            Mat4::from_translation(center),
        )
    }

    fn assert_matches_resident_reference(
        index: &M2FrameWorkIndex,
        scenery: &[Option<SceneryDistance>],
        lights: &[bool],
        camera: Vec3,
        detail: f32,
        shadows: bool,
    ) {
        let mut work = M2FrameWork::default();
        index.select(camera, detail, shadows, &mut work);
        let expected = scenery
            .iter()
            .zip(lights)
            .enumerate()
            .filter_map(|(index, (&scenery, &light))| {
                (light
                    || scenery.is_none_or(|scenery| {
                        !scenery.center().is_finite()
                            || scenery.opacity(camera, detail) != 0.0
                            || (shadows && scenery.admits_shadow(camera, detail))
                    }))
                .then_some(index)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            work.indices, expected,
            "camera={camera:?} detail={detail} shadows={shadows}"
        );
    }

    #[test]
    fn work_query_matches_native_distance_boundaries_and_unusual_inputs()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut scenery = vec![None, Some(prop(Vec3::splat(f32::NAN), 1.))];
        let mut cameras = Vec::new();
        for line in include_str!(
            "../../../../tests/application/fixtures/scenery_shadow_distance_native.txt"
        )
        .lines()
        .filter_map(|line| line.strip_prefix("shadow_distance "))
        {
            let values = line
                .split_whitespace()
                .map(str::parse::<f32>)
                .collect::<Result<Vec<_>, _>>()?;
            let center = Vec3::from_slice(&values[2..5]);
            scenery.push(Some(prop(
                center,
                [1., 4., 15., 100., 101.][values[0] as usize],
            )));
            cameras.push((Vec3::from_slice(&values[5..8]), values[1]));
        }
        for row in
            include_str!("../../../../tests/application/fixtures/scenery_distance_native.txt")
                .lines()
                .filter(|row| !row.starts_with('#'))
        {
            let values = row
                .split_whitespace()
                .map(str::parse::<f32>)
                .collect::<Result<Vec<_>, _>>()?;
            let matrix: [f32; 16] = values[6..22].try_into()?;
            scenery.push(Some(SceneryDistance::new(
                Vec3::from_slice(&values[..3]),
                Vec3::from_slice(&values[3..6]),
                Mat4::from_cols_array(&matrix),
            )));
            cameras.push((Vec3::from_slice(&values[22..25]), values[25]));
        }
        let mut lights = vec![false; scenery.len()];
        lights[2] = true;
        let mut index = M2FrameWorkIndex::default();
        index.rebuild(&scenery, &lights);
        for (camera, detail) in cameras.into_iter().chain([
            (Vec3::ZERO, 0.),
            (Vec3::ZERO, -1.),
            (Vec3::ZERO, f32::NAN),
            (Vec3::ZERO, f32::INFINITY),
            (Vec3::splat(f32::NAN), 1.),
        ]) {
            for shadows in [false, true] {
                assert_matches_resident_reference(
                    &index, &scenery, &lights, camera, detail, shadows,
                );
            }
        }
        Ok(())
    }

    #[test]
    fn distant_residency_does_not_enter_frame_preparation() {
        let mut scenery = vec![Some(prop(Vec3::ZERO, 1.)), None];
        scenery.extend((0..20_000).map(|i| Some(prop(Vec3::new(0., 10_000. + i as f32, 0.), 1.))));
        let mut lights = vec![false; scenery.len()];
        lights[500] = true;
        let mut index = M2FrameWorkIndex::default();
        index.rebuild(&scenery, &lights);
        let mut work = M2FrameWork::default();
        index.select(Vec3::ZERO, 1.5, true, &mut work);
        assert_eq!(work.indices, [0, 1, 500]);
        assert!(work.static_distance_tests <= LEAF_SIZE);

        // Replacement/removal changes placement indices; an unchanged static
        // tail from a previous topology must not keep an obsolete candidate.
        scenery.remove(0);
        lights.remove(0);
        index.rebuild(&scenery, &lights);
        index.select(Vec3::ZERO, 1.5, true, &mut work);
        assert_eq!(work.indices, [0, 499]);
        assert_eq!(work.static_distance_tests, 0);
    }

    #[test]
    fn new_effects_preserve_completed_parent_order_and_empty_boundaries() {
        let mut work = M2FrameWork {
            indices: vec![2, 5, 10],
            ..Default::default()
        };
        assert_eq!(work.next(), Some(2));
        assert_eq!(work.next(), Some(5));
        work.publish_effect_tail(10, 13);
        assert_eq!(
            (work.next(), work.next(), work.next(), work.next()),
            (Some(10), Some(11), Some(12), None)
        );
        let mut work = M2FrameWork::default();
        work.publish_effect_tail(20_000, 20_001);
        assert_eq!(work.next(), Some(20_000));
        assert_eq!(work.next(), None);
    }
}
