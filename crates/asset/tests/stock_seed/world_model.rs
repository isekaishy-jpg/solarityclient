//! External stock-compatibility tests for WMO root and group decoding.

use std::error::Error;
use std::sync::Arc;

use solarity_asset::{
    ArchiveCatalog, AssetError, AssetPath, AssetStore, ClientDataRoot, DecodedWorldModel, Locale,
    WmoModelCache, WorldModelBatchClass, WorldModelBlendMode, WorldModelShader,
};

use crate::support::{Fixture, FixtureFile};

/// Missing MOCV uses 7C8560's exact colors for both native vertex layouts.
#[test]
fn world_model_missing_vertex_colors_match_original_upload() -> Result<(), Box<dyn Error>> {
    let root = root_fixture(1);
    let group = group_fixture(0x08);
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "World/Colors.wmo",
            bytes: &root,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "World/Colors_000.wmo",
            bytes: &group,
        },
    ])?;
    let mut assets = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let model = DecodedWorldModel::load(&mut assets, &AssetPath::new("World/Colors.wmo")?)?;
    let group = &model.groups()[0];
    assert!(group.vertex_colors().is_empty());
    let mut cases = 0;
    for line in include_str!("../fixtures/wmo_vertex_colors_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let row = line.split_ascii_whitespace().collect::<Vec<_>>();
        if row[3] != "none" {
            continue;
        }
        let flags = u16::from_str_radix(row[0], 16)?;
        let expected = u32::from_str_radix(row[4], 16)?.to_le_bytes();
        for vertex in 0..group.vertices().len() {
            assert_eq!(
                group.fixed_vertex_color(flags, vertex),
                Some(expected),
                "{line}"
            );
        }
        assert_eq!(
            group.fixed_vertex_color(flags, group.vertices().len()),
            None
        );
        cases += 1;
    }
    assert_eq!(cases, 24);
    Ok(())
}

/// A version-17 root admits independently resolved collision-ready groups.
#[test]
fn world_model_loads_stock_group_geometry_and_bsp() -> Result<(), Box<dyn Error>> {
    let root_wmo = root_fixture(1);
    let group_wmo = group_fixture(0x08);
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "World\\Wmo\\Fixture.wmo",
            bytes: &root_wmo,
        },
        FixtureFile {
            archive: "patch-2.MPQ",
            path: "World\\Wmo\\Fixture_000.wmo",
            bytes: &group_wmo,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let path = AssetPath::new("World\\Wmo\\Fixture.wmo")?;

    let model = DecodedWorldModel::load(&mut store, &path)?;
    assert_eq!(model.path(), &path);
    assert_eq!(model.world_model_id(), 42);
    assert_eq!(model.flags(), 0x8);
    assert_eq!(model.bounds(), [[-2.0, -3.0, -4.0], [2.0, 3.0, 4.0]]);
    assert_eq!(model.group_info()[0].flags(), 0);
    assert_eq!(model.groups().len(), 1);
    let group = &model.groups()[0];
    assert_eq!(group.index(), 0);
    assert_eq!(group.path().as_str(), "WORLD\\WMO\\FIXTURE_000.WMO");
    assert_eq!(
        group.source().relative_path(),
        std::path::Path::new("patch-2.MPQ")
    );
    assert_eq!(
        group.vertices(),
        &[[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]]
    );
    assert_eq!(group.indices(), &[0, 1, 2]);
    assert_eq!(group.polygons().len(), 1);
    assert!(group.polygons()[0].is_collidable());
    assert!(group.polygons()[0].is_camera_collidable());
    assert!(!group.polygons()[0].is_renderable());
    assert_eq!(group.bsp_nodes().len(), 1);
    assert_eq!(group.bsp_nodes()[0].face_count(), 1);
    assert_eq!(group.bsp_faces(), &[0]);
    Ok(())
}

/// Nested reference lists survive MLIQ and retain strict root-table validation.
#[test]
fn world_model_decodes_and_validates_nested_group_references() -> Result<(), Box<dyn Error>> {
    let mut root_wmo = doodad_root_fixture();
    set_u32(&mut root_wmo, 20 + 4, 1);
    set_u32(&mut root_wmo, 20 + 12, 1);
    let mut info = vec![0_u8; 32];
    set_vec3(&mut info, 4, [-1.; 3]);
    set_vec3(&mut info, 16, [1.; 3]);
    set_u32(&mut info, 28, u32::MAX);
    push_chunk(&mut root_wmo, *b"IGOM", &info);
    push_chunk(&mut root_wmo, *b"TLOM", &[0; 48]);
    for (doodads, lights, expected_error) in [
        (&[1, 0, 0, 0, 1, 0][..], &[0, 0, 0, 0][..], None),
        (&[1][..], &[0, 0][..], Some("MODR requires complete u16")),
        (
            &[2, 0][..],
            &[0, 0][..],
            Some("MODR references a doodad outside MODD"),
        ),
        (
            &[0, 0][..],
            &[1, 0][..],
            Some("MOLR references a light outside MOLT"),
        ),
    ] {
        let mut group_wmo = group_fixture_with_liquid(8, 2, Some(&liquid_fixture()));
        let old_size = u32::from_le_bytes(group_wmo[16..20].try_into()?);
        push_chunk(&mut group_wmo, *b"RDOM", doodads);
        push_chunk(&mut group_wmo, *b"RLOM", lights);
        set_u32(
            &mut group_wmo,
            16,
            old_size + 16 + u32::try_from(doodads.len() + lights.len())?,
        );
        let fixture = Fixture::new(&[
            FixtureFile {
                archive: "common.MPQ",
                path: "World\\Wmo\\Fixture.wmo",
                bytes: &root_wmo,
            },
            FixtureFile {
                archive: "common.MPQ",
                path: "World\\Wmo\\Fixture_000.wmo",
                bytes: &group_wmo,
            },
        ])?;
        let mut store = AssetStore::mount(ArchiveCatalog::discover(
            ClientDataRoot::new(fixture.data_root())?,
            Locale::EnUs,
        )?)?;
        let loaded =
            DecodedWorldModel::load(&mut store, &AssetPath::new("World\\Wmo\\Fixture.wmo")?);
        if let Some(message) = expected_error {
            let Err(error) = loaded else {
                return Err("invalid group references were admitted".into());
            };
            assert!(error.to_string().contains(message), "{error}");
        } else {
            let model = loaded?;
            assert_eq!(model.groups()[0].doodad_references(), &[1, 0, 1]);
            assert_eq!(model.referenced_active_doodad_indices(0)?, [0]);
            assert_eq!(model.referenced_active_doodad_indices(1)?, [1, 0]);
            assert_eq!(model.groups()[0].light_references(), &[0, 0]);
        }
    }
    Ok(())
}

