//! Original-executable WMO spatial registration probes over decoded MPQ fixtures.

use std::{error::Error, sync::Arc};

use glam::{Mat4, Vec3};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedWorldModel, Locale,
};
use solarity_systems::{
    MovementBspCacheMode, MovementCollisionBounds, PlacedWorldModelCollision,
    WorldModelCollisionError, WorldModelRegistrationKind, WorldModelRegistrationQuery,
    probe_world_model_portals,
};

use super::support::{Fixture, FixtureFile};

#[test]
fn group_membership_matches_original_interior_exterior_bounds_and_order()
-> Result<(), Box<dyn Error>> {
    let transforms = [
        Mat4::IDENTITY,
        Mat4::from_translation(Vec3::new(-100., 200., -30.)),
        Mat4::from_cols_array(&[
            0., -2., 0., 0., 2., 0., 0., 0., 0., 0., 2., 0., -20., 40., -10., 1.,
        ]),
    ];
    let mut models = std::collections::HashMap::new();
    let mut count = 0;
    for line in include_str!("../fixtures/wmo-group-registration-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let words = line.split_whitespace().collect::<Vec<_>>();
        let bounds = words[..6]
            .iter()
            .map(|v| u32::from_str_radix(v, 16).map(f32::from_bits))
            .collect::<Result<Vec<_>, _>>()?;
        let fields = words[6..]
            .iter()
            .map(|v| v.parse::<i32>())
            .collect::<Result<Vec<_>, _>>()?;
        let flags = [fields[1], fields[2], fields[3], fields[4]];
        let model = if let Some(model) = models.get(&flags) {
            Arc::clone(model)
        } else {
            let (root, group) = membership_fixture(flags.map(|v| v as u32));
            let fixture = Fixture::new(&[
                FixtureFile {
                    path: "World\\Members.wmo",
                    bytes: &root,
                },
                FixtureFile {
                    path: "World\\Members_000.wmo",
                    bytes: &group,
                },
                FixtureFile {
                    path: "World\\Members_001.wmo",
                    bytes: &group,
                },
                FixtureFile {
                    path: "World\\Members_002.wmo",
                    bytes: &group,
                },
                FixtureFile {
                    path: "World\\Members_003.wmo",
                    bytes: &group,
                },
            ])?;
            let mut store = AssetStore::mount(ArchiveCatalog::discover(
                ClientDataRoot::new(fixture.data_root())?,
                Locale::EnUs,
            )?)?;
            let model = Arc::new(DecodedWorldModel::load(
                &mut store,
                &AssetPath::new("World\\Members.wmo")?,
            )?);
            models.insert(flags, Arc::clone(&model));
            model
        };
        let inverse = transforms[fields[0] as usize];
        let placement =
            PlacedWorldModelCollision::prepare_transforms(model, inverse.inverse(), inverse)?;
        let mut groups = Vec::new();
        placement.append_registration_groups(
            MovementCollisionBounds::new(
                Vec3::from_slice(&bounds[..3]),
                Vec3::from_slice(&bounds[3..]),
            )?,
            usize::try_from(fields[5]).ok(),
            &mut groups,
        )?;
        assert_eq!(groups.len(), fields[6] as usize, "{line}");
        assert_eq!(
            groups,
            fields[7..].iter().map(|&v| v as usize).collect::<Vec<_>>(),
            "{line}"
        );
        count += 1;
    }
    assert_eq!(count, 450);
    Ok(())
}

fn membership_fixture(flags: [u32; 4]) -> (Vec<u8>, Vec<u8>) {
    let (_, group) = scene_floor_fixture(8, 0.);
    let mut root = Vec::new();
    chunk(&mut root, *b"REVM", &17u32.to_le_bytes());
    let mut header = vec![0; 64];
    word(&mut header, 4, 4);
    vector(&mut header, 36, [-16.; 3]);
    vector(&mut header, 48, [16.; 3]);
    chunk(&mut root, *b"DHOM", &header);
    let bounds = [
        [[-16., -16., -16.], [0., 16., 16.]],
        [[0., -16., -16.], [16., 16., 16.]],
        [[-16., 0., -16.], [16., 16., 16.]],
        [[-16., -16., -16.], [16., 0., 16.]],
    ];
    let mut info = vec![0; 128];
    for i in 0..4 {
        word(&mut info, i * 32, flags[i]);
        vector(&mut info, i * 32 + 4, bounds[i][0]);
        vector(&mut info, i * 32 + 16, bounds[i][1]);
        word(&mut info, i * 32 + 28, u32::MAX);
    }
    chunk(&mut root, *b"IGOM", &info);
    (root, group)
}

