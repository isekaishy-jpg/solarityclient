//! Native exterior-window entry into decoded WMO group graphs.

use std::{error::Error, sync::Arc};

use glam::{Mat4, Vec3};
use solarity_systems::{
    PlacedWorldModelCollision, WorldModelCameraSceneQuery, WorldModelVisibilityError,
    WorldModelVisibilityQuery, WorldSceneCameraFrame,
};

use super::world_model_visibility::visibility_model;

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
