//! Native exterior-window entry into decoded WMO group graphs.

use std::{error::Error, sync::Arc};

use glam::{Mat4, Vec3};
use solarity_systems::{
    PlacedWorldModelCollision, WorldModelCameraSceneQuery, WorldModelVisibilityError,
    WorldModelVisibilityQuery, WorldSceneCameraFrame,
};

use super::world_model_visibility::visibility_model;

/// Reusing scene query storage must not reuse entry clips, recursion,
/// fog writes or exterior banks. Compare every call with a fresh query while
/// changing the admitted generation, both placement matrices and the camera.
#[test]
fn repeated_scene_queries_preserve_independent_projection_and_visits() -> Result<(), Box<dyn Error>>
{
    let mut retained = WorldModelCameraSceneQuery::default();
    for portal_height in [0., 2., 0.] {
        let model = Arc::new(visibility_model(
            &[0, 8, 8],
            &[0, 8, 8],
            &[[0, 1], [0, 2]],
            &[[0., 0., 1., portal_height], [0., 0., 1., -3.]],
        )?);
        let mut root = PlacedWorldModelCollision::prepare_transform(model, Mat4::IDENTITY)?;
        for transform in [
            Mat4::IDENTITY,
            Mat4::from_translation(Vec3::new(2., -1., 0.5))
                * Mat4::from_rotation_y(0.15)
                * Mat4::from_scale(Vec3::splat(1.2)),
            Mat4::IDENTITY,
        ] {
            root.set_transform(transform)?;
            for (eye, aspect) in [
                (Vec3::new(0., 0., 6.), 1.),
                (Vec3::new(1., -2., 8.), 1.5),
                (Vec3::new(0., 0., 6.), 1.),
            ] {
                let camera = WorldSceneCameraFrame::perspective(
                    eye,
                    Vec3::ZERO,
                    -eye.normalize(),
                    Vec3::Y,
                    0.9424778,
                    aspect,
                    [0.2, 100.],
                )?;
                for _ in 0..2 {
                    let mut fresh = WorldModelCameraSceneQuery::default();
                    retained.query_camera_root(&root, camera, &[0])?;
                    fresh.query_camera_root(&root, camera, &[0])?;
                    assert_scene_queries_match(&retained, &fresh);
                    for (group, window) in [
                        (1, [0., 0., 1., 1.]),
                        (2, [0.1234567, 0.2345678, 0.8123456, 0.9123456]),
                        (1, [3., 3., 4., 4.]),
                        (2, [0., 0., 1., 1.]),
                    ] {
                        let mut fresh = WorldModelCameraSceneQuery::default();
                        retained.query_outdoor_group(&root, camera, group, window)?;
                        fresh.query_outdoor_group(&root, camera, group, window)?;
                        assert_scene_queries_match(&retained, &fresh);
                    }
                }
            }
        }
    }
    Ok(())
}

fn assert_scene_queries_match(
    left: &WorldModelCameraSceneQuery,
    right: &WorldModelCameraSceneQuery,
) {
    assert_eq!(left.groups(), right.groups());
    assert_eq!(left.exterior_window(), right.exterior_window());
    assert_eq!(left.sky_window(), right.sky_window());
    assert_eq!(left.has_skybox_request(), right.has_skybox_request());
    assert_eq!(left.visits().len(), right.visits().len());
    for (left, right) in left.visits().iter().zip(right.visits()) {
        assert_eq!(left.group, right.group);
        assert_eq!(left.fog, right.fog);
        super::world_model_visibility::assert_clip_bits(left.frustum, right.frustum);
    }
}