#[test]
fn scene_registration_matches_original_banks_fallbacks_ties_and_terrain()
-> Result<(), Box<dyn Error>> {
    let mut count = 0;
    for line in include_str!("../fixtures/wmo-scene-registration-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let words = line.split_whitespace().collect::<Vec<_>>();
        let mode = words[0].parse::<u32>()?;
        let height = f32::from_bits(u32::from_str_radix(words[1], 16)?);
        let clear = words[2] == "1";
        let roots = words[3].parse::<usize>()?;
        let mut query = WorldModelRegistrationQuery::<u32>::new(
            Vec3::new(0., 0., 4.),
            Vec3::new(0., 0., -4.),
            Vec3::new(0., 0., 0.15),
        )?;
        for i in 0..roots {
            let fields = &words[4 + i * 4..8 + i * 4];
            let owner = fields[0].parse::<u32>()?;
            let flags = fields[1].parse::<u32>()?;
            if flags & 0x20 != 0 {
                continue;
            } // The scene filters excluded native roots.
            let (root, group) = scene_floor_fixture(
                fields[2].parse()?,
                f32::from_bits(u32::from_str_radix(fields[3], 16)?),
            );
            let fixture = Fixture::new(&[
                FixtureFile {
                    path: "World\\Scene.wmo",
                    bytes: &root,
                },
                FixtureFile {
                    path: "World\\Scene_000.wmo",
                    bytes: &group,
                },
            ])?;
            let mut store = AssetStore::mount(ArchiveCatalog::discover(
                ClientDataRoot::new(fixture.data_root())?,
                Locale::EnUs,
            )?)?;
            let model = Arc::new(DecodedWorldModel::load(
                &mut store,
                &AssetPath::new("World\\Scene.wmo")?,
            )?);
            let mut placement =
                PlacedWorldModelCollision::prepare_transform(model, Mat4::IDENTITY)?;
            query.probe_root(
                owner,
                if flags & 0x400 == 0 {
                    WorldModelRegistrationKind::Static
                } else {
                    WorldModelRegistrationKind::Transformed
                },
                &mut placement,
                MovementBspCacheMode::Disabled,
            )?;
        }
        let mut selection = query.finish();
        if mode != 0 {
            if clear {
                selection.clear_secondary_bank();
            }
            let fraction = ((4.0 - f64::from(height)) * f64::from(0.001_f32)) as f32;
            if mode == 1 && fraction >= 0.0 {
                selection.occlude_by_terrain(fraction)?;
            }
        }
        let primary = selection.primary();
        let fallback = selection.fallback();
        for (i, candidate) in [primary[0], primary[1], fallback[0], fallback[1]]
            .into_iter()
            .enumerate()
        {
            let offset = 4 + roots * 4 + i * 5;
            let expected = words[offset..offset + 5]
                .iter()
                .map(|word| u32::from_str_radix(word, 16))
                .collect::<Result<Vec<_>, _>>()?;
            assert_eq!(
                candidate.map_or(u32::MAX, |hit| hit.owner()),
                expected[0],
                "{line}"
            );
            if let Some(candidate) = candidate {
                let hit = candidate.hit();
                assert_eq!(hit.group_index(), expected[1] as usize, "{line}");
                assert_eq!(hit.fraction().to_bits(), expected[2], "{line}");
                assert_eq!(
                    u32::from(hit.face().unwrap_or(u16::MAX)),
                    expected[3],
                    "{line}"
                );
                assert_eq!(hit.is_interior(), expected[4] != 0, "{line}");
            }
        }
        assert_eq!(selection.selected(), primary[0].or(primary[1]));
        count += 1;
    }
    assert_eq!(count, 61);
    Ok(())
}

