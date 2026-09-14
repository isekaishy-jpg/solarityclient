//! Spatial selection and publication preserve native ordered consumers.

use super::{LEAF_SIZE, M2FrameWork, M2FrameWorkIndex, SceneryDistance, Vec3};
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
    for line in include_str!("fixtures/scenery_shadow_distance_native.txt")
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
    for row in include_str!("fixtures/scenery_distance_native.txt")
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
            assert_matches_resident_reference(&index, &scenery, &lights, camera, detail, shadows);
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
impl M2FrameWork {
    pub(in crate::application::terrain_frame::m2) fn diagnostic_counts(&self) -> (usize, usize) {
        (self.cursor, self.static_distance_tests)
    }
}
