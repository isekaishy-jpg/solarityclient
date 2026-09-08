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
        let (mut root, groups) = super::world_model_fog::graph(&flags, &info_flags, &edges);
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
                    for &[a, b] in &edges {
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
        let model = DecodedWorldModel::load(&mut store, &AssetPath::new("World\\Visible.wmo")?)?;
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