fn scene_floor_fixture(flag: u8, height: f32) -> (Vec<u8>, Vec<u8>) {
    let mut root = Vec::new();
    chunk(&mut root, *b"REVM", &17u32.to_le_bytes());
    let mut header = vec![0; 64];
    word(&mut header, 4, 1);
    vector(&mut header, 36, [-16.; 3]);
    vector(&mut header, 48, [16.; 3]);
    chunk(&mut root, *b"DHOM", &header);
    let mut info = vec![0; 32];
    word(&mut info, 0, 8);
    vector(&mut info, 4, [-16.; 3]);
    vector(&mut info, 16, [16.; 3]);
    word(&mut info, 28, u32::MAX);
    chunk(&mut root, *b"IGOM", &info);
    let mut header = vec![0; 68];
    word(&mut header, 8, 8);
    vector(&mut header, 12, [-16.; 3]);
    vector(&mut header, 24, [16.; 3]);
    chunk(&mut header, *b"YPOM", &[flag, 255]);
    let indices = [0u16, 1, 2]
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    chunk(&mut header, *b"IVOM", &indices);
    let vertices = [[-3., -3., height], [3., -3., height], [-3., 3., height]]
        .into_iter()
        .flatten()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    chunk(&mut header, *b"TVOM", &vertices);
    let normals = [[0f32, 0., 1.]; 3]
        .into_iter()
        .flatten()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    chunk(&mut header, *b"RNOM", &normals);
    let mut node = vec![0; 16];
    short(&mut node, 0, 4);
    short(&mut node, 2, u16::MAX);
    short(&mut node, 4, u16::MAX);
    short(&mut node, 6, 1);
    chunk(&mut header, *b"NBOM", &node);
    chunk(&mut header, *b"RBOM", &0u16.to_le_bytes());
    let mut group = Vec::new();
    chunk(&mut group, *b"REVM", &17u32.to_le_bytes());
    chunk(&mut group, *b"PGOM", &header);
    (root, group)
}

#[test]
fn floor_probe_rejects_selected_cycles_invalid_axes_and_negative_children()
-> Result<(), Box<dyn Error>> {
    for variant in 0..3 {
        let (root, _, adjacent) = portal_fixture([0., 0., 1.], 0., 1, false, 2);
        let mut group = floor_fixture(1, 0, 0);
        let header = chunk_data_mut(&mut group, *b"PGOM");
        let nodes = chunk_data_mut(&mut header[68..], *b"NBOM");
        match variant {
            0 => short(nodes, 4, 0),
            1 => short(nodes, 0, 3),
            _ => short(nodes, 2, (-2_i16) as u16),
        }
        let fixture = Fixture::new(&[
            FixtureFile {
                path: "World\\Portals.wmo",
                bytes: &root,
            },
            FixtureFile {
                path: "World\\Portals_000.wmo",
                bytes: &group,
            },
            FixtureFile {
                path: "World\\Portals_001.wmo",
                bytes: &adjacent,
            },
        ])?;
        let mut store = AssetStore::mount(ArchiveCatalog::discover(
            ClientDataRoot::new(fixture.data_root())?,
            Locale::EnUs,
        )?)?;
        let model = Arc::new(DecodedWorldModel::load(
            &mut store,
            &AssetPath::new("World\\Portals.wmo")?,
        )?);
        let mut placement = PlacedWorldModelCollision::prepare_transform(model, Mat4::IDENTITY)?;
        for _ in 0..2 {
            assert_eq!(
                placement.probe_group_floor(
                    0,
                    Vec3::new(0., 0., 4.),
                    Vec3::new(0., 0., -4.),
                    [1.05; 2],
                    MovementBspCacheMode::Enabled
                ),
                Err(WorldModelCollisionError::InvalidBsp)
            );
            // Empty admitted group remains queryable after an error in another group.
            assert_eq!(
                placement
                    .probe_group_floor(
                        1,
                        Vec3::new(0., 0., 4.),
                        Vec3::new(0., 0., -4.),
                        [1.05; 2],
                        MovementBspCacheMode::Enabled
                    )?
                    .primary,
                None
            );
        }
    }
    Ok(())
}