/// Eager projection and native traversal remain independent references for
/// demand-driven polygon evaluation through branches and repeated entries.
#[test]
fn scene_projection_matches_eager_portals_through_recursive_groups() -> Result<(), Box<dyn Error>> {
    use solarity_systems::{
        WorldModelPortalProjector, WorldModelSceneFog, WorldModelSceneVisibilityEvent,
    };
    let model = Arc::new(visibility_model(
        &[8, 0, 0, 8],
        &[8, 0, 0, 8],
        &[[0, 1], [1, 2], [2, 3], [0, 2]],
        &[
            [0., 0., 1., 0.],
            [0., 0., 1., -1.],
            [0., 0., 1., -2.],
            [0., 0., 1., -1.5],
        ],
    )?);
    let mut root =
        PlacedWorldModelCollision::prepare_transform(Arc::clone(&model), Mat4::IDENTITY)?;
    let mut lazy = WorldModelCameraSceneQuery::default();
    let mut eager = WorldModelVisibilityQuery::default();
    let mut projector = WorldModelPortalProjector::default();
    let mut recursive_visits = 0;
    for transform in [Mat4::IDENTITY, Mat4::from_rotation_y(0.2), Mat4::IDENTITY] {
        root.set_transform(transform)?;
        for eye in [
            Vec3::new(0., 0., 6.),
            Vec3::new(1., -2., 8.),
            Vec3::new(0., 0., -6.),
        ] {
            let camera = WorldSceneCameraFrame::perspective(
                eye,
                Vec3::ZERO,
                -eye.normalize(),
                Vec3::Y,
                0.9424778,
                1.3,
                [0.2, 100.],
            )?;
            let frame = camera.for_root(transform, root.inverse_transform())?;
            let projected = projector.project(&model, frame)?;
            for initial in [&[1][..], &[2, 1][..], &[][..], &[1, 1][..]] {
                lazy.query_camera_root(&root, camera, initial)?;
                let expected = eager
                    .query_scene(&model, frame.local_camera, initial, 10, projected)?
                    .iter()
                    .filter_map(|event| match event {
                        WorldModelSceneVisibilityEvent::Group(visit) => Some(*visit),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(lazy.visits().len(), expected.len());
                for (actual, expected) in lazy.visits().iter().zip(expected) {
                    recursive_visits += usize::from(expected.depth > 0);
                    assert_eq!(actual.group, expected.group);
                    assert_eq!(
                        actual.fog,
                        if expected.indoor_fog {
                            WorldModelSceneFog::Indoor
                        } else {
                            WorldModelSceneFog::Outdoor
                        }
                    );
                    super::world_model_visibility::assert_clip_bits(
                        actual.frustum,
                        expected.frustum(camera, camera.frustum())?,
                    );
                }
            }
            for window in [
                [0., 0., 1., 1.],
                [0.1234567, 0.2345678, 0.8123456, 0.9123456],
            ] {
                for group in [0, 3, 0] {
                    lazy.query_outdoor_group(&root, camera, group, window)?;
                    let clip = window.map(|value| (f64::from(value) * 2. - 1.) as f32);
                    let expected = eager.query_outdoor(
                        &model,
                        frame.local_camera,
                        group,
                        10,
                        clip,
                        projected,
                    )?;
                    assert_eq!(lazy.visits().len(), expected.len());
                    for (actual, expected) in lazy.visits().iter().zip(expected) {
                        recursive_visits += usize::from(expected.depth > 0);
                        assert_eq!(actual.group, expected.group);
                        assert_eq!(
                            actual.fog,
                            if expected.indoor_fog {
                                WorldModelSceneFog::Indoor
                            } else {
                                WorldModelSceneFog::Outdoor
                            }
                        );
                        super::world_model_visibility::assert_clip_bits(
                            actual.frustum,
                            expected.frustum(camera, camera.frustum_for_window(window)?)?,
                        );
                    }
                }
            }
        }
    }
    assert!(
        recursive_visits > 0,
        "fixture must exercise recursive portal clips"
    );
    Ok(())
}

/// Decodes exact original float stores without decimal conversion.
fn float(word: &str) -> Result<f32, Box<dyn Error>> {
    Ok(f32::from_bits(u32::from_str_radix(word, 16)?))
}

/// Actual 7AC060 runs with 7AD350's outdoor mode and caller-supplied window.
#[test]
fn outdoor_entry_matches_original_visits_fog_and_inherited_windows() -> Result<(), Box<dyn Error>> {
    let mut lines = include_str!("../fixtures/world_model_outdoor_visibility_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .peekable();
    let mut count = 0;
    let mut query = WorldModelVisibilityQuery::default();
    while let Some(line) = lines.next() {
        let row = line.split_ascii_whitespace().collect::<Vec<_>>();
        assert_eq!(row[0], "scene");
        let size: usize = row[1].parse()?;
        let flags = row[2..2 + size]
            .iter()
            .map(|v| v.parse::<u32>())
            .collect::<Result<Vec<_>, _>>()?;
        let info = row[2 + size..2 + size * 2]
            .iter()
            .map(|v| v.parse::<u32>())
            .collect::<Result<Vec<_>, _>>()?;
        let edge_count: usize = row[2 + size * 2].parse()?;
        let mut edges = Vec::new();
        let mut planes = Vec::new();
        let mut windows = Vec::new();
        for _ in 0..edge_count {
            let edge = lines
                .next()
                .ok_or("missing edge")?
                .split_ascii_whitespace()
                .collect::<Vec<_>>();
            edges.push([edge[1].parse()?, edge[2].parse()?]);
            let plane = edge[3..7]
                .iter()
                .map(|v| float(v))
                .collect::<Result<Vec<_>, _>>()?;
            planes.push(plane.try_into().map_err(|_| "invalid plane count")?);
            let window = edge[7..11]
                .iter()
                .map(|v| float(v))
                .collect::<Result<Vec<_>, _>>()?;
            windows.push(Some(window.try_into().map_err(|_| "invalid window count")?));
        }
        let model = visibility_model(&flags, &info, &edges, &planes)?;
        while lines.peek().is_some_and(|line| line.starts_with("query ")) {
            let line = lines.next().ok_or("missing query")?;
            let row = line.split_ascii_whitespace().collect::<Vec<_>>();
            let screen_window = [
                float(row[6])?,
                float(row[7])?,
                float(row[8])?,
                float(row[9])?,
            ];
            let visits = query.query_outdoor(
                &model,
                Vec3::new(float(row[3])?, float(row[4])?, float(row[5])?),
                row[1].parse()?,
                row[2].parse()?,
                screen_window,
                &windows,
            )?;
            assert_eq!(visits.len(), row[10].parse::<usize>()?, "{line}, {flags:?}");
            for visit in visits {
                let expected = lines
                    .next()
                    .ok_or("missing visit")?
                    .split_ascii_whitespace()
                    .collect::<Vec<_>>();
                assert_eq!(expected[0], "visit");
                assert_eq!(visit.group, expected[1].parse::<usize>()?, "{line}");
                assert_eq!(visit.indoor_fog, expected[2] == "1", "{line}");
                assert_eq!(visit.depth, expected[3].parse::<u32>()?, "{line}");
                for (actual, expected) in visit.screen_window.iter().zip(&expected[4..8]) {
                    assert_eq!(
                        actual.to_bits(),
                        u32::from_str_radix(expected, 16)?,
                        "{line}"
                    );
                }
            }
            count += 1;
        }
        assert_eq!(
            query.query_outdoor(&model, Vec3::ZERO, size, 10, [-1., -1., 1., 1.], &windows),
            Err(WorldModelVisibilityError::InvalidGroup)
        );
        assert_eq!(
            query.query_outdoor(&model, Vec3::ZERO, 0, 10, [0.; 4], &windows),
            Err(WorldModelVisibilityError::DegenerateFrustum)
        );
        assert_eq!(
            query.query_outdoor(&model, Vec3::ZERO, 0, 10, [f32::NAN; 4], &windows),
            Err(WorldModelVisibilityError::NonFiniteCoordinates)
        );
    }
    assert_eq!(count, 420);
    Ok(())
}

/// 7B3A10 reads MOGI flags before choosing direct callbacks or full projection.
#[test]
fn exterior_group_entry_combines_flags_bounds_and_portal_visibility() -> Result<(), Box<dyn Error>>
{
    let mut query = WorldModelCameraSceneQuery::default();
    for (loaded, info, expected) in [
        (0, 0, vec![]),
        (8, 8, vec![0, 1]),
        (0, 8, vec![0, 1]),
        (0x10000, 8, vec![]),
        (0, 0x10000, vec![0]),
        (8, 0x10008, vec![0]),
    ] {
        let model = Arc::new(visibility_model(
            &[loaded, 0],
            &[info, 0],
            &[[0, 1]],
            &[[0., 0., 1., 0.]],
        )?);
        let mut root = PlacedWorldModelCollision::prepare_transform(model, Mat4::IDENTITY)?;
        let eye = Vec3::new(0., 0., 3.);
        let camera = WorldSceneCameraFrame::perspective(
            eye,
            Vec3::ZERO,
            -Vec3::Z,
            Vec3::Y,
            0.9424778,
            1.,
            [0.2, 100.],
        )?;
        assert_eq!(
            query.query_outdoor_group(&root, camera, 0, [0., 0., 1., 1.])?,
            expected
        );
        assert_eq!(
            query
                .visits()
                .iter()
                .map(|visit| visit.group)
                .collect::<Vec<_>>(),
            expected
        );
        if let Some(visit) = query.visits().first() {
            assert_eq!(
                visit.fog,
                if info & 0x10000 != 0 {
                    solarity_systems::WorldModelSceneFog::Inherited
                } else {
                    solarity_systems::WorldModelSceneFog::Outdoor
                }
            );
            super::world_model_visibility::assert_clip_bits(
                visit.frustum,
                camera.frustum_for_window([0., 0., 1., 1.])?,
            );
        }
        // Irregular float values distinguish the exact inherited crop from
        // round-tripping through the recursive clip coordinate conversion.
        let window = [0.1234567, 0.2345678, 0.8123456, 0.9123456];
        query.query_outdoor_group(&root, camera, 0, window)?;
        if let Some(visit) = query.visits().first() {
            super::world_model_visibility::assert_clip_bits(
                visit.frustum,
                camera.frustum_for_window(window)?,
            );
        }
        // The group bounds contain the camera, but the off-screen window
        // excludes the portal. Only the entry callback can survive.
        let entry = if expected.is_empty() {
            &[][..]
        } else {
            &[0][..]
        };
        assert_eq!(
            query.query_outdoor_group(&root, camera, 0, [3., 3., 4., 4.])?,
            entry
        );
        assert_eq!(
            query
                .visits()
                .iter()
                .map(|visit| visit.group)
                .collect::<Vec<_>>(),
            entry
        );
        root.set_transform(Mat4::from_translation(Vec3::splat(1000.)))?;
        assert!(
            query
                .query_outdoor_group(&root, camera, 0, [0., 0., 1., 1.])?
                .is_empty()
        );
        assert_eq!(
            query.query_outdoor_group(&root, camera, 2, [0., 0., 1., 1.]),
            Err(WorldModelVisibilityError::InvalidGroup)
        );
        assert!(query.visits().is_empty());
    }
    Ok(())
}

/// The nearer portal widens the exterior window, while the farther supplies
/// its depth. Stopping after the first admitted portal loses this union.
#[test]
fn camera_root_merges_all_true_exterior_windows_and_resets_between_roots()
-> Result<(), Box<dyn Error>> {
    use solarity_systems::WorldModelPortalProjector;
    let model = Arc::new(visibility_model(
        &[0, 8, 8],
        &[0, 8, 8],
        &[[0, 1], [0, 2]],
        &[[0., 0., 1., 0.], [0., 0., 1., -3.]],
    )?);
    let root = PlacedWorldModelCollision::prepare_transform(model, Mat4::IDENTITY)?;
    let camera = WorldSceneCameraFrame::perspective(
        Vec3::new(0., 0., 6.),
        Vec3::ZERO,
        -Vec3::Z,
        Vec3::Y,
        0.9424778,
        1.,
        [0.2, 100.],
    )?;
    let frame = camera.for_root(Mat4::IDENTITY, Mat4::IDENTITY)?;
    let forward = camera.local_forward_plane(Mat4::IDENTITY)?;
    let mut projector = WorldModelPortalProjector::default();
    let mut single = Vec::new();
    for portal in root.model().portals() {
        let first = usize::from(portal.vertex_start());
        let end = first + usize::from(portal.vertex_count());
        single.push(
            projector
                .project_exterior_polygon(
                    &root.model().portal_vertices()[first..end],
                    Vec3::Z,
                    1,
                    forward,
                    frame,
                )?
                .ok_or("fixture exterior portal was rejected")?,
        );
    }
    assert!(single[0].depth > single[1].depth);
    assert!(single[0].screen_window[0] > single[1].screen_window[0]);
    let mut query = WorldModelCameraSceneQuery::default();
    assert!(query.query_camera_root(&root, camera, &[0])?);
    let bank = query.exterior_window().ok_or("missing exterior bank")?;
    assert_eq!(bank.screen_window, single[1].screen_window);
    assert_eq!(bank.depth, single[0].depth);
    assert!(!query.query_camera_root(&root, camera, &[])?);
    assert!(query.exterior_window().is_none());
    assert!(query.query_camera_root(&root, camera, &[0])?);
    query.query_outdoor_group(&root, camera, 1, [0., 0., 1., 1.])?;
    assert!(query.exterior_window().is_none());
    Ok(())
}
