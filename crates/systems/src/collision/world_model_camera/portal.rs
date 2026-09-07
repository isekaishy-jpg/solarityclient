//! Root-wide camera portal traversal (`7AF280`, mode zero).

use crate::collision::{
    WorldModelPortalHit,
    world_model_portal::{plane_hit, polygon_contains},
    world_model_registration::segment_intersects_box,
};
use glam::Vec3;
use solarity_asset::DecodedWorldModel;

/// The root query carries one distance through every loaded group's MOPR list.
pub(super) fn probe(
    model: &DecodedWorldModel,
    start: Vec3,
    end: Vec3,
) -> Option<WorldModelPortalHit> {
    let delta = end.as_dvec3() - start.as_dvec3();
    let length = ((delta.z * delta.z + delta.y * delta.y) + delta.x * delta.x).sqrt();
    if length == 0.0 {
        return None;
    }
    let inverse = length.recip();
    let direction = (delta * inverse).as_vec3();
    let inverse = inverse as f32;
    let mut nearest = (length * f64::from(1.05_f32)) as f32;
    let mut hit = None;
    for (index, group) in model.groups().iter().enumerate() {
        if !segment_intersects_box(
            model.group_info()[index].bounds().map(Vec3::from_array),
            start,
            end,
        ) {
            continue;
        }
        let first = usize::from(group.portal_reference_start());
        let count = usize::from(group.portal_reference_count());
        for reference in &model.portal_references()[first..first + count] {
            let portal = model.portals()[usize::from(reference.portal_index())];
            let normal = Vec3::from_array(portal.normal());
            let Some((distance, point)) = plane_hit(start, direction, normal, portal.distance())
            else {
                continue;
            };
            if distance < 0.0 || distance > nearest {
                continue;
            }
            let first = usize::from(portal.vertex_start());
            let count = usize::from(portal.vertex_count());
            if !polygon_contains(
                point,
                &model.portal_vertices()[first..first + count],
                normal,
            ) {
                continue;
            }
            nearest = distance;
            let normal = normal.as_dvec3();
            let start = start.as_dvec3();
            let signed = ((normal.y * start.y + normal.z * start.z) + normal.x * start.x)
                + f64::from(portal.distance());
            let adjacent = usize::from(reference.group_index());
            let (source_group, destination_group) = if (signed < 0.0) == (reference.side() > 0) {
                (adjacent, index)
            } else {
                (index, adjacent)
            };
            hit = Some(WorldModelPortalHit {
                fraction: (f64::from(nearest) * f64::from(inverse)) as f32,
                source_group,
                destination_group,
            });
        }
    }
    hit
}