#[test]
fn root_registration_matches_original_group_flags_containment_and_portal_precedence()
-> Result<(), Box<dyn Error>> {
    let mut count = 0;
    for line in include_str!("../fixtures/wmo-root-registration-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let words = line.split_whitespace().collect::<Vec<_>>();
        let values = words[..12]
            .iter()
            .map(|word| u32::from_str_radix(word, 16).map(f32::from_bits))
            .collect::<Result<Vec<_>, _>>()?;
        let settings = words[12..21]
            .iter()
            .map(|v| v.parse::<i32>())
            .collect::<Result<Vec<_>, _>>()?;
        let (mut root, _, mut adjacent) =
            portal_fixture([0., 0., 1.], values[11], settings[8] as i16, false, 2);
        let mut group = floor_fixture(settings[0] as usize, 0, 0);
        if settings[7] != 0 {
            adjacent = group.clone();
        }
        let info = chunk_data_mut(&mut root, *b"IGOM");
        for (i, flags) in [settings[2], settings[4]].into_iter().enumerate() {
            word(info, i * 32, flags as u32);
            vector(info, i * 32 + 4, [-3., -3., -1.]);
            vector(info, i * 32 + 16, [3., 3., 3.]);
        }
        let header = chunk_data_mut(&mut group, *b"PGOM");
        word(header, 8, settings[3] as u32);
        short(header, 38, 1);
        word(
            chunk_data_mut(&mut adjacent, *b"PGOM"),
            8,
            settings[5] as u32,
        );
        let fixture = Fixture::new(&[
            FixtureFile {
                path: "World\\Portals.wmo",
                bytes: &root,
            },
            FixtureFile {
                path: "World\\Portals_000.wmo",
                bytes: &group,
            },
            FixtureFile {
                path: "World\\Portals_001.wmo",
                bytes: &adjacent,
            },
        ])?;
        let mut store = AssetStore::mount(ArchiveCatalog::discover(
            ClientDataRoot::new(fixture.data_root())?,
            Locale::EnUs,
        )?)?;
        let model = Arc::new(DecodedWorldModel::load(
            &mut store,
            &AssetPath::new("World\\Portals.wmo")?,
        )?);
        let mut placement = PlacedWorldModelCollision::prepare_transform(model, Mat4::IDENTITY)?;
        let hits = placement.probe_registration(
            Vec3::from_slice(&values[..3]),
            Vec3::from_slice(&values[3..6]),
            Vec3::from_slice(&values[6..9]),
            if settings[6] == 0 {
                WorldModelRegistrationKind::Static
            } else {
                WorldModelRegistrationKind::Transformed
            },
            [values[9], values[10]],
            if settings[1] == 0 {
                MovementBspCacheMode::Disabled
            } else {
                MovementBspCacheMode::Enabled
            },
        )?;
        for (i, hit) in [hits.primary, hits.fallback].into_iter().enumerate() {
            let expected = words[21 + i * 5..26 + i * 5]
                .iter()
                .map(|word| u32::from_str_radix(word, 16))
                .collect::<Result<Vec<_>, _>>()?;
            assert_eq!(hit.is_some(), expected[0] != 0, "{line}");
            if let Some(hit) = hit {
                assert_eq!(hit.group_index(), expected[1] as usize, "{line}");
                assert_eq!(hit.fraction().to_bits(), expected[2], "{line}");
                assert_eq!(
                    u32::from(hit.face().unwrap_or(u16::MAX)),
                    expected[3],
                    "{line}"
                );
                assert_eq!(hit.is_interior(), expected[4] != 0, "{line}");
            }
        }
        count += 1;
    }
    assert_eq!(count, 208);
    Ok(())
}

pub(super) fn chunk_data_mut(bytes: &mut [u8], magic: [u8; 4]) -> &mut [u8] {
    let mut offset = 0;
    while offset + 8 <= bytes.len() {
        let size = u32::from_le_bytes(std::array::from_fn(|i| bytes[offset + 4 + i])) as usize;
        if bytes[offset..offset + 4] == magic {
            return &mut bytes[offset + 8..offset + 8 + size];
        }
        offset += 8 + size;
    }
    panic!("fixture missing chunk {magic:?}");
}

