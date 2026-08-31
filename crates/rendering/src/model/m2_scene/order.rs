//! Build-12340 transparent M2 scene-element ordering.

use std::cmp::Ordering;

use glam::{Mat4, Vec3};

/// Squared-length normalization guard at build-12340 address `0x009EA27C`.
const STOCK_SORT_DIRECTION_EPSILON: f32 = 2.384_185_8e-7;

/// The stock fields shared by transparent mesh elements in passes one and two.
///
/// Meshes, ribbons, particles, and callbacks ultimately share this prefix. The
/// producer-specific suffix belongs to the eventual unified world scene queue;
/// this key closes the mesh-only boundary without inventing a stable FIFO tie.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2TransparentSortKey {
    primary_distance: f32,
    alternate_copy: bool,
    priority_plane: i8,
    secondary_distance: f32,
    instance_identity: usize,
    material_layer: u16,
}

impl M2TransparentSortKey {
    /// Captures the exact common comparator fields of one transparent mesh.
    ///
    /// Distances may be negative for the stock radius-adjusted branch behind
    /// the view origin, but decoded and sampled inputs guarantee finiteness.
    #[must_use]
    pub const fn new(
        primary_distance: f32,
        alternate_copy: bool,
        priority_plane: i8,
        secondary_distance: f32,
        instance_identity: usize,
        material_layer: u16,
    ) -> Self {
        Self {
            primary_distance,
            alternate_copy,
            priority_plane,
            secondary_distance,
            instance_identity,
            material_layer,
        }
    }
}

/// Replays the common prefix of build 12340's pass-one/pass-two comparator.
///
/// The executable calls an in-place heapsort, so equality deliberately remains
/// equality; callers must use an unstable sort and must not promise FIFO order.
#[must_use]
pub fn compare_m2_transparent(
    left: &M2TransparentSortKey,
    right: &M2TransparentSortKey,
) -> Ordering {
    right
        .primary_distance
        .partial_cmp(&left.primary_distance)
        .unwrap_or(Ordering::Equal)
        .then_with(|| right.alternate_copy.cmp(&left.alternate_copy))
        .then_with(|| left.priority_plane.cmp(&right.priority_plane))
        .then_with(|| {
            right
                .secondary_distance
                .partial_cmp(&left.secondary_distance)
                .unwrap_or(Ordering::Equal)
        })
        .then_with(|| left.instance_identity.cmp(&right.instance_identity))
        .then_with(|| left.material_layer.cmp(&right.material_layer))
}

/// Computes the animated section-center distance used by M2 mesh elements.
///
/// Flags `0x1` and `0x2` select the near and far edge of the transformed sort
/// sphere. Those two branches retain the view-space Z sign after squaring.
#[must_use]
pub fn m2_section_distance_key(
    sort_center: Vec3,
    sort_radius: f32,
    batch_flags: u8,
    view_transform: Mat4,
) -> f32 {
    let mut center = view_transform.transform_point3(sort_center);
    if batch_flags & 0x3 != 0 {
        let distance_squared = center.length_squared();
        if distance_squared > STOCK_SORT_DIRECTION_EPSILON {
            let radius = sort_radius * view_transform.x_axis.truncate().length();
            let radius_offset = center * (radius / distance_squared.sqrt());
            if batch_flags & 0x1 != 0 {
                center -= radius_offset;
            } else {
                center += radius_offset;
            }
        }
        let distance_squared = center.length_squared();
        return if center.z < 0.0 {
            -distance_squared
        } else {
            distance_squared
        };
    }
    center.length_squared()
}
