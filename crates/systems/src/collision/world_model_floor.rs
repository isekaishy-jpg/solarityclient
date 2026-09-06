//! Native dual-result WMO group floor query (`0x007CB260`).

use glam::Vec3;
use solarity_asset::DecodedWorldModelGroup;

use super::{MovementBspCacheMode, PlacedWorldModelCollision, WorldModelCollisionError};

const REGION_TOLERANCE: f64 = 0.01_f32 as f64;

/// One authored face selected by the registration floor ray.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldModelFloorHit {
    fraction: f32,
    face: u16,
}

impl WorldModelFloorHit {
    /// Returns the fraction along the supplied WMO-local segment.
    #[must_use]
    pub const fn fraction(self) -> f32 {
        self.fraction
    }

    /// Returns the original group MOPY/MOVI face index.
    #[must_use]
    pub const fn face(self) -> u16 {
        self.face
    }
}

/// Independently selected native primary (`0x08/0x20`) and fallback (`0x04/0x20`) floors.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WorldModelFloorHits {
    /// Nearest primary face, replacing the earlier face on equal distance.
    pub primary: Option<WorldModelFloorHit>,
    /// Nearest fallback face, replacing the earlier face on equal distance.
    pub fallback: Option<WorldModelFloorHit>,
}

#[derive(Default)]
pub(super) struct FloorProbeScratch {
    pending: Vec<NodeQuery>,
    visited: Vec<bool>,
}

#[derive(Clone, Copy)]
struct NodeQuery {
    node: usize,
    segment: [Vec3; 2],
    region: [Vec3; 2],
    depth: usize,
}

impl PlacedWorldModelCollision {
    /// Probes one admitted group's BSP in WMO-local coordinates.
    ///
    /// Each result has its own maximum fraction; stock registration initially
    /// uses 1.05 for both. Eligible cached leaves apply native segment outcodes,
    /// while uncached leaves ray-test beyond the endpoint up to those maxima.
    /// Repeated queries reuse traversal and face-visit storage. This method does
    /// not apply root/group admission or portals.
    ///
    /// # Errors
    /// Returns [`WorldModelCollisionError`] for invalid inputs or selected BSP.
    pub fn probe_group_floor(
        &mut self,
        group_index: usize,
        start: Vec3,
        end: Vec3,
        maximum_fractions: [f32; 2],
        cache_mode: MovementBspCacheMode,
    ) -> Result<WorldModelFloorHits, WorldModelCollisionError> {
        if !start.is_finite() || !end.is_finite() {
            return Err(WorldModelCollisionError::NonFiniteSegment);
        }
        if maximum_fractions.iter().any(|f| !f.is_finite() || *f < 0.0) {
            return Err(WorldModelCollisionError::InvalidMaximumFraction);
        }
        let group = self
            .model
            .groups()
            .get(group_index)
            .ok_or(WorldModelCollisionError::InvalidGroup { group_index })?;
        self.floor_probe.probe(
            group,
            &self.movement_cached_leaves[group_index],
            [start, end],
            maximum_fractions,
            cache_mode,
        )
    }
}