#[test]
fn floor_probe_matches_original_primary_fallback_cache_and_bsp_order() -> Result<(), Box<dyn Error>>
{
    let mut count = 0;
    for line in include_str!("../fixtures/wmo-bsp-probe-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let words = line.split_whitespace().collect::<Vec<_>>();
        let values = words[..8]
            .iter()
            .map(|word| u32::from_str_radix(word, 16).map(f32::from_bits))
            .collect::<Result<Vec<_>, _>>()?;
        let profile = words[8].parse::<usize>()?;
        let (root, _, adjacent) = portal_fixture([0., 0., 1.], 0., 1, false, 2);
        let group = floor_fixture(profile, words[10].parse()?, words[11].parse()?);
        let fixture = Fixture::new(&[
            FixtureFile {
                path: "World\\Portals.wmo",
                bytes: &root,
            },
            FixtureFile {
                path: "World\\Portals_000.wmo",
                bytes: &group,
            },
            FixtureFile {
                path: "World\\Portals_001.wmo",
                bytes: &adjacent,
            },
        ])?;
        let mut store = AssetStore::mount(ArchiveCatalog::discover(
            ClientDataRoot::new(fixture.data_root())?,
            Locale::EnUs,
        )?)?;
        let model = Arc::new(DecodedWorldModel::load(
            &mut store,
            &AssetPath::new("World\\Portals.wmo")?,
        )?);
        let mut placement = PlacedWorldModelCollision::prepare_transform(model, Mat4::IDENTITY)?;
        let cache_mode = if words[9] == "1" {
            MovementBspCacheMode::Enabled
        } else {
            MovementBspCacheMode::Disabled
        };
        // Repeat on the same owner to exercise scratch clearing after both hits and misses.
        for _ in 0..2 {
            let hits = placement.probe_group_floor(
                0,
                Vec3::from_slice(&values[..3]),
                Vec3::from_slice(&values[3..6]),
                [values[6], values[7]],
                cache_mode,
            )?;
            for (i, hit) in [hits.primary, hits.fallback].into_iter().enumerate() {
                let fraction = u32::from_str_radix(words[12 + i * 2], 16)?;
                let face = u32::from_str_radix(words[13 + i * 2], 16)?;
                assert_eq!(
                    hit.map_or(u32::MAX, |h| u32::from(h.face())),
                    face,
                    "{line}"
                );
                assert_eq!(
                    hit.map_or(values[6 + i], |h| h.fraction()).to_bits(),
                    fraction,
                    "{line}"
                );
            }
        }
        count += 1;
    }
    assert_eq!(count, 468);
    Ok(())
}

