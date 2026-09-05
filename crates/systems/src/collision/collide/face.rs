//! Shared extruded-edge planes used by body sweeps and support tests.

use glam::{DVec3, Vec3};

use super::{DEGENERATE_TOLERANCE, MovementCollisionPlane};

/// Builds one extrusion side through an authored edge, facing away from its
/// previous polygon vertex (`0x0075BC50` / `0x0075BE80`).
pub(super) fn extruded_edge_plane(
    origin: Vec3,
    next: Vec3,
    previous: Vec3,
    extrusion: Vec3,
) -> Option<MovementCollisionPlane> {
    // x87 retains the translated endpoint through subtraction, avoiding loss
    // of short travel when the origin is thousands of units away.
    let origin_extended = origin.as_dvec3();
    let edge_delta = next.as_dvec3() - origin_extended;
    let stored_edge = edge_delta.as_vec3().as_dvec3();
    let travel = (origin_extended + extrusion.as_dvec3()) - origin_extended;
    // Native edge differences spill before the Y/Z products; X still uses
    // extended differences. The boundary affects simultaneous contact order.
    let cross = DVec3::new(
        edge_delta.y * travel.z - edge_delta.z * travel.y,
        stored_edge.z * travel.x - stored_edge.x * travel.z,
        stored_edge.x * travel.y - stored_edge.y * travel.x,
    );
    let squared = cross.length_squared();
    if squared < f64::from(DEGENERATE_TOLERANCE) {
        return None;
    }
    let mut normal = cross / squared.sqrt();
    if normal.dot(previous.as_dvec3() - origin_extended) > 0.0 {
        normal = -normal;
    }
    Some(MovementCollisionPlane::through_extended(normal, origin))
}