/// MOPY's no-camera flag remains distinct from ordinary world collision.
#[test]
fn world_model_preserves_stock_no_camera_collision_flag() -> Result<(), Box<dyn Error>> {
    let root_wmo = root_fixture(1);
    let group_wmo = group_fixture(0x0a);
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "World\\Wmo\\Fixture.wmo",
            bytes: &root_wmo,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "World\\Wmo\\Fixture_000.wmo",
            bytes: &group_wmo,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let model = DecodedWorldModel::load(&mut store, &AssetPath::new("World\\Wmo\\Fixture.wmo")?)?;

    assert!(model.groups()[0].polygons()[0].is_collidable());
    assert!(!model.groups()[0].polygons()[0].is_camera_collidable());
    Ok(())
}

/// Repeated placements retain one path-keyed root/group generation.
#[test]
fn world_model_cache_shares_and_collects_generations() -> Result<(), Box<dyn Error>> {
    let root_wmo = root_fixture(1);
    let group_wmo = group_fixture(0x08);
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "World\\Wmo\\Fixture.wmo",
            bytes: &root_wmo,
        },
        FixtureFile {
            archive: "patch-2.MPQ",
            path: "World\\Wmo\\Fixture_000.wmo",
            bytes: &group_wmo,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let path = AssetPath::new("World\\Wmo\\Fixture.wmo")?;
    let mut cache = WmoModelCache::new();

    let first = cache.load(&mut store, &path)?;
    let second = cache.load(&mut store, &path)?;
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.collect_unused(), 0);

    drop(first);
    drop(second);
    assert_eq!(cache.collect_unused(), 1);
    assert!(cache.is_empty());
    Ok(())
}

/// WotLK's 30-byte MLIQ header and overloaded vertex records remain exact.
#[test]
fn world_model_decodes_group_liquid_grid() -> Result<(), Box<dyn Error>> {
    let mut root_wmo = root_fixture(1);
    set_u16(&mut root_wmo, 80, 0x0c);
    let liquid = liquid_fixture();
    let group_wmo = group_fixture_with_liquid(0x08, 2, Some(&liquid));
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "World\\Wmo\\Fixture.wmo",
            bytes: &root_wmo,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "World\\Wmo\\Fixture_000.wmo",
            bytes: &group_wmo,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let model = DecodedWorldModel::load(&mut store, &AssetPath::new("World\\Wmo\\Fixture.wmo")?)?;
    let group = &model.groups()[0];
    let liquid = group.liquid().ok_or("fixture MLIQ was omitted")?;

    assert_eq!(liquid.vertex_width(), 2);
    assert_eq!(liquid.vertex_height(), 2);
    assert_eq!(liquid.tile_width(), 1);
    assert_eq!(liquid.tile_height(), 1);
    assert_eq!(liquid.corner(), [10.0, 20.0, 30.0]);
    assert_eq!(liquid.material_id(), 0);
    assert_eq!(liquid.tiles(), &[0x41]);
    assert_eq!(liquid.vertices().len(), 4);
    assert_eq!(liquid.vertices()[0].flow_one(), 1);
    assert_eq!(liquid.vertices()[0].flow_two(), 2);
    assert_eq!(liquid.vertices()[0].flow_one_percent(), 3);
    assert_eq!(liquid.vertices()[0].texture_s(), 0x0201);
    assert_eq!(liquid.vertices()[0].texture_t(), 0x0403);
    assert_eq!(liquid.vertices()[0].height(), 5.0);
    assert_eq!(group.resolve_liquid_type(model.flags()), 14);
    Ok(())
}