pub(super) fn floor_fixture(profile: usize, geometry: usize, topology: usize) -> Vec<u8> {
    let positions: [[f32; 3]; 8] = [
        [-3., -3., 0.],
        [3., -3., 0.],
        [-3., 3., 0.],
        [3., 3., 0.],
        [-3., -3., 2.],
        [3., -3., 2.],
        [-3., 3., 2.],
        [3., 3., 2.],
    ];
    let positions = positions.map(|v| {
        let [x, y, z] = v.map(f64::from);
        (match geometry {
            1 => [x, y, z + x * 0.125 + y * 0.3 + 0.12345],
            2 => [
                x * 0.127 + 10000.123,
                y * 1.33 - 17000.234,
                z * 0.723 + 31.567,
            ],
            3 => [x + 16., y, z],
            _ => [x, y, z],
        })
        .map(|f| f as f32)
    });
    let triangles: Vec<[u16; 3]> = if profile >= 6 {
        vec![[0, 1, 2]; 8193]
    } else {
        vec![
            [0, 1, 2],
            [1, 3, 2],
            [4, 5, 6],
            [5, 7, 6],
            [0, 1, 2],
            [4, 5, 6],
        ]
    };
    let flags = if profile >= 6 {
        let mut flags = vec![[8, 2, 32, 0][profile - 6]; 8193];
        if matches!(profile, 7 | 9) {
            flags[8192] = 8;
        }
        flags
    } else {
        [[8, 8, 4, 4, 32, 2], [32; 6], [4; 6], [8; 6], [2; 6], [0; 6]][profile].to_vec()
    };
    let mut header = vec![0; 68];
    vector(&mut header, 12, [-16.; 3]);
    vector(&mut header, 24, [16.; 3]);
    let polygons = flags
        .into_iter()
        .flat_map(|flag| [flag, 255])
        .collect::<Vec<_>>();
    chunk(&mut header, *b"YPOM", &polygons);
    let indices = triangles
        .into_iter()
        .flatten()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    chunk(&mut header, *b"IVOM", &indices);
    let vertices = positions
        .into_iter()
        .flatten()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    chunk(&mut header, *b"TVOM", &vertices);
    let normals = positions
        .into_iter()
        .flat_map(|_| [0f32, 0., 1.])
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    chunk(&mut header, *b"RNOM", &normals);
    let node_defs = if profile >= 6 {
        vec![(4, u16::MAX, u16::MAX, 8193, 0, 0.)]
    } else {
        match topology {
            3 => vec![
                (0, 1, 2, 0, 0, 0.),
                (4, u16::MAX, u16::MAX, 4, 0, 0.),
                (1, 3, 4, 0, 4, 0.),
                (4, u16::MAX, u16::MAX, 5, 4, 0.),
                (4, u16::MAX, u16::MAX, 6, 9, 0.),
            ],
            4 => vec![(4, u16::MAX, u16::MAX, 6, 0, 0.)],
            _ => vec![
                (
                    topology as u16,
                    1,
                    2,
                    0,
                    0,
                    if topology == 2 { 1. } else { 0. },
                ),
                (4, u16::MAX, u16::MAX, 4, 0, 0.),
                (4, u16::MAX, u16::MAX, 5, 4, 0.),
            ],
        }
    };
    let mut nodes = vec![0; node_defs.len() * 16];
    for (i, (flags, negative, positive, count, first, plane)) in node_defs.into_iter().enumerate() {
        short(&mut nodes, i * 16, flags);
        short(&mut nodes, i * 16 + 2, negative);
        short(&mut nodes, i * 16 + 4, positive);
        short(&mut nodes, i * 16 + 6, count);
        word(&mut nodes, i * 16 + 8, first);
        word(&mut nodes, i * 16 + 12, f32::to_bits(plane));
    }
    chunk(&mut header, *b"NBOM", &nodes);
    let references: Vec<u16> = if profile >= 6 {
        (0..8193).collect()
    } else {
        match topology {
            3 => vec![4, 0, 2, 5, 1, 3, 0, 4, 2, 5, 2, 4, 0, 3, 1],
            4 => vec![0, 1, 2, 3, 4, 5],
            _ => vec![4, 0, 2, 5, 1, 3, 0, 4, 2],
        }
    };
    let references = references
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    chunk(&mut header, *b"RBOM", &references);
    let mut group = Vec::new();
    chunk(&mut group, *b"REVM", &17_u32.to_le_bytes());
    chunk(&mut group, *b"PGOM", &header);
    group
}

#[test]
fn portal_probe_matches_original_edges_sides_proximity_and_ties() -> Result<(), Box<dyn Error>> {
    let mut count = 0;
    for line in include_str!("../fixtures/wmo-portal-probe-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let words = line.split_whitespace().collect::<Vec<_>>();
        let values = words[..11]
            .iter()
            .map(|word| u32::from_str_radix(word, 16).map(f32::from_bits))
            .collect::<Result<Vec<_>, _>>()?;
        let start = Vec3::from_slice(&values[..3]);
        let end = Vec3::from_slice(&values[3..6]);
        let maximum = values[6];
        let normal = [values[7], values[8], values[9]];
        let side = words[12].parse::<i16>()?;
        let loaded = words[13] == "1";
        let second = words[14] == "1";
        let geometry = words[11].parse::<usize>()?;
        let (root, group, adjacent) = portal_fixture(normal, values[10], side, second, geometry);
        let mut files = vec![
            FixtureFile {
                path: "World\\Portals.wmo",
                bytes: &root,
            },
            FixtureFile {
                path: "World\\Portals_000.wmo",
                bytes: &group,
            },
        ];
        if loaded {
            files.push(FixtureFile {
                path: "World\\Portals_001.wmo",
                bytes: &adjacent,
            });
        }
        let fixture = Fixture::new(&files)?;
        let mut store = AssetStore::mount(ArchiveCatalog::discover(
            ClientDataRoot::new(fixture.data_root())?,
            Locale::EnUs,
        )?)?;
        let model = DecodedWorldModel::load(&mut store, &AssetPath::new("World\\Portals.wmo")?);
        if !loaded {
            // Native skips an unloaded neighbor. This boundary admits whole WMO
            // generations, so it must remain unavailable until that group loads.
            assert!(model.is_err());
            assert_eq!(words[15], "0");
        } else {
            let hit = probe_world_model_portals(&model?, 0, start, end, maximum)?;
            assert_eq!(hit.is_some(), words[15] == "1", "{line}");
            if let Some(hit) = hit {
                assert_eq!(
                    hit.fraction().to_bits(),
                    u32::from_str_radix(words[16], 16)?,
                    "{line}"
                );
                assert_eq!(hit.source_group(), words[17].parse::<usize>()?, "{line}");
                assert_eq!(
                    hit.destination_group(),
                    words[18].parse::<usize>()?,
                    "{line}"
                );
            }
        }
        count += 1;
    }
    assert_eq!(count, 138);
    Ok(())
}

