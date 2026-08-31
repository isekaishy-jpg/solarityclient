//! External stock-compatibility tests for the build-12340 M2 boundary.

use std::error::Error;
use std::io::Cursor;

use solarity_asset::{
    ArchiveCatalog, AssetError, AssetPath, AssetStore, ClientDataRoot, DecodedM2Model, Locale,
    M2BlendMode, M2Interpolation, M2ModelCache, M2SequenceStorage, M2TextureKind,
};
use wow_m2::chunks::attachment::M2Attachment as RawAttachment;
use wow_m2::chunks::material::{
    M2BlendMode as RawBlendMode, M2Material as RawMaterial, M2RenderFlags,
};
use wow_m2::chunks::texture::{M2Texture as RawTexture, M2TextureFlags, M2TextureType};
use wow_m2::chunks::vertex::M2Vertex as RawM2Vertex;
use wow_m2::common::{C2Vector, C3Vector, FixedString, M2Array, M2ArrayString};
use wow_m2::header::{M2Header, M2ModelFlags};
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
    assert_eq!(model.textures().len(), 2);
    assert_eq!(model.textures()[0].kind(), M2TextureKind::Hardcoded);
    assert_eq!(
        model.textures()[0].filename().map(AssetPath::as_str),
        Some("CREATURE\\SOLARITY\\SOLARITY.BLP")
    );
    assert_eq!(model.textures()[1].kind(), M2TextureKind::Monster1);
    assert_eq!(model.textures()[1].filename(), None);
    assert_eq!(model.materials()[0].blend_mode(), M2BlendMode::Alpha);
    assert_eq!(model.texture_lookup(), &[0, 1]);
    assert_eq!(model.texture_units(), &[0, 1]);
    assert_eq!(model.bounds().minimum(), glam::Vec3::new(-1.0, -2.0, -3.0));
    assert_eq!(model.bounds().maximum(), glam::Vec3::new(4.0, 5.0, 6.0));
    assert_eq!(model.bounds().sphere_radius(), 7.25);
    let collision = model
        .collision_mesh()
        .ok_or("fixture collision mesh is absent")?;
    assert_eq!(collision.indices(), &[0, 1, 2]);
    assert_eq!(
        collision.vertices(),
        &[glam::Vec3::ZERO, glam::Vec3::X * 2.0, glam::Vec3::Y * 2.0]
    );
    assert_eq!(
        collision.bounds().minimum(),
        glam::Vec3::new(0.0, 0.0, -0.1)
    );
    assert_eq!(collision.bounds().maximum(), glam::Vec3::new(2.0, 2.0, 0.1));
    let breath = model
        .attachment(17)
        .ok_or("fixture Breath attachment is absent")?;
    assert_eq!(breath.id(), 17);
    assert_eq!(breath.bone_index(), -1);
    assert_eq!(breath.position(), glam::Vec3::new(0.25, 0.5, 1.75));
    assert_eq!(model.attachments(), &[breath]);
    assert_eq!(model.attachment_lookup().len(), 18);
    assert!(model.attachment(16).is_none());
    assert!(model.attachment(18).is_none());
    Ok(())
}

