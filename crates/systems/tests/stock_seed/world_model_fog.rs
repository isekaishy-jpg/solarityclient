//! Decoded portal graphs compared with original 7D77C0 traversal.

use super::support::{Fixture, FixtureFile};
use glam::{Mat4, Vec3};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedWorldModel, Locale,
};
use solarity_systems::PlacedWorldModelCollision;
use std::{error::Error, sync::Arc};

#[test]
fn world_model_fog_portals_match_original_depth_flags_and_placement() -> Result<(), Box<dyn Error>>
{
    let transforms = [
        Mat4::IDENTITY,
        Mat4::from_translation(Vec3::new(40., -20., 10.)),
        Mat4::from_cols_array(&[
            0., -2., 0., 0., 2., 0., 0., 0., 0., 0., 2., 0., -20., 40., -10., 1.,
        ]),
    ];
    let mut placements = Vec::new();
    let mut count = 0;
    for line in include_str!("../fixtures/world_model_fog_portal_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let words = line.split_ascii_whitespace().collect::<Vec<_>>();
        if words[0] == "scene" {
            let values = words[2..]
                .iter()
                .map(|v| v.parse::<u32>())
                .collect::<Result<Vec<_>, _>>()?;
            let size = values[0] as usize;
            let mogp = &values[1..1 + size];
            let mogi = &values[1 + size..1 + size * 2];
            let edges = values[2 + size * 2..].as_chunks::<2>().0;
            let (root, groups) = graph(mogp, mogi, edges);
            let paths = (0..size)
                .map(|i| format!("World\\Fog_{i:03}.wmo"))
                .collect::<Vec<_>>();
            let mut files = vec![FixtureFile {
                path: "World\\Fog.wmo",
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
            let model = Arc::new(DecodedWorldModel::load(
                &mut store,
                &AssetPath::new("World\\Fog.wmo")?,
            )?);
            placements = transforms
                .into_iter()
                .map(|transform| {
                    PlacedWorldModelCollision::prepare_transform(Arc::clone(&model), transform)
                })
                .collect::<Result<Vec<_>, _>>()?;
        } else {
            let primary = words[1].parse::<usize>()?;
            let secondary = words[2].parse::<i32>()?;
            let secondary = (secondary >= 0).then_some(secondary as usize);
            let values = words[3..6]
                .iter()
                .map(|v| u32::from_str_radix(v, 16).map(f32::from_bits))
                .collect::<Result<Vec<_>, _>>()?;
            let point = Vec3::from_slice(&values);
            let expected = if words[6] == "-" {
                None
            } else {
                Some(u32::from_str_radix(words[6], 16)?)
            };
            for (placement, transform) in placements.iter().zip(transforms) {
                let environment = placement
                    .fog_environment(primary, secondary, transform.transform_point3(point))?
                    .ok_or("missing base MFOG")?;
                assert_eq!(
                    environment.boundary_distance().map(f32::to_bits),
                    expected,
                    "{line}, {transform}"
                );
                assert_eq!(environment.palette().banks()[0].range(), (100., 0.25));
                assert_eq!(environment.palette().banks()[1].range(), (50., 0.5));
                count += 1;
            }
        }
    }
    assert_eq!(count, 810);
    Ok(())
}

pub(super) fn graph(mogp: &[u32], mogi: &[u32], edges: &[[u32; 2]]) -> (Vec<u8>, Vec<Vec<u8>>) {
    let mut root = Vec::new();
    chunk(&mut root, *b"REVM", &17u32.to_le_bytes());
    let mut header = vec![0; 64];
    word(&mut header, 4, mogp.len() as u32);
    word(&mut header, 8, edges.len() as u32);
    vector(&mut header, 36, [-100.; 3]);
    vector(&mut header, 48, [100.; 3]);
    chunk(&mut root, *b"DHOM", &header);
    let mut info = vec![0; mogp.len() * 32];
    for (index, flags) in mogi.iter().enumerate() {
        word(&mut info, index * 32, *flags);
        vector(&mut info, index * 32 + 4, [-100.; 3]);
        vector(&mut info, index * 32 + 16, [100.; 3]);
        word(&mut info, index * 32 + 28, u32::MAX);
    }
    chunk(&mut root, *b"IGOM", &info);
    let mut vertices = Vec::new();
    let mut portals = Vec::new();
    for index in 0..edges.len() {
        let z = index as f32 * 3.;
        for point in [[-2., -2., z], [2., -2., z], [2., 2., z], [-2., 2., z]] {
            vertices.extend(point.into_iter().flat_map(f32::to_le_bytes));
        }
        portals.extend((index as u16 * 4).to_le_bytes());
        portals.extend(4u16.to_le_bytes());
        portals.extend([0., 0., 1., -z].into_iter().flat_map(f32::to_le_bytes));
    }
    chunk(&mut root, *b"VPOM", &vertices);
    chunk(&mut root, *b"TPOM", &portals);
    let mut references = Vec::new();
    let mut groups = Vec::new();
    for (index, flags) in mogp.iter().enumerate() {
        let first = references.len() / 8;
        for (portal, &[a, b]) in edges.iter().enumerate() {
            if a as usize == index || b as usize == index {
                let neighbor = if a as usize == index { b } else { a };
                references.extend(
                    [portal as u16, neighbor as u16, 1, 0]
                        .into_iter()
                        .flat_map(u16::to_le_bytes),
                );
            }
        }
        let mut header = vec![0; 68];
        word(&mut header, 8, *flags);
        vector(&mut header, 12, [-100.; 3]);
        vector(&mut header, 24, [100.; 3]);
        header[36..38].copy_from_slice(&(first as u16).to_le_bytes());
        header[38..40].copy_from_slice(&((references.len() / 8 - first) as u16).to_le_bytes());
        let mut group = Vec::new();
        chunk(&mut group, *b"REVM", &17u32.to_le_bytes());
        chunk(&mut group, *b"PGOM", &header);
        groups.push(group);
    }
    chunk(&mut root, *b"RPOM", &references);
    let mut fog = [0; 48];
    for (offset, value) in [(24, 100f32), (28, 0.25), (36, 50.), (40, 0.5)] {
        word(&mut fog, offset, value.to_bits());
    }
    word(&mut fog, 32, 0xff123456);
    word(&mut fog, 44, 0xffabcdef);
    chunk(&mut root, *b"GOFM", &fog);
    (root, groups)
}

fn word(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn vector(bytes: &mut [u8], offset: usize, values: [f32; 3]) {
    for (index, value) in values.into_iter().enumerate() {
        word(bytes, offset + index * 4, value.to_bits());
    }
}
fn chunk(bytes: &mut Vec<u8>, magic: [u8; 4], data: &[u8]) {
    bytes.extend(magic);
    bytes.extend((data.len() as u32).to_le_bytes());
    bytes.extend(data);
}