impl FloorProbeScratch {
    fn probe(
        &mut self,
        group: &DecodedWorldModelGroup,
        cached_leaves: &[bool],
        segment: [Vec3; 2],
        maximum: [f32; 2],
        cache_mode: MovementBspCacheMode,
    ) -> Result<WorldModelFloorHits, WorldModelCollisionError> {
        let mut hits = [None; 2];
        // 7C77D0 spills each subtraction, but retains the length and reciprocal
        // during normalization. The final fraction uses the stored reciprocal.
        let delta = (segment[1] - segment[0]).as_dvec3();
        let length = ((delta.x * delta.x + delta.y * delta.y) + delta.z * delta.z).sqrt();
        if length == 0.0 || group.bsp_nodes().is_empty() {
            return Ok(WorldModelFloorHits::default());
        }
        if !length.is_finite() {
            return Err(WorldModelCollisionError::NonFiniteSegment);
        }
        let inverse = length.recip();
        let direction = (delta * inverse).as_vec3();
        let inverse = inverse as f32;
        let mut nearest = maximum.map(|f| (length * f64::from(f)) as f32);
        let box_min = segment[0].min(segment[1]) - Vec3::splat(0.01);
        let box_max = segment[0].max(segment[1]) + Vec3::splat(0.01);
        self.visited.resize(group.polygons().len(), false);
        self.visited.fill(false);
        self.pending.clear();
        self.pending.push(NodeQuery {
            node: 0,
            segment,
            region: group.bounds().map(Vec3::from_array),
            depth: 0,
        });
        let mut selected = 0;
        while let Some(query) = self.pending.pop() {
            if query.depth >= group.bsp_nodes().len() {
                return Err(WorldModelCollisionError::InvalidBsp);
            }
            let node = group
                .bsp_nodes()
                .get(query.node)
                .ok_or(WorldModelCollisionError::InvalidBsp)?;
            if node.flags() & 4 != 0 {
                let cached =
                    cache_mode == MovementBspCacheMode::Enabled && cached_leaves[query.node];
                let first = node.first_face() as usize;
                for &face in &group.bsp_faces()[first..first + usize::from(node.face_count())] {
                    let index = usize::from(face);
                    let flags = group.polygons()[index].flags();
                    if self.visited[index] || flags & 0x82 != 0 || selected == 8192 {
                        continue;
                    }
                    self.visited[index] = true;
                    selected += 1;
                    let vertices = std::array::from_fn(|i| {
                        Vec3::from_array(
                            group.vertices()[usize::from(group.indices()[index * 3 + i])],
                        )
                    });
                    if cached
                        && (0..3).any(|axis| {
                            vertices.iter().all(|v| v[axis] < box_min[axis])
                                || vertices.iter().all(|v| v[axis] > box_max[axis])
                        })
                    {
                        continue;
                    }
                    let Some(distance) = triangle_distance(segment[0], direction, vertices) else {
                        continue;
                    };
                    if distance < 0.0 {
                        continue;
                    }
                    let channels = if flags & 0x20 != 0 {
                        [true, true]
                    } else if flags & 8 != 0 {
                        [true, false]
                    } else {
                        [false, flags & 4 != 0]
                    };
                    for channel in 0..2 {
                        if channels[channel] && distance <= nearest[channel] {
                            nearest[channel] = distance;
                            hits[channel] = Some(face);
                        }
                    }
                }
                continue;
            }
            let axis = usize::from(node.flags() & 3);
            if axis >= 3 {
                return Err(WorldModelCollisionError::InvalidBsp);
            }
            let [start, end] = query.segment.map(|v| f64::from(v[axis]));
            let lower = f64::from(query.region[0][axis]);
            let upper = f64::from(query.region[1][axis]);
            // These are intentionally asymmetric in original 7CA600.
            if (start - lower < -REGION_TOLERANCE && end - lower < -REGION_TOLERANCE)
                || (upper - start < REGION_TOLERANCE && upper - end < REGION_TOLERANCE)
            {
                continue;
            }
            let plane = node.plane_distance();
            let first = start - f64::from(plane);
            let last = end - f64::from(plane);
            let mut positive = query;
            positive.region[0][axis] = plane;
            let mut negative = query;
            negative.region[1][axis] = plane;
            let positive_child = node.positive_child();
            let negative_child = node.negative_child();
            if first.abs() <= REGION_TOLERANCE || last.abs() <= REGION_TOLERANCE {
                self.push(negative, negative_child)?;
                self.push(positive, positive_child)?;
            } else if first > REGION_TOLERANCE && last > REGION_TOLERANCE {
                self.push(positive, positive_child)?;
            } else if first < -REGION_TOLERANCE && last < -REGION_TOLERANCE {
                self.push(negative, negative_child)?;
            } else {
                let amount = f64::from((first / (first - last)) as f32);
                let start = query.segment[0].as_dvec3();
                let split = (start + (query.segment[1].as_dvec3() - start) * amount).as_vec3();
                if first as f32 > 0.0 {
                    negative.segment[0] = split;
                    positive.segment[1] = split;
                    self.push(negative, negative_child)?;
                    self.push(positive, positive_child)?;
                } else {
                    positive.segment[0] = split;
                    negative.segment[1] = split;
                    self.push(positive, positive_child)?;
                    self.push(negative, negative_child)?;
                }
            }
        }
        let hits = std::array::from_fn::<_, 2, _>(|i| {
            hits[i].map(|face| WorldModelFloorHit {
                face,
                fraction: ((f64::from(nearest[i]) * f64::from(inverse)) as f32).min(maximum[i]),
            })
        });
        Ok(WorldModelFloorHits {
            primary: hits[0],
            fallback: hits[1],
        })
    }

    fn push(&mut self, mut query: NodeQuery, child: i16) -> Result<(), WorldModelCollisionError> {
        if child == -1 {
            return Ok(());
        }
        query.node = usize::try_from(child).map_err(|_| WorldModelCollisionError::InvalidBsp)?;
        query.depth += 1;
        self.pending.push(query);
        Ok(())
    }
}

fn triangle_distance(start: Vec3, direction: Vec3, vertices: [Vec3; 3]) -> Option<f32> {
    let [a, b, c] = vertices.map(Vec3::as_dvec3);
    let first = b - a;
    let second = c - a;
    let direction = direction.as_dvec3();
    let mut cross = direction.cross(second);
    cross.x = f64::from(cross.x as f32);
    let determinant = (cross.z * first.z + cross.y * first.y) + cross.x * first.x;
    if determinant.abs() < f64::from(0.000_001_f32) {
        return None;
    }
    let inverse = f64::from(determinant.recip() as f32);
    let offset = start.as_dvec3() - a;
    let u = ((offset.z * cross.z + offset.y * cross.y) + offset.x * cross.x) * inverse;
    let tolerance = f64::from(0.002_f32);
    let maximum = f64::from((1.0 + tolerance) as f32);
    if u < -tolerance || u > maximum {
        return None;
    }
    let first = glam::DVec3::new(first.x, first.y, f64::from(first.z as f32));
    let cross = offset.cross(first);
    let v = ((direction.z * cross.z + direction.y * cross.y) + direction.x * cross.x) * inverse;
    if v < -tolerance || v + f64::from(u as f32) > maximum {
        return None;
    }
    let second = second.as_vec3().as_dvec3();
    Some((((cross.z * second.z + cross.y * second.y) + cross.x * second.x) * inverse) as f32)
}
