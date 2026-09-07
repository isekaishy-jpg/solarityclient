//! Root/group admission and floor/portal precedence (`0x007C25D0/0x007C1DC0`).

use glam::Vec3;

use super::{
    MovementBspCacheMode, MovementCollectionError, MovementCollisionBounds,
    PlacedWorldModelCollision, WorldModelCollisionError, movement_collection::transform_point,
    probe_world_model_portals,
};

/// Native placed-WMO classification, independent of WDT global-WMO ownership.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldModelRegistrationKind {
    /// Initial static placement; exterior groups do not require point containment.
    Static,
    /// A root updated through the native transform setter (`CMapObj +0x0C & 0x400`).
    Transformed,
}

/// Selected group and authored floor, or a portal-selected group without a face.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldModelRegistrationHit {
    group_index: usize,
    fraction: f32,
    face: Option<u16>,
    interior: bool,
}

impl WorldModelRegistrationHit {
    pub(super) const fn without_face(mut self) -> Self {
        self.face = None;
        self
    }
    /// Returns the selected root group index, which can be a portal neighbor.
    #[must_use]
    pub const fn group_index(self) -> usize {
        self.group_index
    }

    /// Returns the fraction along the supplied segment.
    #[must_use]
    pub const fn fraction(self) -> f32 {
        self.fraction
    }

    /// Returns the authored face, or none for a portal-selected result.
    #[must_use]
    pub const fn face(self) -> Option<u16> {
        self.face
    }

    /// Returns the selected loaded group's MOGP interior classification.
    #[must_use]
    pub const fn is_interior(self) -> bool {
        self.interior
    }
}

/// Independent primary and fallback candidates before cross-root bank resolution.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WorldModelRegistrationHits {
    /// Primary floor or qualifying portal candidate.
    pub primary: Option<WorldModelRegistrationHit>,
    /// Fallback floor candidate; portals do not replace this channel.
    pub fallback: Option<WorldModelRegistrationHit>,
}

impl PlacedWorldModelCollision {
    /// Appends native dynamic-reference destinations in this complete root.
    ///
    /// Exterior registration (`0x007C2BF0`) selects overlapping exterior MOGI
    /// groups. Interior registration (`0x007C2D30`) inserts the chosen group
    /// first, then other overlapping interior MOGI groups in root index order.
    /// Both paths first transform the object's render box and test MOHD bounds.
    /// The scene resolves excluded roots and primary-root identity beforehand.
    ///
    /// # Errors
    /// Returns [`MovementCollectionError`] for invalid transformed bounds.
    pub fn append_registration_groups(
        &self,
        render_bounds: MovementCollisionBounds,
        interior_group: Option<usize>,
        output: &mut Vec<usize>,
    ) -> Result<(), MovementCollectionError> {
        let local = render_bounds.transformed(self.inverse_transform)?;
        let root = self.model.bounds();
        if !local.intersects(MovementCollisionBounds::new(
            Vec3::from_array(root[0]),
            Vec3::from_array(root[1]),
        )?) {
            return Ok(());
        }
        if let Some(group) = interior_group {
            if group >= self.model.groups().len() {
                return Err(MovementCollectionError::InvalidRegistrationGroup { group });
            }
            output.push(group);
        }
        for (group, info) in self.model.group_info().iter().enumerate() {
            if Some(group) == interior_group || info.flags() & 0x410080 != 0 {
                continue;
            }
            if (info.flags() & 8 != 0) != interior_group.is_none() {
                continue;
            }
            let bounds = info.bounds();
            if local.intersects(MovementCollisionBounds::new(
                Vec3::from_array(bounds[0]),
                Vec3::from_array(bounds[1]),
            )?) {
                output.push(group);
            }
        }
        Ok(())
    }