/// 7D82E0/7C8D80 resolve a single group type before any point is queried.
#[test]
fn world_model_liquid_type_uses_first_valid_tile_and_legacy_header_mapping()
-> Result<(), Box<dyn Error>> {
    let mut liquid = vec![0u8; 30];
    set_u32(&mut liquid, 0, 4);
    set_u32(&mut liquid, 4, 2);
    set_u32(&mut liquid, 8, 3);
    set_u32(&mut liquid, 12, 1);
    for _ in 0..8 {
        liquid.extend([0u8; 4]);
        liquid.extend(2f32.to_le_bytes());
    }
    liquid.extend([0x0f, 0x42, 0x41]);
    for (root_flags, header, group_flags, expected) in [
        (0, 0, 0, 13),
        (0, 15, 0, 19),
        (4, 0, 0, 19),
        (4, 1, 0, 13),
        (4, 1, 0x80000, 14),
        (4, 21, 0, 21),
    ] {
        let mut root = root_fixture(1);
        set_u16(&mut root, 80, root_flags);
        let mut group = group_fixture_with_liquid(8, header, Some(&liquid));
        set_u32(&mut group, 28, group_flags | 0x1000);
        let fixture = Fixture::new(&[
            FixtureFile {
                archive: "common.MPQ",
                path: "World\\Wmo\\Fixture.wmo",
                bytes: &root,
            },
            FixtureFile {
                archive: "common.MPQ",
                path: "World\\Wmo\\Fixture_000.wmo",
                bytes: &group,
            },
        ])?;
        let mut store = AssetStore::mount(ArchiveCatalog::discover(
            ClientDataRoot::new(fixture.data_root())?,
            Locale::EnUs,
        )?)?;
        let model =
            DecodedWorldModel::load(&mut store, &AssetPath::new("World\\Wmo\\Fixture.wmo")?)?;
        assert_eq!(
            model.groups()[0].resolve_liquid_type(model.flags()),
            expected
        );
    }
    Ok(())
}

/// Root MOMT and layered group chunks retain exact renderer-facing values.
#[test]
fn world_model_decodes_stock_presentation_tables() -> Result<(), Box<dyn Error>> {
    let root_wmo = presentation_root_fixture();
    let group_wmo = presentation_group_fixture();
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "World\\Wmo\\Presentation.wmo",
            bytes: &root_wmo,
        },
        FixtureFile {
            archive: "patch-2.MPQ",
            path: "World\\Wmo\\Presentation_000.wmo",
            bytes: &group_wmo,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let model =
        DecodedWorldModel::load(&mut store, &AssetPath::new("World\\Wmo\\Presentation.wmo")?)?;

    assert_eq!(model.ambient_color(), [0x40, 0x30, 0x20, 0xff]);
    assert_eq!(model.materials().len(), 2);
    let composite = &model.materials()[0];
    assert_eq!(composite.flags(), 0xc1);
    assert_eq!(composite.authored_shader(), WorldModelShader::Composite);
    assert_eq!(composite.shader(), WorldModelShader::Composite);
    assert_eq!(composite.blend_mode(), WorldModelBlendMode::Alpha);
    assert_eq!(composite.texture_offsets(), [0, 9, 20]);
    assert_eq!(
        composite.textures()[0].as_ref().map(AssetPath::as_str),
        Some("WALL.BLP")
    );
    assert_eq!(
        composite.textures()[1].as_ref().map(AssetPath::as_str),
        Some("DETAIL.BLP")
    );
    assert!(composite.textures()[2].is_none());
    assert_eq!(composite.emissive_color(), 0x1122_3344);
    assert_eq!(composite.diffuse_color(), 0x5566_7788);
    assert_eq!(composite.ground_type(), 19);
    assert_eq!(composite.secondary_color(), 0x99aa_bbcc);
    assert_eq!(composite.secondary_flags(), 0x1020_3040);
    assert_eq!(composite.runtime_data(), [0xa5; 16]);
    let normalized = &model.materials()[1];
    assert_eq!(normalized.authored_shader(), WorldModelShader::Environment);
    assert_eq!(normalized.shader(), WorldModelShader::Opaque);
    assert!(normalized.textures()[1].is_none());

    let group = &model.groups()[0];
    assert_eq!(group.portal_reference_start(), 3);
    assert_eq!(group.portal_reference_count(), 2);
    assert_eq!(group.batch_counts(), [1, 0, 0, 7]);
    assert_eq!(group.fog_ids(), [1, 2, 3, 4]);
    assert_eq!(model.fogs().len(), 5);
    for (index, fog) in model.fogs().iter().enumerate() {
        assert_eq!(fog.flags(), 0x100 + index as u32);
        assert_eq!(fog.position().to_array(), [index as f32, 2., 3.]);
        assert_eq!(fog.radii(), (4., 5.));
        assert_eq!(fog.banks()[0].range(), (100. + index as f32, 0.25));
        assert_eq!(fog.banks()[0].packed_color(), 0xff12_3456);
        assert_eq!(fog.banks()[1].range(), (50. + index as f32, 0.5));
        assert_eq!(fog.banks()[1].packed_color(), 0xffab_cdef);
    }
    assert_eq!(group.area_table_id(), 42);
    assert_eq!(group.normals(), &[[0.0, 0.0, 1.0]; 3]);
    assert_eq!(group.texture_coordinates().len(), 2);
    assert!(group.texture_coordinates()[0][0][0].is_nan());
    assert_eq!(group.texture_coordinates()[1][2], [0.25, 0.75]);
    assert_eq!(group.vertex_colors().len(), 2);
    assert_eq!(group.vertex_colors()[0][1], [128, 96, 64, 255]);
    assert_eq!(group.vertex_colors()[1][2], [1, 2, 3, 4]);
    assert_eq!(group.batches().len(), 1);
    let batch = group.batches()[0];
    assert_eq!(batch.bounds(), [[-1, -2, -3], [4, 5, 6]]);
    assert_eq!(batch.first_index(), 0);
    assert_eq!(batch.index_count(), 3);
    assert_eq!(batch.vertex_range(), [0, 2]);
    assert_eq!(batch.flags(), 0x12);
    assert_eq!(batch.material_id(), 0);
    assert_eq!(batch.class(), WorldModelBatchClass::Transition);
    Ok(())
}

