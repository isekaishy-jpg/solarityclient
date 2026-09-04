//! External checks for build-12340's shared transparent-element prefix.

use glam::{Mat4, Vec3};
use solarity_rendering::{
    M2ElementAlphaState, M2TransparentPass, M2TransparentSortKey, compare_m2_transparent,
    m2_model_distance_key, m2_section_distance_key,
};

/// `0x00832EA0 -> 0x00821A20` maps only raw `0x2000` to the late particle queue.
#[test]
fn project_particle_flag_selects_stock_pass_two_without_geometry_projection() {
    assert_eq!(
        M2TransparentPass::for_particle_flags(0),
        M2TransparentPass::One
    );
    assert_eq!(
        M2TransparentPass::for_particle_flags(0x0000_2000),
        M2TransparentPass::Two
    );
    assert!(M2TransparentPass::One < M2TransparentPass::Two);
}

/// `0x00821A20` keeps equality on the visible side of both alpha thresholds.
#[test]
fn stock_element_alpha_admission_preserves_exact_boundaries() {
    assert_eq!(
        M2ElementAlphaState::classify(0.000_099_99),
        M2ElementAlphaState::Hidden
    );
    assert_eq!(
        M2ElementAlphaState::classify(0.000_1),
        M2ElementAlphaState::Translucent
    );
    assert_eq!(
        M2ElementAlphaState::classify(0.999_989_9),
        M2ElementAlphaState::Translucent
    );
    assert_eq!(
        M2ElementAlphaState::classify(0.999_99),
        M2ElementAlphaState::Authored
    );
}

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

/// Particles and ribbons occupy the same sorted array as translucent meshes;
/// distance precedes plane, while the stock type discriminator resolves a tie.
#[test]
fn transparent_effects_share_the_stock_mesh_queue() {
    let mut keys = [
        (
            "near low-plane particle",
            M2TransparentSortKey::new(10.0, false, -20, 10.0, 4, 0).with_scene_element(4, 1),
        ),
        (
            "far high-plane mesh",
            M2TransparentSortKey::new(20.0, false, 20, 20.0, 4, 7).with_scene_element(0, 2),
        ),
        (
            "tied particle",
            M2TransparentSortKey::new(15.0, false, 0, 15.0, 4, 0).with_scene_element(4, 3),
        ),
        (
            "tied ribbon",
            M2TransparentSortKey::new(15.0, false, 0, 15.0, 4, 0).with_scene_element(3, 4),
        ),
        (
            "tied mesh",
            M2TransparentSortKey::new(15.0, false, 0, 15.0, 4, 7).with_scene_element(0, 5),
        ),
    ];
    keys.sort_unstable_by(|left, right| compare_m2_transparent(&left.1, &right.1));
    assert_eq!(
        keys.map(|(label, _key)| label),
        [
            "far high-plane mesh",
            "tied mesh",
            "tied ribbon",
            "tied particle",
            "near low-plane particle",
        ]
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

/// Multi-view instances retain the transformed model origin independently of
/// animated section centers so priority planes can order one shared model.
#[test]
fn model_sort_distance_uses_transformed_origin() {
    let model_view = Mat4::from_scale_rotation_translation(
        Vec3::splat(3.0),
        glam::Quat::from_rotation_z(0.5),
        Vec3::new(2.0, -3.0, 6.0),
    );
    assert_eq!(m2_model_distance_key(model_view), 49.0);
}