    /// Probes a complete placed root for dynamic-object spatial registration.
    ///
    /// Inputs are world-space. Root/group boxes use native segment admission;
    /// root MOGI flags gate groups and containment, while loaded MOGP flags
    /// classify floor/portal results. Groups run in root index order. Each
    /// channel carries its own maximum fraction across groups. Interior portals
    /// independently start at 1.05 and can replace a primary floor that is less
    /// than 0.0001 closer. Cross-root preference and terrain resolution belong to
    /// the spatial scene that owns these candidates.
    ///
    /// # Errors
    /// Returns [`WorldModelCollisionError`] for invalid inputs or selected BSP.
    #[allow(clippy::too_many_arguments)]
    pub fn probe_registration(
        &mut self,
        start: Vec3,
        end: Vec3,
        containment_point: Vec3,
        kind: WorldModelRegistrationKind,
        maximum_fractions: [f32; 2],
        cache_mode: MovementBspCacheMode,
    ) -> Result<WorldModelRegistrationHits, WorldModelCollisionError> {
        if !start.is_finite() || !end.is_finite() || !containment_point.is_finite() {
            return Err(WorldModelCollisionError::NonFiniteSegment);
        }
        if maximum_fractions.iter().any(|f| !f.is_finite() || *f < 0.0) {
            return Err(WorldModelCollisionError::InvalidMaximumFraction);
        }
        let mut result = WorldModelRegistrationHits::default();
        if !segment_intersects_box(self.root_bounds, start, end) {
            return Ok(result);
        }
        let start = transform_point(self.inverse_transform, start);
        let end = transform_point(self.inverse_transform, end);
        let point = transform_point(self.inverse_transform, containment_point);
        if !start.is_finite() || !end.is_finite() || !point.is_finite() {
            return Err(WorldModelCollisionError::NonFiniteSegment);
        }
        let mut maximum = maximum_fractions;
        for group_index in 0..self.model.groups().len() {
            let info = self.model.group_info()[group_index];
            let bounds = info.bounds().map(Vec3::from_array);
            if info.flags() & 0x410080 != 0 || !segment_intersects_box(bounds, start, end) {
                continue;
            }
            if (kind == WorldModelRegistrationKind::Transformed || info.flags() & 8 == 0)
                && (0..3).any(|axis| point[axis] < bounds[0][axis] || point[axis] > bounds[1][axis])
            {
                continue;
            }
            let interior = self.model.groups()[group_index].flags() & 8 == 0;
            let floors = self.probe_group_floor(group_index, start, end, maximum, cache_mode)?;
            for (index, (floor, target)) in [
                (floors.primary, &mut result.primary),
                (floors.fallback, &mut result.fallback),
            ]
            .into_iter()
            .enumerate()
            {
                if let Some(floor) = floor {
                    maximum[index] = floor.fraction();
                    *target = Some(WorldModelRegistrationHit {
                        group_index,
                        fraction: floor.fraction(),
                        face: Some(floor.face()),
                        interior,
                    });
                }
            }
            if interior
                && let Some(portal) =
                    probe_world_model_portals(&self.model, group_index, start, end, 1.05)?
                && f64::from(portal.fraction()) - f64::from(maximum[0]) < f64::from(0.0001_f32)
            {
                let group_index = portal.source_group();
                maximum[0] = portal.fraction();
                result.primary = Some(WorldModelRegistrationHit {
                    group_index,
                    fraction: portal.fraction(),
                    face: None,
                    interior: self.model.groups()[group_index].flags() & 8 == 0,
                });
            }
        }
        Ok(result)
    }
}

/// Original `0x007F9480`: first outside axis with the largest stored fraction.
pub(super) fn segment_intersects_box(bounds: [Vec3; 2], start: Vec3, end: Vec3) -> bool {
    let delta = end - start;
    let mut candidates = [-1.0_f32; 3];
    let mut outside = [false; 3];
    for axis in 0..3 {
        let boundary = if start[axis] < bounds[0][axis] {
            if end[axis] < bounds[0][axis] {
                return false;
            }
            bounds[0][axis]
        } else if start[axis] > bounds[1][axis] {
            if end[axis] > bounds[1][axis] {
                return false;
            }
            bounds[1][axis]
        } else {
            continue;
        };
        outside[axis] = true;
        candidates[axis] =
            ((f64::from(boundary) - f64::from(start[axis])) / f64::from(delta[axis])) as f32;
    }
    if !outside.into_iter().any(|v| v) {
        return true;
    }
    let mut selected = 0;
    for axis in 1..3 {
        if candidates[axis] > candidates[selected] {
            selected = axis;
        }
    }
    let fraction = candidates[selected];
    if fraction.is_sign_negative() {
        return false;
    }
    let tolerance = f64::from(0.000_01_f32);
    (0..3).all(|axis| {
        let point = f64::from(delta[axis]) * f64::from(fraction) + f64::from(start[axis]);
        axis == selected
            || (point >= f64::from(bounds[0][axis]) - tolerance
                && point <= f64::from(bounds[1][axis]) + tolerance)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_box_matches_original_corner_tolerances_and_direction()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut count = 0;
        for line in include_str!("../../tests/fixtures/wmo-segment-box-native.txt")
            .lines()
            .filter(|line| !line.starts_with('#'))
        {
            let words = line.split_whitespace().collect::<Vec<_>>();
            let values = words[..12]
                .iter()
                .map(|word| u32::from_str_radix(word, 16).map(f32::from_bits))
                .collect::<Result<Vec<_>, _>>()?;
            assert_eq!(
                segment_intersects_box(
                    [
                        Vec3::from_slice(&values[..3]),
                        Vec3::from_slice(&values[3..6])
                    ],
                    Vec3::from_slice(&values[6..9]),
                    Vec3::from_slice(&values[9..12]),
                ),
                words[12] == "1",
                "{line}"
            );
            count += 1;
        }
        assert_eq!(count, 298);
        Ok(())
    }
}