/// Root MODN/MODS/MODD tables retain exact paths, ranges, and transforms.
#[test]
fn world_model_decodes_stock_doodad_sets() -> Result<(), Box<dyn Error>> {
    let root_wmo = doodad_root_fixture();
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "World\\Wmo\\Doodads.wmo",
        bytes: &root_wmo,
    }])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let model = DecodedWorldModel::load(&mut store, &AssetPath::new("World\\Wmo\\Doodads.wmo")?)?;

    assert_eq!(model.doodad_sets().len(), 2);
    assert_eq!(model.doodad_sets()[0].name(), "Set_$DefaultGlobal");
    assert_eq!(model.doodad_sets()[0].first_doodad(), 0);
    assert_eq!(model.doodad_sets()[0].doodad_count(), 1);
    assert_eq!(model.doodad_sets()[0].padding(), 0x1122_3344);
    assert_eq!(model.doodad_sets()[1].name(), "FURNITURE");
    assert_eq!(model.doodad_sets()[1].first_doodad(), 1);
    assert_eq!(model.doodad_sets()[1].doodad_count(), 1);

    assert_eq!(model.doodads().len(), 2);
    let tree = &model.doodads()[0];
    assert_eq!(tree.path().as_str(), "TREE.MDX");
    assert_eq!(tree.name_offset(), 0);
    assert_eq!(tree.flags(), 0x03);
    assert_eq!(tree.position(), [1.0, 2.0, 3.0]);
    assert_eq!(tree.orientation(), [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(tree.scale(), 1.5);
    assert_eq!(tree.color(), [10, 20, 30, 40]);
    let lamp = &model.doodads()[1];
    assert_eq!(lamp.path().as_str(), "LAMP.M2");
    assert_eq!(lamp.name_offset(), 9);
    assert_eq!(lamp.flags(), 0x80);
    assert_eq!(lamp.orientation(), [0.0, 0.0, 1.0, 0.0]);
    assert_eq!(model.active_doodad_indices(0)?, [0]);
    assert_eq!(model.active_doodad_indices(1)?, [0, 1]);
    assert!(model.referenced_active_doodad_indices(1)?.is_empty());
    let error = match model.active_doodad_indices(2) {
        Ok(_) => return Err("selector outside MODS was accepted".into()),
        Err(error) => error,
    };
    assert_eq!(error.path(), model.path());
    assert_eq!(error.selector(), 2);
    assert_eq!(error.set_count(), 2);
    Ok(())
}

/// Exporter-era MOHD doodad counts remain advisory when MODD is smaller.
#[test]
fn world_model_accepts_stock_inflated_mohd_doodad_count() -> Result<(), Box<dyn Error>> {
    let mut root_wmo = doodad_root_fixture();
    set_u32(&mut root_wmo, 40, 9);
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "World\\Wmo\\Doodads.wmo",
        bytes: &root_wmo,
    }])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let model = DecodedWorldModel::load(&mut store, &AssetPath::new("World\\Wmo\\Doodads.wmo")?)?;

    assert_eq!(model.doodads().len(), 2);
    assert_eq!(model.active_doodad_indices(1)?, [0, 1]);
    Ok(())
}

/// Build-12340 transport roots retain complete, finite MCVP plane records.
#[test]
fn world_model_retains_stock_transport_convex_volume_planes() -> Result<(), Box<dyn Error>> {
    let planes = [[2.0_f32, 0.0, 0.0, -4.0], [0.0, -3.0, 1.0, 8.0]];
    let valid = planes
        .into_iter()
        .flatten()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    let mut non_finite = valid.clone();
    non_finite[..4].copy_from_slice(&f32::NAN.to_le_bytes());
    for (payload, duplicate, expected_error) in [
        (&valid[..], false, None),
        (&[][..], false, None),
        (
            &valid[..15],
            false,
            Some("MCVP requires complete 16-byte records"),
        ),
        (
            &non_finite[..],
            false,
            Some("MCVP contains a non-finite plane"),
        ),
        (&valid[..], true, Some("WMO root repeats chunk MCVP")),
    ] {
        let mut root_wmo = root_fixture(0);
        push_chunk(&mut root_wmo, *b"PVCM", payload);
        if duplicate {
            push_chunk(&mut root_wmo, *b"PVCM", payload);
        }
        let fixture = Fixture::new(&[FixtureFile {
            archive: "common.MPQ",
            path: "World\\Wmo\\Fixture.wmo",
            bytes: &root_wmo,
        }])?;
        let mut store = AssetStore::mount(ArchiveCatalog::discover(
            ClientDataRoot::new(fixture.data_root())?,
            Locale::EnUs,
        )?)?;
        let result =
            DecodedWorldModel::load(&mut store, &AssetPath::new("World\\Wmo\\Fixture.wmo")?);
        if let Some(expected_error) = expected_error {
            assert!(
                matches!(result, Err(AssetError::WorldModelDecode { message, .. }) if message.contains(expected_error))
            );
        } else {
            let model = result?;
            assert_eq!(
                model.convex_volume_planes(),
                if payload.is_empty() {
                    &[][..]
                } else {
                    &planes[..]
                }
            );
        }
    }
    Ok(())
}