/// Version-264 nested arrays decode per-sequence bone keys rather than outer refs.
#[test]
fn m2_bone_tracks_decode_wotlk_nested_channels() -> Result<(), Box<dyn Error>> {
    let model = animated_m2_bytes()?;
    let skin = skin_bytes(32, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\Animated.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\Animated00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\Animated.m2")?;
    let model = DecodedM2Model::load(&mut store, &path)?;

    let animations = model.animations();
    assert_eq!(animations.sequences().len(), 1);
    let sequence = animations.sequences()[0];
    assert_eq!(sequence.animation_id(), 5);
    assert_eq!(sequence.duration_ms(), 1_000);
    assert_eq!(sequence.storage(), M2SequenceStorage::Internal);
    assert_eq!(animations.is_sequence_available(0), Some(true));
    let bone = &animations.bones()[0];
    assert_eq!(bone.parent(), None);
    assert_eq!(bone.pivot(), glam::Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(bone.translation().interpolation(), M2Interpolation::Linear);
    let channel = &bone.translation().channels()[0];
    assert_eq!(channel.timestamps_ms(), &[0, 1_000]);
    assert_eq!(
        channel.values(),
        &[glam::Vec3::ZERO, glam::Vec3::new(4.0, 5.0, 6.0)]
    );
    Ok(())
}

/// Version-264 material tracks preserve their nested values and fixed16 domain.
#[test]
fn m2_material_tracks_decode_wotlk_nested_channels() -> Result<(), Box<dyn Error>> {
    let model = animated_material_m2_bytes()?;
    let skin = skin_bytes(32, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\MaterialAnimated.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\MaterialAnimated00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\MaterialAnimated.m2")?;
    let model = DecodedM2Model::load(&mut store, &path)?;

    let animations = model.animations();
    let color = animations.colors().first().ok_or("color track is absent")?;
    assert_eq!(color.color().channels()[0].timestamps_ms(), &[0, 1_000]);
    assert_eq!(
        color.color().channels()[0].values(),
        &[
            glam::Vec3::new(1.0, 0.5, 0.25),
            glam::Vec3::new(2.0, 1.0, 0.5)
        ]
    );
    let color_alpha = color.alpha().channels()[0].values();
    assert!((color_alpha[0] - (16_384.0 / 32_767.0)).abs() < f32::EPSILON);
    assert_eq!(color_alpha[1], 1.0);

    let weight = animations
        .texture_weights()
        .first()
        .ok_or("texture weight is absent")?;
    assert_eq!(weight.weight().channels()[0].values()[0], 1.0);
    assert!(
        (weight.weight().channels()[0].values()[1] - (16_384.0 / 32_767.0)).abs() < f32::EPSILON
    );

    let transform = animations
        .texture_transforms()
        .first()
        .ok_or("texture transform is absent")?;
    assert_eq!(
        transform.translation().channels()[0].values(),
        &[glam::Vec3::ZERO, glam::Vec3::new(0.25, 0.5, 0.0)]
    );
    assert_eq!(
        transform.rotation().channels()[0].values(),
        &[glam::Quat::IDENTITY, glam::Quat::IDENTITY]
    );
    assert_eq!(
        transform.scale().channels()[0].values(),
        &[glam::Vec3::ONE, glam::Vec3::new(2.0, 0.5, 1.0)]
    );
    Ok(())
}

/// Stock keeps the model usable while disabling a missing external sequence.
#[test]
fn missing_external_m2_animation_disables_only_its_sequence() -> Result<(), Box<dyn Error>> {
    let mut model = animated_m2_bytes()?;
    let sequence_offset = m2_array_offset(&model, 0x1c)?;
    model[sequence_offset + 12..sequence_offset + 16].copy_from_slice(&0_u32.to_le_bytes());
    let skin = skin_bytes(32, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\External.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\External00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\External.m2")?;
    let model = DecodedM2Model::load(&mut store, &path)?;

    assert_eq!(
        model.animations().sequences()[0].storage(),
        M2SequenceStorage::External
    );
    assert_eq!(model.animations().is_sequence_available(0), Some(false));
    assert!(
        model.animations().bones()[0].translation().channels()[0]
            .timestamps_ms()
            .is_empty()
    );
    Ok(())
}

/// Build-12340's optional shader-combiner table retains its exact `u16` values.
#[test]
fn m2_texture_combiner_table_is_preserved() -> Result<(), Box<dyn Error>> {
    let model = m2_bytes_with_texture_combiners("Combiners", 1, &[0, 4, 7])?;
    let skin = skin_bytes(32, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\Solarity.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\Solarity00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\Solarity.m2")?;
    let model = DecodedM2Model::load(&mut store, &path)?;

    assert_eq!(model.flags() & 0x8, 0x8);
    assert!(model.uses_texture_combiners());
    assert_eq!(model.texture_combiner_combos(), &[0, 4, 7]);
    Ok(())
}

/// Stock cache conversion makes legacy DBC `.mdx` names share the `.m2` entry.
#[test]
fn legacy_model_extensions_use_one_canonical_m2_cache_key() -> Result<(), Box<dyn Error>> {
    let model = m2_bytes("LegacyName", 1)?;
    let skin = skin_bytes(32, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Item\\ObjectComponents\\Weapon\\Legacy.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Item\\ObjectComponents\\Weapon\\Legacy00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let mut cache = M2ModelCache::new();
    let legacy_path = AssetPath::new("Item/ObjectComponents/Weapon/Legacy.mdx")?;
    let modern_path = AssetPath::new("Item/ObjectComponents/Weapon/Legacy.m2")?;

    let legacy_model = cache.load(&mut store, &legacy_path)?;
    let modern_model = cache.load(&mut store, &modern_path)?;

    assert_eq!(cache.len(), 1);
    assert_eq!(legacy_model.path().as_str(), modern_path.as_str());
    assert!(std::sync::Arc::ptr_eq(&legacy_model, &modern_model));
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

/// Unsupported texture replacement tags are not collapsed into an unknown type.
#[test]
fn invalid_m2_texture_type_has_no_compatibility_fallback() -> Result<(), Box<dyn Error>> {
    let mut model = m2_bytes("BadTexture", 1)?;
    let texture_offset = m2_array_offset(&model, 0x50)?;
    model[texture_offset..texture_offset + 4].copy_from_slice(&99_u32.to_le_bytes());
    let skin = skin_bytes(32, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "patch-A.MPQ",
            path: "Creature\\Solarity\\BadTexture.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "patch-A.MPQ",
            path: "Creature\\Solarity\\BadTexture00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature/Solarity/BadTexture.m2")?;

    assert!(matches!(
        DecodedM2Model::load(&mut store, &path),
        Err(AssetError::ModelDecode { path: failed, message })
            if failed == path && message.contains("unsupported replacement type")
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
pub(crate) fn m2_bytes(name: &str, skin_profiles: u32) -> Result<Vec<u8>, Box<dyn Error>> {
    m2_bytes_inner(name, skin_profiles, &[], true)
}

/// Serializes a model carrying WotLK's optional trailing combiner table.
fn m2_bytes_with_texture_combiners(
    name: &str,
    skin_profiles: u32,
    combiners: &[u16],
) -> Result<Vec<u8>, Box<dyn Error>> {
    m2_bytes_inner(name, skin_profiles, combiners, false)
}

/// Serializes a deterministic legacy MD20 fixture with optional combiners.
fn m2_bytes_inner(
    name: &str,
    skin_profiles: u32,
    combiners: &[u16],
    include_camera_metadata: bool,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut model = M2Model {
        header: M2Header::new(M2Version::WotLK),
        name: Some(name.to_owned()),
        ..M2Model::default()
    };
    model.header.num_skin_profiles = Some(skin_profiles);
    if include_camera_metadata {
        model.header.bounding_box_min = [-1.0, -2.0, -3.0];
        model.header.bounding_box_max = [4.0, 5.0, 6.0];
        model.header.bounding_sphere_radius = 7.25;
        let mut breath = RawAttachment::new(17, -1);
        breath.position = C3Vector {
            x: 0.25,
            y: 0.5,
            z: 1.75,
        };
        model.attachments = vec![breath];
        model.raw_data.attachment_lookup_table = vec![u16::MAX; 18];
        model.raw_data.attachment_lookup_table[17] = 0;
        model.header.collision_box_min = [0.0, 0.0, -0.1];
        model.header.collision_box_max = [2.0, 2.0, 0.1];
        model.header.collision_sphere_radius = 2.0_f32.sqrt();
        for index in [0_u16, 1, 2] {
            model
                .raw_data
                .bounding_triangles
                .extend_from_slice(&index.to_le_bytes());
        }
        for vertex in [[0.0_f32, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]] {
            for component in vertex {
                model
                    .raw_data
                    .bounding_vertices
                    .extend_from_slice(&component.to_le_bytes());
            }
        }
    }
    if !combiners.is_empty() {
        model.header.flags |= M2ModelFlags::USE_TEXTURE_COMBINERS;
        model.header.texture_combiner_combos = Some(M2Array::new(0, 0));
    }
    let texture_name = b"Creature\\Solarity\\Solarity.blp";
    model.textures = vec![
        RawTexture {
            texture_type: M2TextureType::Hardcoded,
            flags: M2TextureFlags::WRAP_X,
            filename: M2ArrayString {
                string: FixedString {
                    data: texture_name.to_vec(),
                },
                // The writer recalculates this nonzero sentinel offset.
                array: M2Array::new(u32::try_from(texture_name.len() + 1)?, 1),
            },
        },
        RawTexture {
            texture_type: M2TextureType::Monster1,
            flags: M2TextureFlags::empty(),
            filename: M2ArrayString::default(),
        },
    ];
    model.materials = vec![RawMaterial {
        flags: M2RenderFlags::DEPTH_TEST | M2RenderFlags::DEPTH_WRITE,
        blend_mode: RawBlendMode::ALPHA,
    }];
    model.raw_data.texture_lookup_table = vec![0, 1];
    model.raw_data.texture_units = vec![0, 1];
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
    let mut bytes = cursor.into_inner();

    // Patch the dependency writer's unresolved nested filename reference so
    // this fixture exercises the stock count/offset string layout directly.
    let texture_offset = m2_array_offset(&bytes, 0x50)?;
    let filename_offset = u32::try_from(bytes.len())?;
    bytes[texture_offset + 8..texture_offset + 12]
        .copy_from_slice(&u32::try_from(texture_name.len() + 1)?.to_le_bytes());
    bytes[texture_offset + 12..texture_offset + 16].copy_from_slice(&filename_offset.to_le_bytes());
    bytes.extend_from_slice(texture_name);
    bytes.push(0);
    if !combiners.is_empty() {
        // The dependency writer emits the optional header pair but not its
        // pointed-to table, so complete that exact build-12340 layout here.
        let combiner_offset = u32::try_from(bytes.len())?;
        bytes[0x130..0x134].copy_from_slice(&u32::try_from(combiners.len())?.to_le_bytes());
        bytes[0x134..0x138].copy_from_slice(&combiner_offset.to_le_bytes());
        for combiner in combiners {
            bytes.extend_from_slice(&combiner.to_le_bytes());
        }
    }
    Ok(bytes)
}

/// Adds one internal sequence and one linear bone track to the base fixture.
fn animated_m2_bytes() -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = m2_bytes("Animated", 1)?;

    let sequence_offset = u32::try_from(bytes.len())?;
    let mut sequence = [0_u8; 64];
    sequence[0..2].copy_from_slice(&5_u16.to_le_bytes());
    sequence[4..8].copy_from_slice(&1_000_u32.to_le_bytes());
    sequence[8..12].copy_from_slice(&2.0_f32.to_le_bytes());
    sequence[12..16].copy_from_slice(&0x20_u32.to_le_bytes());
    sequence[16..18].copy_from_slice(&1_i16.to_le_bytes());
    sequence[28..32].copy_from_slice(&100_u32.to_le_bytes());
    sequence[60..62].copy_from_slice(&(-1_i16).to_le_bytes());
    bytes.extend_from_slice(&sequence);

    let bone_offset = u32::try_from(bytes.len())?;
    let mut bone = [0_u8; 88];
    bone[0..4].copy_from_slice(&(-1_i32).to_le_bytes());
    bone[8..10].copy_from_slice(&(-1_i16).to_le_bytes());
    bone[16..18].copy_from_slice(&1_u16.to_le_bytes());
    bone[18..20].copy_from_slice(&u16::MAX.to_le_bytes());
    bone[38..40].copy_from_slice(&u16::MAX.to_le_bytes());
    bone[58..60].copy_from_slice(&u16::MAX.to_le_bytes());
    bone[76..80].copy_from_slice(&1.0_f32.to_le_bytes());
    bone[80..84].copy_from_slice(&2.0_f32.to_le_bytes());
    bone[84..88].copy_from_slice(&3.0_f32.to_le_bytes());
    bytes.extend_from_slice(&bone);

    let timestamp_refs = u32::try_from(bytes.len())?;
    bytes.extend_from_slice(&2_u32.to_le_bytes());
    let timestamp_data_word = bytes.len();
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    let value_refs = u32::try_from(bytes.len())?;
    bytes.extend_from_slice(&2_u32.to_le_bytes());
    let value_data_word = bytes.len();
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    let timestamp_data = u32::try_from(bytes.len())?;
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&1_000_u32.to_le_bytes());
    let value_data = u32::try_from(bytes.len())?;
    for value in [0.0_f32, 0.0, 0.0, 4.0, 5.0, 6.0] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    bytes[timestamp_data_word..timestamp_data_word + 4]
        .copy_from_slice(&timestamp_data.to_le_bytes());
    bytes[value_data_word..value_data_word + 4].copy_from_slice(&value_data.to_le_bytes());
    let bone = usize::try_from(bone_offset)?;
    bytes[bone + 20..bone + 24].copy_from_slice(&1_u32.to_le_bytes());
    bytes[bone + 24..bone + 28].copy_from_slice(&timestamp_refs.to_le_bytes());
    bytes[bone + 28..bone + 32].copy_from_slice(&1_u32.to_le_bytes());
    bytes[bone + 32..bone + 36].copy_from_slice(&value_refs.to_le_bytes());
    bytes[0x1c..0x20].copy_from_slice(&1_u32.to_le_bytes());
    bytes[0x20..0x24].copy_from_slice(&sequence_offset.to_le_bytes());
    bytes[0x2c..0x30].copy_from_slice(&1_u32.to_le_bytes());
    bytes[0x30..0x34].copy_from_slice(&bone_offset.to_le_bytes());
    Ok(bytes)
}

/// Adds color, texture-weight, and texture-transform tracks to one sequence.
fn animated_material_m2_bytes() -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = animated_m2_bytes()?;

    let color_offset = bytes.len();
    bytes.resize(color_offset + 40, 0);
    let weight_offset = bytes.len();
    bytes.resize(weight_offset + 20, 0);
    let transform_offset = bytes.len();
    bytes.resize(transform_offset + 60, 0);

    append_linear_track(
        &mut bytes,
        color_offset,
        &[0, 1_000],
        &f32_values(&[1.0, 0.5, 0.25, 2.0, 1.0, 0.5]),
        12,
    )?;
    append_linear_track(
        &mut bytes,
        color_offset + 20,
        &[0, 1_000],
        &i16_values(&[16_384, 32_767]),
        2,
    )?;
    append_linear_track(
        &mut bytes,
        weight_offset,
        &[0, 1_000],
        &i16_values(&[32_767, 16_384]),
        2,
    )?;
    append_linear_track(
        &mut bytes,
        transform_offset,
        &[0, 1_000],
        &f32_values(&[0.0, 0.0, 0.0, 0.25, 0.5, 0.0]),
        12,
    )?;
    append_linear_track(
        &mut bytes,
        transform_offset + 20,
        &[0, 1_000],
        &i16_values(&[-32_768, -32_768, -32_768, -1, -32_768, -32_768, -32_768, -1]),
        8,
    )?;
    append_linear_track(
        &mut bytes,
        transform_offset + 40,
        &[0, 1_000],
        &f32_values(&[1.0, 1.0, 1.0, 2.0, 0.5, 1.0]),
        12,
    )?;

    set_header_array(&mut bytes, 0x48, 1, color_offset)?;
    set_header_array(&mut bytes, 0x58, 1, weight_offset)?;
    set_header_array(&mut bytes, 0x60, 1, transform_offset)?;
    Ok(bytes)
}

/// Appends one sequence channel and patches its 20-byte nested track header.
fn append_linear_track(
    bytes: &mut Vec<u8>,
    track_offset: usize,
    timestamps: &[u32],
    values: &[u8],
    value_stride: usize,
) -> Result<(), Box<dyn Error>> {
    if values.len() != timestamps.len() * value_stride {
        return Err("fixture track value count differs from timestamps".into());
    }
    let timestamp_refs = bytes.len();
    bytes.extend_from_slice(&u32::try_from(timestamps.len())?.to_le_bytes());
    let timestamp_data_word = bytes.len();
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    let value_refs = bytes.len();
    bytes.extend_from_slice(&u32::try_from(timestamps.len())?.to_le_bytes());
    let value_data_word = bytes.len();
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    let timestamp_data = bytes.len();
    for timestamp in timestamps {
        bytes.extend_from_slice(&timestamp.to_le_bytes());
    }
    let value_data = bytes.len();
    bytes.extend_from_slice(values);

    bytes[timestamp_data_word..timestamp_data_word + 4]
        .copy_from_slice(&u32::try_from(timestamp_data)?.to_le_bytes());
    bytes[value_data_word..value_data_word + 4]
        .copy_from_slice(&u32::try_from(value_data)?.to_le_bytes());
    bytes[track_offset..track_offset + 2].copy_from_slice(&1_u16.to_le_bytes());
    bytes[track_offset + 2..track_offset + 4].copy_from_slice(&(-1_i16).to_le_bytes());
    bytes[track_offset + 4..track_offset + 8].copy_from_slice(&1_u32.to_le_bytes());
    bytes[track_offset + 8..track_offset + 12]
        .copy_from_slice(&u32::try_from(timestamp_refs)?.to_le_bytes());
    bytes[track_offset + 12..track_offset + 16].copy_from_slice(&1_u32.to_le_bytes());
    bytes[track_offset + 16..track_offset + 20]
        .copy_from_slice(&u32::try_from(value_refs)?.to_le_bytes());
    Ok(())
}

fn set_header_array(
    bytes: &mut [u8],
    pair_offset: usize,
    count: u32,
    offset: usize,
) -> Result<(), Box<dyn Error>> {
    bytes[pair_offset..pair_offset + 4].copy_from_slice(&count.to_le_bytes());
    bytes[pair_offset + 4..pair_offset + 8].copy_from_slice(&u32::try_from(offset)?.to_le_bytes());
    Ok(())
}

fn f32_values(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn i16_values(values: &[i16]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

/// Reads the offset word of one M2 header array used by fixture mutation.
fn m2_array_offset(bytes: &[u8], pair_offset: usize) -> Result<usize, Box<dyn Error>> {
    Ok(u32::from_le_bytes(bytes[pair_offset + 4..pair_offset + 8].try_into()?) as usize)
}

/// Serializes WotLK's old external SKIN form without using format detection.
pub(crate) fn skin_bytes(
    bone_count_max: u32,
    triangles: &[u16],
) -> Result<Vec<u8>, Box<dyn Error>> {
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
