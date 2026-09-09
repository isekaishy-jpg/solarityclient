//! Ordered original-client visibility visits over complete decoded WMO graphs.

use super::support::{Fixture, FixtureFile};
use glam::Vec3;
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedWorldModel, Locale,
};
use solarity_systems::{WorldModelVisibilityError, WorldModelVisibilityQuery};
use std::error::Error;

fn words(line: &str) -> Vec<&str> {
    line.split_ascii_whitespace().collect()
}
fn float(word: &str) -> Result<f32, Box<dyn Error>> {
    Ok(f32::from_bits(u32::from_str_radix(word, 16)?))
}

#[test]
fn root_local_camera_plane_matches_original_transform_and_short_direction_rules()
-> Result<(), Box<dyn Error>> {
    use glam::Mat4;
    use solarity_systems::WorldSceneCameraFrame;
    let mut count = 0;
    for (case, line) in include_str!("../fixtures/world_model_local_camera_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .enumerate()
    {
        let values = words(line)
            .into_iter()
            .map(float)
            .collect::<Result<Vec<_>, _>>()?;
        let inverse = Mat4::from_cols_slice(&values[..16]);
        let eye = Vec3::from_slice(&values[16..19]);
        let target = Vec3::from_slice(&values[19..22]);
        // A parallel up vector would make the unrelated perspective frame
        // singular. Select either axis while preserving the exact two points.
        let delta = target - eye;
        let up = if delta.cross(Vec3::Z).length_squared() > 0.01 {
            Vec3::Z
        } else {
            Vec3::Y
        };
        let frame =
            WorldSceneCameraFrame::perspective(eye, target, delta, up, 0.9424778, 1., [0.2, 100.])?;
        let local = frame.for_root(inverse.inverse(), inverse)?.local_camera;
        for (channel, (actual, expected)) in local
            .to_array()
            .into_iter()
            .zip(&values[22..25])
            .enumerate()
        {
            assert_eq!(
                actual.to_bits(),
                expected.to_bits(),
                "local camera {case}/{channel}"
            );
        }
        for (channel, (actual, expected)) in frame
            .local_forward_plane(inverse)?
            .into_iter()
            .zip(&values[28..32])
            .enumerate()
        {
            assert_eq!(
                actual.to_bits(),
                expected.to_bits(),
                "local plane {case}/{channel}: {actual} != {expected}"
            );
        }
        count += 1;
    }
    assert_eq!(count, 312);
    Ok(())
}

#[test]
fn scene_camera_matches_original_perspective_corners_and_relative_projection()
-> Result<(), Box<dyn Error>> {
    use glam::Mat4;
    use solarity_systems::WorldSceneCameraFrame;
    let mut count = 0;
    for (case, line) in include_str!("../fixtures/world_scene_projection_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .enumerate()
    {
        let values = words(line)
            .into_iter()
            .map(float)
            .collect::<Result<Vec<_>, _>>()?;
        let frame = WorldSceneCameraFrame::perspective(
            Vec3::from_slice(&values[..3]),
            Vec3::from_slice(&values[3..6]),
            Vec3::from_slice(&values[6..9]),
            Vec3::from_slice(&values[9..12]),
            values[12],
            values[13],
            [values[14], values[15]],
        )?;
        let projection = frame.for_root(Mat4::IDENTITY, Mat4::IDENTITY)?;
        for (channel, (actual, expected)) in projection
            .relative_projection
            .to_cols_array()
            .into_iter()
            .chain(frame.corners().iter().flat_map(|point| point.to_array()))
            .chain(projection.clip_planes.into_iter().flatten())
            .zip(&values[48..])
            .enumerate()
        {
            assert_eq!(
                actual.to_bits(),
                expected.to_bits(),
                "case {case} channel {channel}: {actual} != {expected}"
            );
        }
        count += 1;
    }
    assert_eq!(count, 648);
    Ok(())
}

/// Native camera-edge fixtures include the sixth face and all tolerance crossings.
#[test]
fn scene_bounds_match_original_six_plane_tolerance_at_camera_edges() -> Result<(), Box<dyn Error>> {
    use solarity_systems::{MovementCollisionBounds, WorldSceneCameraFrame};
    let cameras = include_str!("../fixtures/world_scene_projection_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            let values = words(line)
                .into_iter()
                .map(float)
                .collect::<Result<Vec<_>, _>>()?;
            Ok::<_, Box<dyn Error>>(WorldSceneCameraFrame::perspective(
                Vec3::from_slice(&values[..3]),
                Vec3::from_slice(&values[3..6]),
                Vec3::from_slice(&values[6..9]),
                Vec3::from_slice(&values[9..12]),
                values[12],
                values[13],
                [values[14], values[15]],
            )?)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut count = 0;
    for line in include_str!("../fixtures/world_scene_bounds_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let values = words(line);
        let camera = cameras[values[0].parse::<usize>()?];
        let coordinates = values[1..7]
            .iter()
            .map(|word| float(word))
            .collect::<Result<Vec<_>, _>>()?;
        let bounds = MovementCollisionBounds::new(
            Vec3::from_slice(&coordinates[..3]),
            Vec3::from_slice(&coordinates[3..]),
        )?;
        assert_eq!(
            camera.intersects_bounds(bounds),
            values[7] == "3",
            "case {count}: {line}"
        );
        count += 1;
    }
    assert_eq!(count, 1344);
    Ok(())
}

#[test]
fn portal_camera_planes_match_original_order_and_float_stores() -> Result<(), Box<dyn Error>> {
    use glam::Mat4;
    use solarity_systems::WorldModelPortalProjectionFrame;
    let mut count = 0;
    for (case, line) in include_str!("../fixtures/world_model_portal_camera_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .enumerate()
    {
        let values = words(line)
            .into_iter()
            .map(float)
            .collect::<Result<Vec<_>, _>>()?;
        let corners = std::array::from_fn(|i| Vec3::from_slice(&values[i * 3..i * 3 + 3]));
        let frame = WorldModelPortalProjectionFrame::from_frustum_corners(
            Mat4::IDENTITY,
            Vec3::ZERO,
            Vec3::ZERO,
            Mat4::IDENTITY,
            corners,
        )?;
        for (channel, (actual, expected)) in frame
            .clip_planes
            .into_iter()
            .flatten()
            .zip(&values[24..])
            .enumerate()
        {
            assert_eq!(
                actual.to_bits(),
                expected.to_bits(),
                "case {case} channel {channel}: {actual} != {expected}"
            );
        }
        count += 1;
    }
    assert_eq!(count, 108);
    assert!(matches!(
        WorldModelPortalProjectionFrame::from_frustum_corners(
            Mat4::IDENTITY,
            Vec3::ZERO,
            Vec3::ZERO,
            Mat4::IDENTITY,
            [Vec3::ZERO; 8],
        ),
        Err(WorldModelVisibilityError::DegenerateFrustum)
    ));
    assert!(matches!(
        WorldModelPortalProjectionFrame::from_frustum_corners(
            Mat4::IDENTITY,
            Vec3::ZERO,
            Vec3::ZERO,
            Mat4::IDENTITY,
            [Vec3::splat(f32::NAN); 8],
        ),
        Err(WorldModelVisibilityError::NonFiniteCoordinates)
    ));
    Ok(())
}

#[test]
fn portal_polygon_projection_matches_original_transform_clip_and_near_rules()
-> Result<(), Box<dyn Error>> {
    use glam::Mat4;
    use solarity_systems::{WorldModelPortalProjectionFrame, WorldModelPortalProjector};
    let mut projector = WorldModelPortalProjector::default();
    let mut count = 0;
    for (case, line) in include_str!("../fixtures/world_model_portal_projection_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .enumerate()
    {
        let row = words(line);
        let vertex_count: usize = row[0].parse()?;
        let values = row[1..row.len() - 5]
            .iter()
            .map(|v| float(v))
            .collect::<Result<Vec<_>, _>>()?;
        let mut blocks = values.as_slice();
        let vertices = blocks[..vertex_count * 3].as_chunks::<3>().0;
        blocks = &blocks[vertex_count * 3..];
        let plane = blocks[..4].try_into()?;
        let local_camera = Vec3::from_slice(&blocks[4..7]);
        let world_camera = Vec3::from_slice(&blocks[7..10]);
        let root_transform = Mat4::from_cols_slice(&blocks[10..26]);
        let relative_projection = Mat4::from_cols_slice(&blocks[26..42]);
        let clip_planes = blocks[42..62].as_chunks::<4>().0.try_into()?;
        let flags = u32::from_str_radix(row[row.len() - 5], 16)?;
        let actual = projector.project_polygon(
            vertices,
            plane,
            WorldModelPortalProjectionFrame {
                root_transform,
                local_camera,
                world_camera,
                relative_projection,
                clip_planes,
            },
        )?;
        if flags & 1 != 0 {
            assert!(actual.is_none(), "case {case}: native rejected {actual:?}");
        } else {
            let expected = row[row.len() - 4..]
                .iter()
                .map(|v| float(v))
                .collect::<Result<Vec<_>, _>>()?;
            let actual =
                actual.ok_or_else(|| format!("case {case}: native accepted {expected:?}"))?;
            for (channel, (actual, expected)) in actual.into_iter().zip(expected).enumerate() {
                assert_eq!(
                    actual.to_bits(),
                    expected.to_bits(),
                    "case {case} channel {channel}: {actual} != {expected}"
                );
            }
        }
        count += 1;
    }
    assert_eq!(count, 1122);
    Ok(())
}

#[test]
fn exterior_portal_windows_match_original_displacement_and_full_vertex_depth()
-> Result<(), Box<dyn Error>> {
    use glam::Mat4;
    use solarity_systems::{WorldModelPortalProjectionFrame, WorldModelPortalProjector};
    let mut projector = WorldModelPortalProjector::default();
    let mut count = 0;
    for (case, line) in include_str!("../fixtures/world_model_exterior_portal_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .enumerate()
    {
        let row = words(line);
        let vertex_count: usize = row[0].parse()?;
        let side: i16 = row[1].parse()?;
        let values = row[3..row.len() - 7]
            .iter()
            .map(|value| float(value))
            .collect::<Result<Vec<_>, _>>()?;
        let vertices = values[..vertex_count * 3].as_chunks::<3>().0;
        let blocks = &values[vertex_count * 3..];
        let frame = WorldModelPortalProjectionFrame {
            root_transform: Mat4::from_cols_slice(&blocks[10..26]),
            local_camera: Vec3::from_slice(&blocks[4..7]),
            world_camera: Vec3::from_slice(&blocks[7..10]),
            relative_projection: Mat4::from_cols_slice(&blocks[26..42]),
            clip_planes: blocks[42..62].as_chunks::<4>().0.try_into()?,
        };
        let actual = projector.project_exterior_polygon(
            vertices,
            Vec3::from_slice(&blocks[..3]),
            side,
            blocks[62..66].try_into()?,
            frame,
        )?;
        let admitted = u32::from_str_radix(row[row.len() - 7], 16)? != 0;
        assert_eq!(actual.is_some(), admitted, "admission case {case}");
        if let Some(actual) = actual {
            for (channel, (actual, expected)) in actual
                .screen_window
                .into_iter()
                .chain([actual.depth])
                .zip(&row[row.len() - 5..])
                .enumerate()
            {
                assert_eq!(
                    actual.to_bits(),
                    u32::from_str_radix(expected, 16)?,
                    "case {case} channel {channel}: {actual}"
                );
            }
        }
        count += 1;
    }
    assert_eq!(count, 1188);
    Ok(())
}

#[test]
fn portal_visibility_matches_original_visit_order_fog_and_clipping() -> Result<(), Box<dyn Error>> {
    let mut lines = include_str!("../fixtures/world_model_visibility_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .peekable();
    let mut count = 0;
    let mut query = WorldModelVisibilityQuery::default();
    while let Some(line) = lines.next() {
        let row = words(line);
        assert_eq!(row[0], "scene");
        let size: usize = row[1].parse()?;
        let flags = row[2..2 + size]
            .iter()
            .map(|v| v.parse::<u32>())
            .collect::<Result<Vec<_>, _>>()?;
        let info_flags = row[2 + size..2 + size * 2]
            .iter()
            .map(|v| v.parse::<u32>())
            .collect::<Result<Vec<_>, _>>()?;
        let edge_count: usize = row[2 + size * 2].parse()?;
        let mut edges = Vec::new();
        let mut planes = Vec::new();
        let mut windows = Vec::new();
        for _ in 0..edge_count {
            let edge = words(lines.next().ok_or("missing edge")?);
            edges.push([edge[1].parse()?, edge[2].parse()?]);
            let plane = edge[3..7]
                .iter()
                .map(|v| float(v))
                .collect::<Result<Vec<_>, _>>()?;
            planes.push([plane[0], plane[1], plane[2], plane[3]]);
            let bounds = edge[7..11]
                .iter()
                .map(|v| float(v))
                .collect::<Result<Vec<_>, _>>()?;
            windows.push(Some([bounds[0], bounds[1], bounds[2], bounds[3]]));
        }
        let model = visibility_model(&flags, &info_flags, &edges, &planes)?;
        assert_eq!(
            query.query(&model, Vec3::ZERO, size, 4, true, &windows),
            Err(WorldModelVisibilityError::InvalidGroup)
        );
        assert_eq!(
            query.query(&model, Vec3::ZERO, 0, 4, true, &[]),
            Err(WorldModelVisibilityError::PortalCount)
        );
        assert_eq!(
            query.query(&model, Vec3::splat(f32::NAN), 0, 4, true, &windows),
            Err(WorldModelVisibilityError::NonFiniteCoordinates)
        );
        while lines.peek().is_some_and(|line| line.starts_with("query ")) {
            let line = lines.next().ok_or("missing query")?;
            let row = words(line);
            let expected_count = row[7].parse::<usize>()?;
            let visits = query.query(
                &model,
                Vec3::new(float(row[4])?, float(row[5])?, float(row[6])?),
                row[1].parse()?,
                row[2].parse()?,
                row[3] == "1",
                &windows,
            )?;
            assert_eq!(visits.len(), expected_count, "{line}, {flags:?}");
            for visit in visits {
                let expected = words(lines.next().ok_or("missing visit")?);
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
    }
    assert_eq!(count, 1440);
    Ok(())
}

/// 7AD1F0's initial groups share the portal cache, including rejected exteriors.
#[test]
fn camera_root_scene_events_match_original_order_cache_and_depth_limit()
-> Result<(), Box<dyn Error>> {
    use solarity_systems::WorldModelSceneVisibilityEvent;
    let mut lines = include_str!("../fixtures/world_model_scene_visibility_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .peekable();
    let mut query = WorldModelVisibilityQuery::default();
    let mut count = 0;
    while let Some(line) = lines.next() {
        let row = words(line);
        assert_eq!(row[0], "scene");
        let size = row[1].parse::<usize>()?;
        let flags = row[2..2 + size]
            .iter()
            .map(|v| v.parse::<u32>())
            .collect::<Result<Vec<_>, _>>()?;
        let info = row[2 + size..2 + 2 * size]
            .iter()
            .map(|v| v.parse::<u32>())
            .collect::<Result<Vec<_>, _>>()?;
        let mut edges = Vec::new();
        let mut planes = Vec::new();
        let mut windows = Vec::new();
        for _ in 0..row[2 + 2 * size].parse::<usize>()? {
            let edge = words(lines.next().ok_or("missing scene edge")?);
            edges.push([edge[1].parse()?, edge[2].parse()?]);
            let values = edge[3..]
                .iter()
                .map(|v| float(v))
                .collect::<Result<Vec<_>, _>>()?;
            planes.push(values[..4].try_into()?);
            windows.push(Some(values[4..8].try_into()?));
        }
        let model = visibility_model(&flags, &info, &edges, &planes)?;
        assert_eq!(
            query.query_scene(&model, Vec3::ZERO, &[size], 10, &windows),
            Err(WorldModelVisibilityError::InvalidGroup)
        );
        assert_eq!(
            query.query_scene(&model, Vec3::splat(f32::NAN), &[0], 10, &windows),
            Err(WorldModelVisibilityError::NonFiniteCoordinates)
        );
        while lines.peek().is_some_and(|line| line.starts_with("query ")) {
            let line = lines.next().ok_or("missing scene query")?;
            let row = words(line);
            let maximum = row[1].parse()?;
            let initial_count = row[2].parse::<usize>()?;
            let initial = row[3..3 + initial_count]
                .iter()
                .map(|v| v.parse::<usize>())
                .collect::<Result<Vec<_>, _>>()?;
            let base = 3 + initial_count;
            let point = Vec3::new(
                float(row[base])?,
                float(row[base + 1])?,
                float(row[base + 2])?,
            );
            let events = query.query_scene(&model, point, &initial, maximum, &windows)?;
            assert_eq!(
                events.len(),
                row[base + 3].parse::<usize>()?,
                "case {count}: {line}, {flags:?}/{info:?}"
            );
            for event in events {
                let expected = words(lines.next().ok_or("missing scene event")?);
                match event {
                    WorldModelSceneVisibilityEvent::Group(visit) => {
                        assert_eq!(expected[0], "group", "case {count}");
                        assert_eq!(visit.group, expected[1].parse::<usize>()?, "case {count}");
                        assert_eq!(visit.indoor_fog, expected[2] == "1", "case {count}");
                        assert_eq!(visit.depth, expected[3].parse::<u32>()?, "case {count}");
                        for (actual, expected) in visit.screen_window.iter().zip(&expected[4..8]) {
                            assert_eq!(
                                actual.to_bits(),
                                u32::from_str_radix(expected, 16)?,
                                "case {count}"
                            );
                        }
                    }
                    WorldModelSceneVisibilityEvent::ExteriorPortal { reference } => {
                        assert_eq!(expected[0], "portal", "case {count}");
                        assert_eq!(*reference, expected[1].parse::<usize>()?, "case {count}");
                    }
                }
            }
            count += 1;
        }
        assert!(
            query
                .query_scene(&model, Vec3::ZERO, &[], 10, &windows)?
                .is_empty()
        );
    }
    assert_eq!(count, 756);
    Ok(())
}

/// Composes portal projection and group traversal for the exterior scene bank.
#[test]
fn complete_camera_root_pipeline_opens_only_visible_exterior_links() -> Result<(), Box<dyn Error>> {
    use glam::{Mat4, Quat};
    use solarity_systems::{
        PlacedWorldModelCollision, WorldModelCameraSceneQuery, WorldSceneCameraFrame,
    };
    use std::sync::Arc;
    let mut query = WorldModelCameraSceneQuery::default();
    for adjacent_flags in [8, 0x40, 0x100, 0x40000, 0x10000, 0] {
        let model = Arc::new(visibility_model(
            &[0, adjacent_flags],
            &[0, adjacent_flags],
            &[[0, 1]],
            &[[0., 0., 1., 0.]],
        )?);
        let mut root = PlacedWorldModelCollision::prepare_transform(model, Mat4::IDENTITY)?;
        for transform in [
            Mat4::IDENTITY,
            Mat4::from_scale_rotation_translation(
                Vec3::splat(2.),
                Quat::from_rotation_y(0.37),
                Vec3::new(500., -200., 70.),
            ),
        ] {
            root.set_transform(transform)?;
            let eye = transform.transform_point3(Vec3::new(0., 0., 3.));
            let toward = transform.transform_point3(Vec3::ZERO);
            let away = transform.transform_point3(Vec3::new(0., 0., 6.));
            let up = transform.transform_vector3(Vec3::Y);
            let camera = |target| {
                WorldSceneCameraFrame::perspective(
                    eye,
                    target,
                    target - eye,
                    up,
                    0.9424778,
                    16. / 9.,
                    [0.2, 100.],
                )
            };
            // Only 790AD0's exterior bank enables 79A790's outdoor lists.
            assert_eq!(
                query.query_camera_root(&root, camera(toward)?, &[0])?,
                adjacent_flags & 0x10008 != 0,
                "flags {adjacent_flags:x}"
            );
            assert_eq!(query.groups().first(), Some(&0));
            assert_eq!(query.groups().contains(&1), adjacent_flags != 8);
            assert!(!query.query_camera_root(&root, camera(away)?, &[0])?);
            assert!(!query.query_camera_root(&root, camera(toward)?, &[])?);
            assert_eq!(
                query.query_camera_root(&root, camera(toward)?, &[2]),
                Err(WorldModelVisibilityError::InvalidGroup)
            );
            assert_eq!(
                query.query_camera_root(&root, camera(toward)?, &[0, 0])?,
                adjacent_flags & 0x10008 != 0
            );
        }
    }
    Ok(())
}

/// 7AD1F0's final callbacks use MOGI flags and full world bounds independently
/// of recursive starting groups or the loaded group's 0x10000 flag.
#[test]
fn camera_root_direct_groups_follow_authored_flags_and_transformed_bounds()
-> Result<(), Box<dyn Error>> {
    use glam::Mat4;
    use solarity_systems::{
        PlacedWorldModelCollision, WorldModelCameraSceneQuery, WorldSceneCameraFrame,
    };
    use std::sync::Arc;
    let model = Arc::new(visibility_model(
        &[0, 0x10000, 0],
        &[0x10000, 0, 0],
        &[],
        &[],
    )?);
    let mut root = PlacedWorldModelCollision::prepare_transform(model, Mat4::IDENTITY)?;
    let camera = WorldSceneCameraFrame::perspective(
        Vec3::ZERO,
        Vec3::X,
        Vec3::X,
        Vec3::Z,
        0.9424778,
        1.,
        [0.2, 100.],
    )?;
    let mut query = WorldModelCameraSceneQuery::default();
    assert!(!query.query_camera_root(&root, camera, &[])?);
    assert_eq!(query.groups(), &[0]);
    assert!(!query.query_camera_root(&root, camera, &[2, 1])?);
    assert_eq!(query.groups(), &[2, 0]);
    root.set_transform(Mat4::from_translation(Vec3::new(-1000., 0., 0.)))?;
    assert!(!query.query_camera_root(&root, camera, &[])?);
    assert!(query.groups().is_empty());
    root.set_transform(Mat4::IDENTITY)?;
    assert!(!query.query_camera_root(&root, camera, &[0])?);
    assert_eq!(query.groups(), &[0, 0]);
    Ok(())
}

/// Loads the oracle graph through complete decoded MPQ root/group resources.
fn visibility_model(
    flags: &[u32],
    info_flags: &[u32],
    edges: &[[u32; 2]],
    planes: &[[f32; 4]],
) -> Result<DecodedWorldModel, Box<dyn Error>> {
    let size = flags.len();
    let (mut root, groups) = super::world_model_fog::graph(flags, info_flags, edges);
    let mut offset = 0;
    while offset < root.len() {
        let length = u32::from_le_bytes(root[offset + 4..offset + 8].try_into()?) as usize;
        if &root[offset..offset + 4] == b"TPOM" {
            for (index, plane) in planes.iter().enumerate() {
                for (channel, value) in plane.iter().enumerate() {
                    let start = offset + 8 + index * 20 + 4 + channel * 4;
                    root[start..start + 4].copy_from_slice(&value.to_le_bytes());
                }
            }
        } else if &root[offset..offset + 4] == b"RPOM" {
            let mut reference = 0;
            for group in 0..size as u32 {
                for &[a, b] in edges {
                    if group == a || group == b {
                        let side = if group == a { 1i16 } else { -1i16 };
                        let start = offset + 8 + reference * 8 + 4;
                        root[start..start + 2].copy_from_slice(&side.to_le_bytes());
                        reference += 1;
                    }
                }
            }
        }
        offset += length + 8;
    }
    let paths = (0..size)
        .map(|i| format!("World\\Visible_{i:03}.wmo"))
        .collect::<Vec<_>>();
    let mut files = vec![FixtureFile {
        path: "World\\Visible.wmo",
        bytes: &root,
    }];
    files.extend(
        paths
            .iter()
            .zip(&groups)
            .map(|(path, bytes)| FixtureFile { path, bytes }),
    );
    let fixture = Fixture::new(&files)?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    Ok(DecodedWorldModel::load(
        &mut store,
        &AssetPath::new("World\\Visible.wmo")?,
    )?)
}