/// Post-build chunks fail before the dependency can silently skip them.
#[test]
fn world_model_rejects_unknown_root_chunks() -> Result<(), Box<dyn Error>> {
    let mut root_wmo = root_fixture(0);
    push_chunk(&mut root_wmo, *b"DIAM", &[]);
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "World\\Wmo\\Fixture.wmo",
        bytes: &root_wmo,
    }])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let path = AssetPath::new("World\\Wmo\\Fixture.wmo")?;

    assert!(matches!(
        DecodedWorldModel::load(&mut store, &path),
        Err(AssetError::WorldModelDecode { message, .. })
            if message.contains("unknown build-12340 chunk")
    ));
    Ok(())
}

#[test]
fn world_model_rejects_invalid_fog_records_and_group_indices() -> Result<(), Box<dyn Error>> {
    for (size, nonfinite, group_id, expected) in [
        (47, false, 0, "MFOG requires complete 48-byte records"),
        (48, true, 0, "MFOG contains a nonfinite scalar"),
        (48, false, 1, "MOGP fog index exceeds the MFOG table"),
    ] {
        let mut root = root_fixture(1);
        let mut fog = vec![0; size];
        if nonfinite {
            set_u32(&mut fog, 36, f32::NAN.to_bits());
        }
        push_chunk(&mut root, *b"GOFM", &fog);
        let mut group = group_fixture(0x08);
        group[20 + 48] = group_id;
        let fixture = Fixture::new(&[
            FixtureFile {
                archive: "common.MPQ",
                path: "World\\Fog.wmo",
                bytes: &root,
            },
            FixtureFile {
                archive: "common.MPQ",
                path: "World\\Fog_000.wmo",
                bytes: &group,
            },
        ])?;
        let mut store = AssetStore::mount(ArchiveCatalog::discover(
            ClientDataRoot::new(fixture.data_root())?,
            Locale::EnUs,
        )?)?;
        let error = DecodedWorldModel::load(&mut store, &AssetPath::new("World\\Fog.wmo")?)
            .err()
            .ok_or("invalid fog accepted")?;
        assert!(
            error.to_string().contains(expected),
            "{error}: expected {expected}"
        );
    }
    Ok(())
}

fn root_fixture(group_count: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    push_chunk(&mut bytes, *b"REVM", &17_u32.to_le_bytes());
    let mut header = vec![0_u8; 64];
    set_u32(&mut header, 4, group_count);
    set_u32(&mut header, 32, 42);
    set_vec3(&mut header, 36, [-2.0, -3.0, -4.0]);
    set_vec3(&mut header, 48, [2.0, 3.0, 4.0]);
    set_u16(&mut header, 60, 0x8);
    push_chunk(&mut bytes, *b"DHOM", &header);
    let mut groups = Vec::with_capacity(group_count as usize * 32);
    for _ in 0..group_count {
        groups.extend_from_slice(&0_u32.to_le_bytes());
        for value in [-1.0_f32, -1.0, -1.0, 1.0, 1.0, 1.0] {
            groups.extend_from_slice(&value.to_le_bytes());
        }
        groups.extend_from_slice(&(-1_i32).to_le_bytes());
    }
    push_chunk(&mut bytes, *b"IGOM", &groups);
    bytes
}

fn presentation_root_fixture() -> Vec<u8> {
    let mut bytes = Vec::new();
    push_chunk(&mut bytes, *b"REVM", &17_u32.to_le_bytes());
    let mut header = vec![0_u8; 64];
    set_u32(&mut header, 0, 2);
    set_u32(&mut header, 4, 1);
    set_u32(&mut header, 28, 0xff20_3040);
    set_u32(&mut header, 32, 42);
    set_vec3(&mut header, 36, [-2.0, -3.0, -4.0]);
    set_vec3(&mut header, 48, [2.0, 3.0, 4.0]);
    set_u16(&mut header, 60, 0x02);
    push_chunk(&mut bytes, *b"DHOM", &header);
    let textures = b"wall.blp\0detail.blp\0\0";
    push_chunk(&mut bytes, *b"XTOM", textures);

    let mut materials = vec![0_u8; 128];
    set_u32(&mut materials, 0, 0xc1);
    set_u32(&mut materials, 4, 6);
    set_u32(&mut materials, 8, 2);
    set_u32(&mut materials, 12, 0);
    set_u32(&mut materials, 16, 0x1122_3344);
    set_u32(&mut materials, 24, 9);
    set_u32(&mut materials, 28, 0x5566_7788);
    set_u32(&mut materials, 32, 19);
    set_u32(&mut materials, 36, 20);
    set_u32(&mut materials, 40, 0x99aa_bbcc);
    set_u32(&mut materials, 44, 0x1020_3040);
    materials[48..64].fill(0xa5);
    set_u32(&mut materials, 64 + 4, 3);
    set_u32(&mut materials, 64 + 12, 0);
    set_u32(&mut materials, 64 + 24, 20);
    push_chunk(&mut bytes, *b"TMOM", &materials);

    let mut group = vec![0_u8; 32];
    set_vec3(&mut group, 4, [-1.0, -1.0, -1.0]);
    set_vec3(&mut group, 16, [1.0, 1.0, 1.0]);
    set_u32(&mut group, 28, u32::MAX);
    push_chunk(&mut bytes, *b"IGOM", &group);
    append_portals(&mut bytes, &[[0, 0, 1, 0]; 5]);
    let mut fogs = Vec::new();
    for index in 0..5 {
        let mut fog = [0; 48];
        set_u32(&mut fog, 0, 0x100 + index);
        set_vec3(&mut fog, 4, [index as f32, 2., 3.]);
        for (offset, value) in [
            (16, 4f32),
            (20, 5.),
            (24, 100. + index as f32),
            (28, 0.25),
            (36, 50. + index as f32),
            (40, 0.5),
        ] {
            set_u32(&mut fog, offset, value.to_bits());
        }
        set_u32(&mut fog, 32, 0xff12_3456);
        set_u32(&mut fog, 44, 0xffab_cdef);
        fogs.extend(fog);
    }
    push_chunk(&mut bytes, *b"GOFM", &fogs);
    bytes
}

