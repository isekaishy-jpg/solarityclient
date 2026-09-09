//! Native scene AABB acceptance, including the near face and stored tolerance.

use glam::Vec3;

use crate::collision::MovementCollisionBounds;

/// 983E70 mirrors the stored far normal and anchors it on near corner two.
pub(super) fn near_plane(far: [f32; 4], corner: Vec3) -> [f32; 4] {
    let [x, y, z] = [-far[0], -far[1], -far[2]];
    let offset = -((f64::from(z) * f64::from(corner.z) + f64::from(y) * f64::from(corner.y))
        + f64::from(x) * f64::from(corner.x));
    [x, y, z, offset as f32]
}

/// 9839E0 chooses the supporting corner from coefficient sign bits, including
/// negative zero, then compares the extended dot product against AA2E74.
pub(super) fn intersects(planes: &[[f32; 4]; 6], bounds: MovementCollisionBounds) -> bool {
    let corners = [bounds.maximum().to_array(), bounds.minimum().to_array()];
    planes.iter().all(|plane| {
        let coordinate =
            |axis: usize| f64::from(corners[usize::from(plane[axis].is_sign_negative())][axis]);
        let [x, y, z, offset] = plane.map(f64::from);
        let distance = ((coordinate(2) * z + coordinate(1) * y) + coordinate(0) * x) + offset;
        distance >= f64::from(f32::from_bits(0xbc9f_49f4))
    })
}
