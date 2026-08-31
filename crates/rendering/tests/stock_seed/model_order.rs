//! External checks for build-12340's shared transparent-element prefix.

use glam::{Mat4, Vec3};
use solarity_rendering::{M2TransparentSortKey, compare_m2_transparent, m2_section_distance_key};

/// Distance remains the first key; the remaining fields resolve exact ties in
/// alternate-copy, plane, secondary-distance, instance, then layer order.
#[test]
fn transparent_meshes_follow_stock_comparator_hierarchy() {
    let mut keys = [
        (
            "near",
            M2TransparentSortKey::new(10.0, false, -5, 100.0, 0, 0),
        ),
        (
            "far ordinary",
            M2TransparentSortKey::new(20.0, false, -5, 100.0, 0, 0),
        ),
        (
            "far alternate",
            M2TransparentSortKey::new(20.0, true, 5, 0.0, 8, 9),
        ),
        (
            "far low plane",
            M2TransparentSortKey::new(20.0, false, -6, 0.0, 8, 9),
        ),
        (
            "far secondary",
            M2TransparentSortKey::new(20.0, false, -5, 200.0, 8, 9),
        ),
        (
            "far first instance",
            M2TransparentSortKey::new(20.0, false, -5, 100.0, 1, 9),
        ),
        (
            "far first layer",
            M2TransparentSortKey::new(20.0, false, -5, 100.0, 8, 1),
        ),
        (
            "far later layer",
            M2TransparentSortKey::new(20.0, false, -5, 100.0, 8, 9),
        ),
    ];
    keys.sort_unstable_by(|left, right| compare_m2_transparent(&left.1, &right.1));
    assert_eq!(
        keys.map(|(label, _key)| label),
        [
            "far alternate",
            "far low plane",
            "far secondary",
            "far ordinary",
            "far first instance",
            "far first layer",
            "far later layer",
            "near",
        ]
    );
}

/// Equal common keys remain equal because stock's heapsort does not promise
/// insertion order and producer-specific queues own any later comparisons.
#[test]
fn transparent_common_key_does_not_invent_a_fifo_tie() {
    let left = M2TransparentSortKey::new(25.0, false, 0, 25.0, 4, 0);
    let right = M2TransparentSortKey::new(25.0, false, 0, 25.0, 4, 0);
    assert_eq!(
        compare_m2_transparent(&left, &right),
        std::cmp::Ordering::Equal
    );
}

/// Radius flags move along the transformed center ray and retain its Z sign.
#[test]
fn section_sort_sphere_uses_stock_near_far_and_signed_keys() {
    let in_front = Mat4::from_translation(Vec3::new(0.0, 0.0, 10.0));
    assert_eq!(m2_section_distance_key(Vec3::ZERO, 2.0, 0, in_front), 100.0);
    assert_eq!(
        m2_section_distance_key(Vec3::ZERO, 2.0, 0x1, in_front),
        64.0
    );
    assert_eq!(
        m2_section_distance_key(Vec3::ZERO, 2.0, 0x2, in_front),
        144.0
    );

    let behind = Mat4::from_translation(Vec3::new(0.0, 0.0, -10.0));
    assert_eq!(m2_section_distance_key(Vec3::ZERO, 2.0, 0x1, behind), -64.0);
    assert_eq!(
        m2_section_distance_key(Vec3::ZERO, 2.0, 0x2, behind),
        -144.0
    );
}