fn append_portals(bytes: &mut Vec<u8>, references: &[[u16; 4]]) -> [usize; 3] {
    set_u32(bytes, 20 + 8, 1);
    let mut vertices = Vec::new();
    for vertex in [
        [91., 92., 93.],
        [-1., -1., 3.],
        [1., -1., 3.],
        [1., 1., 3.],
        [-1., 1., 3.],
    ] {
        for value in vertex {
            vertices.extend_from_slice(&f32::to_le_bytes(value));
        }
    }
    let vertex_offset = bytes.len() + 8;
    push_chunk(bytes, *b"VPOM", &vertices);
    let mut portal = vec![0; 20];
    set_u16(&mut portal, 0, 1);
    set_u16(&mut portal, 2, 4);
    set_vec3(&mut portal, 4, [0., 0., 2.]);
    portal[16..20].copy_from_slice(&(-6.0_f32).to_le_bytes());
    let portal_offset = bytes.len() + 8;
    push_chunk(bytes, *b"TPOM", &portal);
    let edges = references
        .iter()
        .flatten()
        .flat_map(|value| value.to_le_bytes())
        .collect::<Vec<_>>();
    let reference_offset = bytes.len() + 8;
    push_chunk(bytes, *b"RPOM", &edges);
    [vertex_offset, portal_offset, reference_offset]
}

fn doodad_root_fixture() -> Vec<u8> {
    let mut bytes = Vec::new();
    push_chunk(&mut bytes, *b"REVM", &17_u32.to_le_bytes());
    let mut header = vec![0_u8; 64];
    set_u32(&mut header, 16, 2);
    set_u32(&mut header, 20, 2);
    set_u32(&mut header, 24, 2);
    set_u32(&mut header, 32, 77);
    set_vec3(&mut header, 36, [-2.0, -3.0, -4.0]);
    set_vec3(&mut header, 48, [2.0, 3.0, 4.0]);
    push_chunk(&mut bytes, *b"DHOM", &header);
    push_chunk(&mut bytes, *b"NDOM", b"Tree.mdx\0Lamp.m2\0");

    let mut doodads = Vec::new();
    push_doodad(
        &mut doodads,
        0x0300_0000,
        [1.0, 2.0, 3.0],
        [0.0, 0.0, 0.0, 1.0],
        1.5,
        [10, 20, 30, 40],
    );
    push_doodad(
        &mut doodads,
        0x8000_0009,
        [-1.0, -2.0, -3.0],
        [0.0, 0.0, 1.0, 0.0],
        0.5,
        [50, 60, 70, 80],
    );
    push_chunk(&mut bytes, *b"DDOM", &doodads);

    let mut sets = Vec::new();
    push_doodad_set(&mut sets, "Set_$DefaultGlobal", 0, 1, 0x1122_3344);
    push_doodad_set(&mut sets, "FURNITURE", 1, 1, 0);
    push_chunk(&mut bytes, *b"SDOM", &sets);
    bytes
}

fn push_doodad(
    bytes: &mut Vec<u8>,
    name_and_flags: u32,
    position: [f32; 3],
    orientation: [f32; 4],
    scale: f32,
    color: [u8; 4],
) {
    bytes.extend_from_slice(&name_and_flags.to_le_bytes());
    for value in position.into_iter().chain(orientation) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&scale.to_le_bytes());
    bytes.extend_from_slice(&color);
}

fn push_doodad_set(
    bytes: &mut Vec<u8>,
    name: &str,
    first_doodad: u32,
    doodad_count: u32,
    padding: u32,
) {
    let mut encoded_name = [0_u8; 20];
    encoded_name[..name.len()].copy_from_slice(name.as_bytes());
    bytes.extend_from_slice(&encoded_name);
    bytes.extend_from_slice(&first_doodad.to_le_bytes());
    bytes.extend_from_slice(&doodad_count.to_le_bytes());
    bytes.extend_from_slice(&padding.to_le_bytes());
}

