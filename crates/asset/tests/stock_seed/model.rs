//! External stock-compatibility tests for the build-12340 M2 boundary.

use std::error::Error;
use std::io::Cursor;

use solarity_asset::{
    ArchiveCatalog, AssetError, AssetPath, AssetStore, ClientDataRoot, DecodedM2Model, Locale,
};
use wow_m2::chunks::vertex::M2Vertex as RawM2Vertex;
use wow_m2::common::{C2Vector, C3Vector};
use wow_m2::header::M2Header;
use wow_m2::skin::{OldSkinHeader, SkinSubmesh};
use wow_m2::{M2Model, M2Version, OldSkin};

use crate::support::{Fixture, FixtureFile};

/// HD model packs replace the same M2 and SKIN names through MPQ priority.
#[test]
fn higher_priority_model_pack_replaces_stock_paths_without_an_hd_type() -> Result<(), Box<dyn Error>>
{
    let stock_model = m2_bytes("StockModel", 2)?;
    let hd_model = m2_bytes("HdModel", 2)?;
    let stock_skin = skin_bytes(32, &[0, 1, 2])?;
    let hd_skin = skin_bytes(72, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\Solarity.m2",
            bytes: &stock_model,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\Solarity00.skin",
            bytes: &stock_skin,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\Solarity01.skin",
            bytes: &stock_skin,
        },
        FixtureFile {
            archive: "patch-A.MPQ",
            path: "Creature\\Solarity\\Solarity.m2",
            bytes: &hd_model,
        },
        FixtureFile {
            archive: "patch-A.MPQ",
            path: "Creature\\Solarity\\Solarity00.skin",
            bytes: &hd_skin,
        },
        FixtureFile {
            archive: "patch-A.MPQ",
            path: "Creature\\Solarity\\Solarity01.skin",
            bytes: &hd_skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature/Solarity/Solarity.m2")?;
    let model = DecodedM2Model::load(&mut store, &path)?;

    assert_eq!(model.name(), Some("HdModel"));
    assert_eq!(model.vertices().len(), 3);
    assert_eq!(model.skins().len(), 2);
    assert_eq!(
        model.skins()[0].path().as_str(),
        "CREATURE\\SOLARITY\\SOLARITY00.SKIN"
    );
    assert_eq!(
        model.skins()[1].path().as_str(),
        "CREATURE\\SOLARITY\\SOLARITY01.SKIN"
    );
    assert_eq!(model.skins()[0].bone_count_max(), 72);
    assert_eq!(
        model.source().relative_path().to_string_lossy(),
        "patch-A.MPQ"
    );
    assert_eq!(
        model.skins()[0].source().relative_path().to_string_lossy(),
        "patch-A.MPQ"
    );

    // The pinned decoder normally repairs these unused invalid indices because
    // the fixture has no bones. Our stock boundary must preserve the file bytes.
    assert_eq!(model.vertices()[0].bone_weights(), [0, 0, 0, 0]);
    assert_eq!(model.vertices()[0].bone_indices(), [7, 8, 9, 10]);
    assert_eq!(model.skins()[0].vertex_lookup(), &[0, 1, 2]);
    assert_eq!(model.skins()[0].triangle_lookup(), &[0, 1, 2]);
    assert_eq!(model.skins()[0].submeshes()[0].center_bone_index, 5);
    Ok(())
}

/// Build 12340 must not silently accept a later M2 header revision.
#[test]
fn later_m2_version_is_rejected_before_companion_lookup() -> Result<(), Box<dyn Error>> {
    let mut model = m2_bytes("LaterModel", 1)?;
    model[4..8].copy_from_slice(&272_u32.to_le_bytes());
    let fixture = Fixture::new(&[FixtureFile {
        archive: "patch-A.MPQ",
        path: "Creature\\Solarity\\Later.m2",
        bytes: &model,
    }])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature/Solarity/Later.m2")?;

    assert!(matches!(
        DecodedM2Model::load(&mut store, &path),
        Err(AssetError::ModelDecode { path: failed, message })
            if failed == path && message.contains("expected M2 version 264")
    ));
    Ok(())
}

/// Every external profile named by the WotLK M2 header is mandatory.
#[test]
fn absent_external_skin_is_reported_at_its_derived_stock_path() -> Result<(), Box<dyn Error>> {
    let model = m2_bytes("MissingSkin", 1)?;
    let fixture = Fixture::new(&[FixtureFile {
        archive: "patch-A.MPQ",
        path: "Creature\\Solarity\\Missing.m2",
        bytes: &model,
    }])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature/Solarity/Missing.m2")?;

    assert!(matches!(
        DecodedM2Model::load(&mut store, &path),
        Err(AssetError::AssetNotFound { path: failed })
            if failed.as_str() == "CREATURE\\SOLARITY\\MISSING00.SKIN"
    ));
    Ok(())
}

/// Triangle entries index the SKIN vertex lookup rather than the M2 directly.
#[test]
fn skin_triangle_reference_outside_vertex_lookup_is_rejected() -> Result<(), Box<dyn Error>> {
    let model = m2_bytes("BadSkin", 1)?;
    let skin = skin_bytes(32, &[0, 1, 3])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "patch-A.MPQ",
            path: "Creature\\Solarity\\Bad.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "patch-A.MPQ",
            path: "Creature\\Solarity\\Bad00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let model_path = AssetPath::new("Creature/Solarity/Bad.m2")?;

    assert!(matches!(
        DecodedM2Model::load(&mut store, &model_path),
        Err(AssetError::ModelDecode { path, message })
            if path.as_str() == "CREATURE\\SOLARITY\\BAD00.SKIN"
                && message.contains("missing profile vertex")
    ));
    Ok(())
}

/// The SKIN level word supplies high triangle-start bits for large profiles.
#[test]
fn extended_triangle_start_loads_large_hd_profile() -> Result<(), Box<dyn Error>> {
    const EXTENDED_TRIANGLE_COUNT: usize = 65_541;

    let model = m2_bytes("LargeSkin", 1)?;
    let triangles = vec![0; EXTENDED_TRIANGLE_COUNT];
    let mut skin = skin_bytes(256, &triangles)?;
    let submesh_offset = u32::from_le_bytes(skin[32..36].try_into()?) as usize;
    skin[submesh_offset + 2..submesh_offset + 4].copy_from_slice(&1_u16.to_le_bytes());
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "patch-A.MPQ",
            path: "Creature\\Solarity\\Large.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "patch-A.MPQ",
            path: "Creature\\Solarity\\Large00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature/Solarity/Large.m2")?;
    let decoded = DecodedM2Model::load(&mut store, &path)?;

    assert_eq!(
        decoded.skins()[0].triangle_lookup().len(),
        EXTENDED_TRIANGLE_COUNT
    );
    assert_eq!(decoded.skins()[0].submeshes()[0].level, 1);
    Ok(())
}

