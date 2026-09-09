//! Moving-root list order, overlap callbacks and native camera-envelope gates.

use std::error::Error;

use glam::{Mat4, Vec3};
use solarity_ecs::{ActiveWorld, ObjectKind, WorldBootstrap, WorldMapId, WorldTransform};
use solarity_systems::{
    MovementBspCacheMode, WorldModelRegistrationKind, WorldModelRegistrationQuery,
};

use super::{
    MovementCollisionBounds, RuntimeWorldModelMovementOwner, UnitSceneAdmission,
    WorldSceneCameraFrame, WorldSceneDepthFrame, camera, floor_root, owner, registration,
    scene_root, unit_bounds, unit_query,
};

/// 792BD0 stops later moving roots only after the full-camera envelope filter.
#[test]
fn moving_depth_list_preserves_early_return_and_initial_envelope() -> Result<(), Box<dyn Error>> {
    let moving = moving_owner(90)?;
    let mut entrance = scene_root(0, 0, true)?;
    let mut query = unit_query()?;
    query.probe_root(
        moving,
        WorldModelRegistrationKind::Transformed,
        &mut entrance,
        MovementBspCacheMode::Enabled,
    )?;
    let selection = query.finish();
    let eye = Vec3::new(-30., -1., 1.);
    let camera = WorldSceneCameraFrame::perspective(
        eye,
        eye + Vec3::X,
        Vec3::X,
        Vec3::Z,
        1.,
        1.5,
        [0.1, 5000.],
    )?;
    let depth = WorldSceneDepthFrame::new(eye, eye + Vec3::X)?;
    let mut distant = floor_root(8, 8)?;
    for (first_owner, y, admitted) in [
        (moving_owner(91)?, 0., false),
        (moving_owner(91)?, 100000., true),
        (owner(18), 0., true),
    ] {
        distant.set_transform(Mat4::from_translation(Vec3::new(3000., y, 0.)))?;
        let mut scene = UnitSceneAdmission::default();
        for (root, owner) in [(&distant, first_owner), (&entrance, moving)] {
            if scene
                .record_outdoor_root(root, camera, owner, None, depth, [0., 0., 1., 1.])?
                .is_break()
            {
                break;
            }
        }
        assert_eq!(
            scene.admits_registration(selection, unit_bounds()?)?,
            admitted
        );
    }
    Ok(())
}

/// 799F80 uses 793270's exterior-enabled callback even with no outdoor window.
#[test]
fn indoor_moving_overlap_admits_exterior_registered_units() -> Result<(), Box<dyn Error>> {
    let primary = owner(17);
    let moving = moving_owner(90)?;
    let primary_root = floor_root(0, 0)?;
    let mut moving_root = floor_root(8, 8)?;
    let mut query = unit_query()?;
    query.probe_root(
        moving,
        WorldModelRegistrationKind::Transformed,
        &mut moving_root,
        MovementBspCacheMode::Enabled,
    )?;
    let selection = query.finish();
    assert!(
        !selection
            .selected()
            .ok_or("missing exterior floor")?
            .hit()
            .is_interior()
    );
    for overlap in [false, true] {
        let mut scene = UnitSceneAdmission::default();
        if overlap {
            scene.record_camera_root(&primary_root, camera()?, registration(primary), primary)?;
        }
        scene.record_overlap_root(&moving_root, camera()?, moving, primary)?;
        assert!(scene.outdoor.is_none());
        assert_eq!(
            scene.admits_registration(selection, unit_bounds()?)?,
            overlap
        );
    }
    Ok(())
}

/// Earlier group callbacks extend the visible set used by later overlap entries.
#[test]
fn moving_overlap_callbacks_admit_later_roots_in_native_order() -> Result<(), Box<dyn Error>> {
    let primary = owner(17);
    let first = moving_owner(90)?;
    let second = moving_owner(91)?;
    let primary_root = floor_root(0, 0)?;
    let mut first_root = scene_root(0, 0, true)?;
    let mut second_root = scene_root(0, 0, true)?;
    first_root.set_transform(Mat4::from_translation(Vec3::X * 15.))?;
    second_root.set_transform(Mat4::from_translation(Vec3::X * 40.))?;
    let start = Vec3::new(39., -1., 1.1);
    let mut query = WorldModelRegistrationQuery::new(start, start - Vec3::Z * 1000., start)?;
    query.probe_root(
        second,
        WorldModelRegistrationKind::Transformed,
        &mut second_root,
        MovementBspCacheMode::Enabled,
    )?;
    let selection = query.finish();
    assert_eq!(
        selection
            .selected()
            .ok_or("missing second root floor")?
            .hit()
            .group_index(),
        1
    );
    for forward in [false, true] {
        let mut scene = UnitSceneAdmission::default();
        scene.record_camera_root(&primary_root, camera()?, registration(primary), primary)?;
        let mut roots = [(&first_root, first), (&second_root, second)];
        if !forward {
            roots.reverse();
        }
        for (root, owner) in roots {
            scene.record_overlap_root(root, camera()?, owner, primary)?;
        }
        assert_eq!(
            scene.admits_registration(selection, unit_bounds()?)?,
            forward
        );
    }
    Ok(())
}