pub(super) fn portal_fixture(
    normal: [f32; 3],
    distance: f32,
    side: i16,
    second: bool,
    geometry: usize,
) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let count = if second { 2 } else { 1 };
    let mut root = Vec::new();
    chunk(&mut root, *b"REVM", &17_u32.to_le_bytes());
    let mut header = vec![0; 64];
    word(&mut header, 4, 2);
    word(&mut header, 8, count);
    vector(&mut header, 36, [-16.; 3]);
    vector(&mut header, 48, [16.; 3]);
    chunk(&mut root, *b"DHOM", &header);
    let mut info = vec![0; 64];
    for index in 0..2 {
        vector(&mut info, index * 32 + 4, [-16.; 3]);
        vector(&mut info, index * 32 + 16, [16.; 3]);
        word(&mut info, index * 32 + 28, u32::MAX);
    }
    chunk(&mut root, *b"IGOM", &info);
    let mut vertices = Vec::new();
    let points = if geometry == 3 {
        [[-5., 1., 1.], [-1., -1., 1.], [5., -1., -1.], [1., 1., -1.]]
    } else {
        let (x, y) = [(1, 2), (2, 0), (0, 1)][geometry];
        [[-2., -2.], [2., -2.], [2., 2.], [-2., 2.]].map(|v| {
            let mut point = [0.; 3];
            point[x] = v[0];
            point[y] = v[1];
            point
        })
    };
    for vertex in points {
        for value in vertex {
            vertices.extend_from_slice(&f32::to_le_bytes(value));
        }
    }
    chunk(&mut root, *b"VPOM", &vertices);
    let mut portals = Vec::new();
    let mut references = Vec::new();
    for index in 0..count as u16 {
        let mut portal = vec![0; 20];
        short(&mut portal, 2, 4);
        vector(&mut portal, 4, normal);
        word(&mut portal, 16, distance.to_bits());
        portals.extend_from_slice(&portal);
        for value in [index, 1, (if index == 0 { side } else { -side }) as u16, 0] {
            references.extend_from_slice(&value.to_le_bytes());
        }
    }
    chunk(&mut root, *b"TPOM", &portals);
    chunk(&mut root, *b"RPOM", &references);
    let make_group = |portal_count| {
        let mut header = vec![0; 68];
        vector(&mut header, 12, [-16.; 3]);
        vector(&mut header, 24, [16.; 3]);
        short(&mut header, 38, portal_count);
        let mut bytes = Vec::new();
        chunk(&mut bytes, *b"REVM", &17_u32.to_le_bytes());
        chunk(&mut bytes, *b"PGOM", &header);
        bytes
    };
    (root, make_group(count as u16), make_group(0))
}

fn chunk(bytes: &mut Vec<u8>, magic: [u8; 4], data: &[u8]) {
    bytes.extend_from_slice(&magic);
    bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
    bytes.extend_from_slice(data);
}

fn word(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn short(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn vector(bytes: &mut [u8], offset: usize, value: [f32; 3]) {
    for (index, value) in value.into_iter().enumerate() {
        word(bytes, offset + index * 4, value.to_bits());
    }
}