fn presentation_group_fixture() -> Vec<u8> {
    let mut nested = Vec::new();
    push_chunk(&mut nested, *b"YPOM", &[0x20, 0]);
    let mut indices = Vec::new();
    for index in [0_u16, 1, 2] {
        indices.extend_from_slice(&index.to_le_bytes());
    }
    push_chunk(&mut nested, *b"IVOM", &indices);
    let mut vertices = Vec::new();
    for vertex in [[0.0_f32, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]] {
        for value in vertex {
            vertices.extend_from_slice(&value.to_le_bytes());
        }
    }
    push_chunk(&mut nested, *b"TVOM", &vertices);
    let mut normals = Vec::new();
    for _ in 0..3 {
        for value in [0.0_f32, 0.0, 1.0] {
            normals.extend_from_slice(&value.to_le_bytes());
        }
    }
    push_chunk(&mut nested, *b"RNOM", &normals);
    let mut first_uv = Vec::new();
    for value in [f32::from_bits(0xffff_fc00), 0.0, 1.0, 0.0, 0.0, 1.0] {
        first_uv.extend_from_slice(&value.to_le_bytes());
    }
    push_chunk(&mut nested, *b"VTOM", &first_uv);
    let mut second_uv = Vec::new();
    for value in [0.0_f32, 0.0, 0.5, 0.5, 0.25, 0.75] {
        second_uv.extend_from_slice(&value.to_le_bytes());
    }
    push_chunk(&mut nested, *b"VTOM", &second_uv);
    push_chunk(
        &mut nested,
        *b"VCOM",
        &[255, 255, 255, 255, 128, 96, 64, 255, 32, 16, 8, 255],
    );
    push_chunk(&mut nested, *b"VCOM", &[4, 3, 2, 1, 8, 7, 6, 5, 1, 2, 3, 4]);
    let mut batch = vec![0_u8; 24];
    for (value, offset) in [(-1_i16, 0), (-2, 2), (-3, 4), (4, 6), (5, 8), (6, 10)] {
        set_i16(&mut batch, offset, value);
    }
    set_u32(&mut batch, 12, 0);
    set_u16(&mut batch, 16, 3);
    set_u16(&mut batch, 18, 0);
    set_u16(&mut batch, 20, 2);
    batch[22] = 0x12;
    batch[23] = 0;
    push_chunk(&mut nested, *b"ABOM", &batch);

    let mut group = vec![0_u8; 68];
    set_u32(&mut group, 8, 0x3005);
    set_vec3(&mut group, 12, [-1.0, -2.0, -3.0]);
    set_vec3(&mut group, 24, [4.0, 5.0, 6.0]);
    set_u16(&mut group, 36, 3);
    set_u16(&mut group, 38, 2);
    set_u16(&mut group, 40, 1);
    set_u16(&mut group, 46, 7);
    group[48..52].copy_from_slice(&[1, 2, 3, 4]);
    set_u32(&mut group, 56, 42);
    group.extend_from_slice(&nested);
    let mut bytes = Vec::new();
    push_chunk(&mut bytes, *b"REVM", &17_u32.to_le_bytes());
    push_chunk(&mut bytes, *b"PGOM", &group);
    bytes
}

fn group_fixture(polygon_flags: u8) -> Vec<u8> {
    group_fixture_with_liquid(polygon_flags, 0, None)
}

fn group_fixture_with_liquid(
    polygon_flags: u8,
    liquid_type: u32,
    liquid: Option<&[u8]>,
) -> Vec<u8> {
    let mut nested = Vec::new();
    push_chunk(&mut nested, *b"YPOM", &[polygon_flags, 0xff]);
    let mut indices = Vec::new();
    for index in [0_u16, 1, 2] {
        indices.extend_from_slice(&index.to_le_bytes());
    }
    push_chunk(&mut nested, *b"IVOM", &indices);
    let mut vertices = Vec::new();
    for vertex in [[0.0_f32, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]] {
        for value in vertex {
            vertices.extend_from_slice(&value.to_le_bytes());
        }
    }
    push_chunk(&mut nested, *b"TVOM", &vertices);
    let mut normals = Vec::new();
    for _ in 0..3 {
        for value in [0.0_f32, 0.0, 1.0] {
            normals.extend_from_slice(&value.to_le_bytes());
        }
    }
    push_chunk(&mut nested, *b"RNOM", &normals);
    let mut node = Vec::new();
    node.extend_from_slice(&4_u16.to_le_bytes());
    node.extend_from_slice(&(-1_i16).to_le_bytes());
    node.extend_from_slice(&(-1_i16).to_le_bytes());
    node.extend_from_slice(&1_u16.to_le_bytes());
    node.extend_from_slice(&0_u32.to_le_bytes());
    node.extend_from_slice(&0.0_f32.to_le_bytes());
    push_chunk(&mut nested, *b"NBOM", &node);
    push_chunk(&mut nested, *b"RBOM", &0_u16.to_le_bytes());
    if let Some(liquid) = liquid {
        push_chunk(&mut nested, *b"QILM", liquid);
    }

    let mut group = vec![0_u8; 68];
    set_vec3(&mut group, 12, [-1.0, -1.0, -1.0]);
    set_vec3(&mut group, 24, [1.0, 1.0, 1.0]);
    set_u32(&mut group, 52, liquid_type);
    group.extend_from_slice(&nested);
    let mut bytes = Vec::new();
    push_chunk(&mut bytes, *b"REVM", &17_u32.to_le_bytes());
    push_chunk(&mut bytes, *b"PGOM", &group);
    bytes
}

fn liquid_fixture() -> Vec<u8> {
    let mut bytes = vec![0_u8; 30];
    set_u32(&mut bytes, 0, 2);
    set_u32(&mut bytes, 4, 2);
    set_u32(&mut bytes, 8, 1);
    set_u32(&mut bytes, 12, 1);
    set_vec3(&mut bytes, 16, [10.0, 20.0, 30.0]);
    set_u16(&mut bytes, 28, 0);
    for (index, height) in [5.0_f32, 6.0, 7.0, 8.0].into_iter().enumerate() {
        bytes.extend_from_slice(&[1, 2, 3, 4]);
        bytes.extend_from_slice(&height.to_le_bytes());
        assert_eq!(bytes.len(), 30 + (index + 1) * 8);
    }
    bytes.push(0x41);
    bytes
}