/// Original 799F80 executes its own overlap loop and cropped full-window planes.
#[test]
fn moving_overlap_gate_matches_original_client() -> Result<(), Box<dyn Error>> {
    let cameras = camera_fixtures()?;
    let mut count = 0;
    for line in include_str!("../fixtures/world_model_scene_overlap_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let fields: Vec<_> = line.split_whitespace().collect();
        let camera = cameras[fields[0].parse::<usize>()?];
        let coordinates = fields[1..13]
            .iter()
            .map(|word| float(word))
            .collect::<Result<Vec<_>, _>>()?;
        let bounds = MovementCollisionBounds::new(
            Vec3::from_slice(&coordinates[..3]),
            Vec3::from_slice(&coordinates[3..6]),
        )?;
        let mut scene = UnitSceneAdmission::default();
        if fields[13] == "1" {
            scene.visible_bounds.insert(
                (owner(1), 0),
                MovementCollisionBounds::new(
                    Vec3::from_slice(&coordinates[6..9]),
                    Vec3::from_slice(&coordinates[9..12]),
                )?,
            );
        }
        if fields[14] == "0" {
            scene.outdoor = Some(WorldSceneDepthFrame::new(Vec3::ZERO, Vec3::X)?);
        }
        assert_eq!(
            scene.overlap_bounds_visible(camera, bounds)?,
            fields[15] == "1",
            "native overlap case {count}"
        );
        count += 1;
    }
    assert_eq!(count, 960);
    Ok(())
}

/// Native scene insertion envelopes retain all six exact float stores.
#[test]
fn camera_envelopes_match_original_client() -> Result<(), Box<dyn Error>> {
    let cameras = camera_fixtures()?;
    let mut count = 0;
    for line in include_str!("../../../systems/tests/fixtures/world_scene_envelope_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let fields: Vec<_> = line.split_whitespace().collect();
        let bounds = cameras[fields[0].parse::<usize>()?].enclosing_bounds();
        for (actual, expected) in bounds
            .minimum()
            .to_array()
            .into_iter()
            .chain(bounds.maximum().to_array())
            .zip(&fields[1..])
        {
            assert_eq!(
                actual.to_bits(),
                float(expected)?.to_bits(),
                "native envelope case {count}"
            );
        }
        count += 1;
    }
    assert_eq!(count, 648);
    Ok(())
}

/// Uses the same native perspective inputs as the shared systems camera tests.
fn camera_fixtures() -> Result<Vec<WorldSceneCameraFrame>, Box<dyn Error>> {
    include_str!("../../../systems/tests/fixtures/world_scene_projection_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| {
            let values = line
                .split_whitespace()
                .take(16)
                .map(float)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(WorldSceneCameraFrame::perspective(
                Vec3::from_slice(&values[..3]),
                Vec3::from_slice(&values[3..6]),
                Vec3::from_slice(&values[6..9]),
                Vec3::from_slice(&values[9..12]),
                values[12],
                values[13],
                [values[14], values[15]],
            )?)
        })
        .collect()
}

/// Decodes an exact original float store.
fn float(word: &str) -> Result<f32, Box<dyn Error>> {
    Ok(f32::from_bits(u32::from_str_radix(word, 16)?))
}

/// Creates a real replicated object identity without exposing ECS internals.
fn moving_owner(guid: u64) -> Result<RuntimeWorldModelMovementOwner, Box<dyn Error>> {
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        1,
        "Fixture",
        Vec3::ZERO,
        0.,
    ));
    world.create_object(
        guid,
        ObjectKind::GameObject,
        Some(WorldTransform::new(Vec3::ZERO, 0.)),
        [],
    )?;
    Ok(RuntimeWorldModelMovementOwner::GameObject {
        identity: world
            .object_identity(guid)
            .ok_or("missing moving root identity")?,
    })
}