/// Serializes a deterministic legacy MD20 fixture with raw influence sentinels.
fn m2_bytes(name: &str, skin_profiles: u32) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut model = M2Model {
        header: M2Header::new(M2Version::WotLK),
        name: Some(name.to_owned()),
        ..M2Model::default()
    };
    model.header.num_skin_profiles = Some(skin_profiles);
    for index in 0..3 {
        model.vertices.push(RawM2Vertex {
            position: C3Vector {
                x: index as f32,
                y: 0.0,
                z: 0.0,
            },
            bone_weights: [0, 0, 0, 0],
            bone_indices: [7, 8, 9, 10],
            normal: C3Vector {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
            tex_coords: C2Vector { x: 0.0, y: 0.0 },
            tex_coords2: Some(C2Vector { x: 1.0, y: 1.0 }),
        });
    }

    let mut cursor = Cursor::new(Vec::new());
    model.write(&mut cursor)?;
    Ok(cursor.into_inner())
}

/// Serializes WotLK's old external SKIN form without using format detection.
fn skin_bytes(bone_count_max: u32, triangles: &[u16]) -> Result<Vec<u8>, Box<dyn Error>> {
    let skin = OldSkin {
        header: OldSkinHeader {
            bone_count_max,
            ..OldSkinHeader::new()
        },
        indices: vec![0, 1, 2],
        triangles: triangles.to_vec(),
        bone_indices: vec![0; 12],
        submeshes: vec![SkinSubmesh {
            id: 0,
            level: 0,
            vertex_start: 0,
            vertex_count: 3,
            triangle_start: 0,
            triangle_count: 3,
            bone_count: 0,
            bone_start: 0,
            bone_influence: 0,
            center: [0.0; 3],
            sort_center: [0.0; 3],
            bounding_radius: 1.0,
        }],
        batches: Vec::new(),
    };
    let mut cursor = Cursor::new(Vec::new());
    skin.write(&mut cursor)?;
    let mut bytes = cursor.into_inner();

    // `wow-m2` currently writes this stock center-bone word as padding. Patch
    // the serialized fixture to ensure the asset facade recovers the real field.
    let submesh_offset = u32::from_le_bytes(bytes[32..36].try_into()?) as usize;
    bytes[submesh_offset + 18..submesh_offset + 20].copy_from_slice(&5_u16.to_le_bytes());
    Ok(bytes)
}