fn push_chunk(bytes: &mut Vec<u8>, magic: [u8; 4], payload: &[u8]) {
    bytes.extend_from_slice(&magic);
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(payload);
}

fn set_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn set_i16(bytes: &mut [u8], offset: usize, value: i16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn set_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn set_vec3(bytes: &mut [u8], offset: usize, value: [f32; 3]) {
    for (axis, value) in value.into_iter().enumerate() {
        bytes[offset + axis * 4..offset + axis * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
}

#[test]
fn world_model_preserves_distinct_group_flags_and_portal_topology() -> Result<(), Box<dyn Error>> {
    let mut root = root_fixture(2);
    // MOGI is independent of the flags in each separately decoded group.
    set_u32(&mut root, 92, 0x0041_0088);
    append_portals(&mut root, &[[0, 1, u16::MAX, 0xbeef], [0, 0, 1, 0xabcd]]);
    let mut first = group_fixture(8);
    let mut second = group_fixture(8);
    set_u16(&mut first, 20 + 38, 1);
    set_u16(&mut second, 20 + 36, 1);
    set_u16(&mut second, 20 + 38, 1);
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "World\\Portals.wmo",
            bytes: &root,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "World\\Portals_000.wmo",
            bytes: &first,
        },
        FixtureFile {
            archive: "patch-2.MPQ",
            path: "World\\Portals_001.wmo",
            bytes: &second,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let model = DecodedWorldModel::load(&mut store, &AssetPath::new("World\\Portals.wmo")?)?;
    assert_eq!(model.group_info()[0].flags(), 0x0041_0088);
    assert_eq!(model.groups()[0].flags(), 0);
    assert_eq!(model.group_info()[0].bounds(), [[-1.; 3], [1.; 3]]);
    assert_eq!(model.portal_vertices().len(), 5);
    assert_eq!(model.portal_vertices()[0], [91., 92., 93.]);
    let portal = model.portals()[0];
    assert_eq!((portal.vertex_start(), portal.vertex_count()), (1, 4));
    assert_eq!(portal.normal(), [0., 0., 2.]);
    assert_eq!(portal.distance(), -6.);
    let references = model.portal_references();
    assert_eq!(
        (references[0].portal_index(), references[0].group_index()),
        (0, 1)
    );
    assert_eq!(
        (references[0].side(), references[0].padding()),
        (-1, 0xbeef)
    );
    assert_eq!((references[1].side(), references[1].padding()), (1, 0xabcd));
    assert_eq!(model.groups()[0].portal_reference_start(), 0);
    assert_eq!(model.groups()[1].portal_reference_start(), 1);
    assert_eq!(model.groups()[1].portal_reference_count(), 1);
    Ok(())
}

#[test]
fn world_model_rejects_broken_portal_tables_before_spatial_queries() -> Result<(), Box<dyn Error>> {
    for (case, expected) in [
        (0, "MOPT vertex range exceeds MOPV"),
        (1, "MOPR references a portal outside MOPT"),
        (2, "MOPR references a group outside MOGI"),
        (3, "MOGP portal range exceeds root MOPR"),
        (4, "MOPV contains a non-finite vertex"),
        (5, "MOPT contains a non-finite plane"),
        (6, "MOPV requires complete 12-byte records"),
        (7, "MOPT requires complete 20-byte records"),
        (8, "MOPR requires complete 8-byte records"),
    ] {
        let mut root = root_fixture(1);
        let [vertices, portal, edges] = append_portals(&mut root, &[[0, 0, u16::MAX, 0]]);
        let mut group = group_fixture(8);
        set_u16(&mut group, 20 + 38, 1);
        match case {
            0 => set_u16(&mut root, portal, 2),
            1 => set_u16(&mut root, edges, 1),
            2 => set_u16(&mut root, edges + 2, 1),
            3 => set_u16(&mut group, 20 + 36, 1),
            4 => set_u32(&mut root, vertices, f32::NAN.to_bits()),
            5 => set_u32(&mut root, portal + 16, f32::INFINITY.to_bits()),
            6..=8 => {
                let (offset, size) = [(vertices, 60), (portal, 20), (edges, 8)][case - 6];
                set_u32(&mut root, offset - 4, size - 1);
                root.remove(offset + size as usize - 1);
            }
            _ => unreachable!(),
        }
        let fixture = Fixture::new(&[
            FixtureFile {
                archive: "common.MPQ",
                path: "World\\Portals.wmo",
                bytes: &root,
            },
            FixtureFile {
                archive: "common.MPQ",
                path: "World\\Portals_000.wmo",
                bytes: &group,
            },
        ])?;
        let mut store = AssetStore::mount(ArchiveCatalog::discover(
            ClientDataRoot::new(fixture.data_root())?,
            Locale::EnUs,
        )?)?;
        assert!(
            matches!(
                DecodedWorldModel::load(&mut store, &AssetPath::new("World\\Portals.wmo")?),
                Err(AssetError::WorldModelDecode { message, .. }) if message.contains(expected)
            ),
            "portal case {case}"
        );
    }
    Ok(())
}
