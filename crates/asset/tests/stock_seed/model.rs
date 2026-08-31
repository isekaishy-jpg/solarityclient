//! External stock-compatibility tests for the build-12340 M2 boundary.

use std::error::Error;
use std::io::Cursor;

use solarity_asset::{
    ArchiveCatalog, AssetError, AssetPath, AssetStore, ClientDataRoot, DecodedM2Model, Locale,
    M2BlendMode, M2Interpolation, M2ModelCache, M2SequenceStorage, M2TextureKind,
};
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
    assert_eq!(model.texture_coordinate_lookup(), &[0, -1]);
    assert_eq!(model.replaceable_texture_lookup().len(), 12);
    assert_eq!(model.replaceable_texture_lookup()[11], 1);
    assert_eq!(model.bounds().minimum(), glam::Vec3::new(-1.0, -2.0, -3.0));
    assert_eq!(model.bounds().maximum(), glam::Vec3::new(4.0, 5.0, 6.0));
    assert_eq!(model.bounds().sphere_radius(), 7.25);
    let collision = model
        .collision_mesh()
        .ok_or("fixture collision mesh is absent")?;
    assert_eq!(collision.indices(), &[0, 1, 2]);
    assert_eq!(collision.face_normals(), &[glam::Vec3::Z]);
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
    assert_eq!(breath.bone_index(), 0);
    assert_eq!(breath.unknown(), 0x1234);
    assert_eq!(breath.position(), glam::Vec3::new(0.25, 0.5, 1.75));
    assert!(breath.enabled().channels().is_empty());
    assert_eq!(model.attachments().len(), 1);
    assert_eq!(model.attachments().first(), Some(breath));
    assert_eq!(model.animations().key_bone_lookup(), &[None, Some(0), None]);
    assert_eq!(
        model.animations().key_bone(1),
        model.animations().bones().first()
    );
    assert!(model.animations().key_bone(0).is_none());
    assert!(model.animations().key_bone(3).is_none());
    assert_eq!(model.attachment_lookup().len(), 18);
    assert!(model.attachment(16).is_none());
    assert!(model.attachment(18).is_none());
    Ok(())
}

/// Attachment lookup holes stay absent and non-hole missing records are rejected.
#[test]
fn m2_attachment_lookup_rejects_a_missing_attachment() -> Result<(), Box<dyn Error>> {
    let mut model = m2_bytes("BadAttachment", 1)?;
    let lookup_offset = m2_array_offset(&model, 0xf8)?;
    model[lookup_offset + 34..lookup_offset + 36].copy_from_slice(&1_u16.to_le_bytes());
    let skin = skin_bytes(32, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\BadAttachment.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\BadAttachment00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\BadAttachment.m2")?;

    assert!(matches!(
        DecodedM2Model::load(&mut store, &path),
        Err(AssetError::ModelDecode { path: failed, message })
            if failed == path
                && message.contains("attachment lookup 17 references missing attachment 1")
    ));
    Ok(())
}

/// Values below the sole `-1` key-bone sentinel have no compatibility meaning.
#[test]
fn m2_key_bone_lookup_rejects_an_invalid_signed_value() -> Result<(), Box<dyn Error>> {
    let mut model = m2_bytes("BadKeyBone", 1)?;
    let lookup_offset = m2_array_offset(&model, 0x34)?;
    model[lookup_offset + 2..lookup_offset + 4].copy_from_slice(&(-2_i16).to_le_bytes());
    let skin = skin_bytes(32, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\BadKeyBone.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\BadKeyBone00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\BadKeyBone.m2")?;

    assert!(matches!(
        DecodedM2Model::load(&mut store, &path),
        Err(AssetError::ModelDecode { path: failed, message })
            if failed == path
                && message.contains("key-bone lookup 1 references missing bone -2")
    ));
    Ok(())
}

/// Replacement roles may not point at a texture declaration that is absent.
#[test]
fn m2_replaceable_texture_lookup_rejects_a_missing_texture() -> Result<(), Box<dyn Error>> {
    let mut model = m2_bytes("BadReplacement", 1)?;
    let lookup_offset = m2_array_offset(&model, 0x68)?;
    model[lookup_offset + 22..lookup_offset + 24].copy_from_slice(&2_u16.to_le_bytes());
    let skin = skin_bytes(32, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\BadReplacement.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\BadReplacement00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\BadReplacement.m2")?;

    assert!(matches!(
        DecodedM2Model::load(&mut store, &path),
        Err(AssetError::ModelDecode { path: failed, message })
            if failed == path
                && message.contains(
                    "replaceable-texture lookup 11 references missing entry 2"
                )
    ));
    Ok(())
}

/// Dedicated collision retains exactly one authored normal per triangle.
#[test]
fn m2_collision_rejects_a_missing_face_normal() -> Result<(), Box<dyn Error>> {
    let mut model = m2_bytes("BadCollisionNormal", 1)?;
    model[0xe8..0xf0].fill(0);
    let skin = skin_bytes(32, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\BadCollisionNormal.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\BadCollisionNormal00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\BadCollisionNormal.m2")?;

    assert!(matches!(
        DecodedM2Model::load(&mut store, &path),
        Err(AssetError::ModelDecode { path: failed, message })
            if failed == path
                && message.contains(
                    "collision must contain vertices and one face normal per triangle"
                )
    ));
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
    assert_eq!(
        animations.animation_lookup(),
        &[u16::MAX, u16::MAX, u16::MAX, u16::MAX, u16::MAX, 0]
    );
    assert_eq!(animations.select_sequence(5, Some(0), 0), Some(0));
    assert_eq!(animations.sequence_for_variation(5, 0), Some(0));
    assert_eq!(animations.sequence_for_variation(5, 1), None);
    assert_eq!(animations.select_sequence(4, None, 0), None);
    assert_eq!(animations.available_variation_count(5), Some(1));
    assert_eq!(sequence.replay_range(), (0, 0));
    assert_eq!(sequence.cycle_count(0), 1);
    assert_eq!(sequence.cycle_count(0x7fff), 1);
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
    let breath = model
        .attachment(17)
        .ok_or("fixture Breath attachment is absent")?;
    assert_eq!(breath.enabled().interpolation(), M2Interpolation::Linear);
    assert_eq!(breath.enabled().channels()[0].timestamps_ms(), &[0, 1_000]);
    assert_eq!(breath.enabled().channels()[0].values(), &[1, 0]);
    Ok(())
}

/// Stock scans sequence records only when the lookup table is entirely absent.
#[test]
fn m2_animation_selection_scans_when_lookup_table_is_absent() -> Result<(), Box<dyn Error>> {
    let mut model = animated_m2_bytes()?;
    model[0x24..0x2c].fill(0);
    let skin = skin_bytes(32, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\NoLookup.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\NoLookup00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\NoLookup.m2")?;
    let model = DecodedM2Model::load(&mut store, &path)?;

    assert_eq!(model.animations().sequences()[0].animation_id(), 5);
    assert_eq!(model.animations().select_sequence(5, None, 0), Some(0));
    Ok(())
}

/// A present lookup miss is authoritative and does not enter the absent-table scan.
#[test]
fn m2_animation_selection_preserves_present_lookup_miss() -> Result<(), Box<dyn Error>> {
    let mut model = animated_m2_bytes()?;
    let lookup_offset = m2_array_offset(&model, 0x24)?;
    model[lookup_offset + 10..lookup_offset + 12].copy_from_slice(&u16::MAX.to_le_bytes());
    let skin = skin_bytes(32, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\LookupMiss.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\LookupMiss00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\LookupMiss.m2")?;
    let model = DecodedM2Model::load(&mut store, &path)?;

    assert_eq!(model.animations().sequences()[0].animation_id(), 5);
    assert_eq!(model.animations().select_sequence(5, None, 0), None);
    Ok(())
}

/// Replay fields scale the CRT roll over an exclusive upper bound.
#[test]
fn m2_sequence_cycle_count_matches_stock_integer_scaling() -> Result<(), Box<dyn Error>> {
    let mut model = animated_m2_bytes()?;
    let sequence_offset = m2_array_offset(&model, 0x1c)?;
    model[sequence_offset + 20..sequence_offset + 24].copy_from_slice(&2_u32.to_le_bytes());
    model[sequence_offset + 24..sequence_offset + 28].copy_from_slice(&6_u32.to_le_bytes());
    let skin = skin_bytes(32, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\Replay.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\Replay00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\Replay.m2")?;
    let model = DecodedM2Model::load(&mut store, &path)?;
    let sequence = model.animations().sequences()[0];

    assert_eq!(sequence.cycle_count(0), 2);
    assert_eq!(sequence.cycle_count(8_192), 3);
    assert_eq!(sequence.cycle_count(16_384), 4);
    assert_eq!(sequence.cycle_count(0x7fff), 5);
    Ok(())
}

/// A variation chain cannot cycle even when every referenced record exists.
#[test]
fn m2_animation_variation_cycle_is_rejected() -> Result<(), Box<dyn Error>> {
    let mut model = animated_m2_bytes()?;
    let sequence_offset = m2_array_offset(&model, 0x1c)?;
    model[sequence_offset + 60..sequence_offset + 62].copy_from_slice(&0_i16.to_le_bytes());
    let skin = skin_bytes(32, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\Cycle.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\Cycle00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\Cycle.m2")?;

    assert!(matches!(
        DecodedM2Model::load(&mut store, &path),
        Err(AssetError::ModelDecode { path: failed, message })
            if failed == path && message.contains("variation chain cycles")
    ));
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

/// Version-264 ribbons retain exact static fields and all six nested tracks.
#[test]
fn m2_ribbon_emitters_decode_wotlk_record_and_channels() -> Result<(), Box<dyn Error>> {
    let model = animated_ribbon_m2_bytes()?;
    let skin = skin_bytes(32, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\Ribbon.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\Ribbon00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\Ribbon.m2")?;
    let model = DecodedM2Model::load(&mut store, &path)?;
    let ribbon = model
        .animations()
        .ribbons()
        .first()
        .ok_or("ribbon is absent")?;

    assert_eq!(ribbon.id(), 0x5249_424E);
    assert_eq!(ribbon.bone_index(), Some(0));
    assert_eq!(ribbon.position(), glam::Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(ribbon.texture_indices(), &[0]);
    assert_eq!(ribbon.material_indices(), &[0]);
    assert_eq!(ribbon.color().channels()[0].timestamps_ms(), &[0, 1_000]);
    assert_eq!(
        ribbon.color().channels()[0].values(),
        &[
            glam::Vec3::new(1.0, 0.5, 0.25),
            glam::Vec3::new(0.0, 1.0, 0.5)
        ]
    );
    assert_eq!(ribbon.alpha().channels()[0].values()[0], 1.0);
    assert_eq!(ribbon.height_above().channels()[0].values(), &[1.0, 2.0]);
    assert_eq!(ribbon.height_below().channels()[0].values(), &[0.5, 1.5]);
    assert_eq!(ribbon.edges_per_second(), 20.0);
    assert_eq!(ribbon.edge_lifetime_seconds(), 1.25);
    assert_eq!(ribbon.gravity(), 9.8);
    assert_eq!((ribbon.texture_rows(), ribbon.texture_columns()), (2, 4));
    assert_eq!(ribbon.texture_slot().channels()[0].values(), &[0, 3]);
    assert_eq!(ribbon.visibility().channels()[0].values(), &[1, 0]);
    assert_eq!(ribbon.priority_plane(), -3);
    assert_eq!(ribbon.color_index(), -1);
    assert_eq!(ribbon.texture_transform_lookup_index(), -1);
    Ok(())
}

/// Stock indexes ribbon material and texture pass arrays in lockstep.
#[test]
fn m2_ribbon_pass_arrays_must_have_equal_lengths() -> Result<(), Box<dyn Error>> {
    let mut model = animated_ribbon_m2_bytes()?;
    let ribbon_offset = m2_array_offset(&model, 0x120)?;
    model[ribbon_offset + 28..ribbon_offset + 32].copy_from_slice(&0_u32.to_le_bytes());
    let skin = skin_bytes(32, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\BadRibbonPasses.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\BadRibbonPasses00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\BadRibbonPasses.m2")?;

    assert!(matches!(
        DecodedM2Model::load(&mut store, &path),
        Err(AssetError::ModelDecode { path: failed, message })
            if failed == path && message.contains("1 textures for 0 material passes")
    ));
    Ok(())
}

/// Version-264 particles retain the entire 476-byte record and both track domains.
#[test]
fn m2_particle_emitters_decode_wotlk_record_and_channels() -> Result<(), Box<dyn Error>> {
    let model = animated_particle_m2_bytes()?;
    let skin = skin_bytes(32, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\Particle.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\Particle00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\Particle.m2")?;
    let model = DecodedM2Model::load(&mut store, &path)?;
    let particle = model
        .animations()
        .particles()
        .first()
        .ok_or("particle is absent")?;

    assert_eq!(particle.id(), 0x5041_5254);
    assert_eq!(particle.flags(), 0x9000_8042);
    assert!(!particle.particles_in_model_space());
    assert_eq!(particle.position(), glam::Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(particle.bone_index(), Some(0));
    assert_eq!(particle.texture_id(), Some(0));
    assert!(particle.uses_multiple_textures());
    assert_eq!(particle.texture_indices(), [Some(0), Some(0), Some(0)]);
    assert_eq!(
        particle.geometry_model_path().map(AssetPath::as_str),
        Some("SPELLS\\PARTICLEGEOMETRY.M2")
    );
    assert_eq!(
        particle.child_emitter_model_path().map(AssetPath::as_str),
        Some("SPELLS\\CHILDEMITTER.M2")
    );
    assert_eq!((particle.blending_type(), particle.emitter_type()), (2, 3));
    assert_eq!(particle.particle_color_index(), 12);
    assert_eq!((particle.particle_type(), particle.head_or_tail()), (1, 2));
    assert_eq!(particle.priority_plane(), -4);
    assert_eq!(
        (particle.texture_rows(), particle.texture_columns()),
        (4, 8)
    );

    assert_eq!(
        particle.emission_speed().channels()[0].values(),
        &[2.0, 4.0]
    );
    assert_eq!(
        particle.speed_variation().channels()[0].values(),
        &[0.1, 0.2]
    );
    assert_eq!(
        particle.vertical_range().channels()[0].values(),
        &[0.3, 0.4]
    );
    assert_eq!(
        particle.horizontal_range().channels()[0].values(),
        &[0.5, 0.6]
    );
    assert_eq!(particle.gravity().channels()[0].values(), &[9.0, 8.0]);
    assert_eq!(particle.lifespan().channels()[0].values(), &[1.0, 2.0]);
    assert_eq!(particle.lifespan_variation(), 0.25);
    assert_eq!(
        particle.emission_rate().channels()[0].values(),
        &[10.0, 20.0]
    );
    assert_eq!(particle.emission_rate_variation(), 0.5);
    assert_eq!(
        particle.emission_area_width().channels()[0].values(),
        &[3.0, 4.0]
    );
    assert_eq!(
        particle.emission_area_length().channels()[0].values(),
        &[5.0, 6.0]
    );
    assert_eq!(particle.z_source().channels()[0].values(), &[7.0, 8.0]);

    assert_eq!(particle.color().timestamps(), &[0, i16::MAX as u16]);
    assert_eq!(
        particle.color().values(),
        &[
            glam::Vec3::new(1.0, 0.5, 0.25),
            glam::Vec3::new(0.0, 1.0, 0.5)
        ]
    );
    assert_eq!(particle.alpha().values(), &[1.0, 16_384.0 / 32_767.0]);
    assert_eq!(
        particle.scale().values(),
        &[glam::Vec2::new(1.0, 2.0), glam::Vec2::new(3.0, 4.0)]
    );
    assert_eq!(particle.scale_variation(), glam::Vec2::new(0.25, 0.5));
    assert_eq!(particle.head_uv_animation().values(), &[1, 2]);
    assert_eq!(particle.tail_uv_animation().values(), &[3, 4]);

    assert_eq!(particle.tail_length(), 1.5);
    assert_eq!(particle.twinkle_speed(), 2.5);
    assert_eq!(particle.twinkle_percent(), 0.75);
    assert_eq!(particle.twinkle_scale(), glam::Vec2::new(0.5, 1.5));
    assert_eq!(particle.inherit_velocity_scale(), 0.6);
    assert_eq!(particle.drag(), 0.7);
    assert_eq!(particle.base_spin(), 0.8);
    assert_eq!(particle.base_spin_variation(), 0.9);
    assert_eq!(particle.spin_speed(), 1.1);
    assert_eq!(particle.spin_speed_variation(), 1.2);
    assert_eq!(
        particle.tumble(),
        (
            glam::Vec3::new(1.0, 2.0, 3.0),
            glam::Vec3::new(4.0, 5.0, 6.0)
        )
    );
    assert_eq!(particle.wind_vector(), glam::Vec3::new(7.0, 8.0, 9.0));
    assert_eq!(particle.wind_time(), 1.3);
    assert_eq!(particle.follow_speed(), (1.4, 1.6));
    assert_eq!(particle.follow_scale(), (1.5, 1.7));
    assert_eq!(
        particle.spline_points(),
        &[
            glam::Vec3::new(1.0, 3.0, 5.0),
            glam::Vec3::new(2.0, 4.0, 6.0)
        ]
    );
    assert_eq!(particle.enabled().channels()[0].values(), &[1, 0]);
    Ok(())
}

/// Lifetime ramps must be ordered for the stock interval search.
#[test]
fn m2_particle_lifetime_timestamps_must_be_ordered() -> Result<(), Box<dyn Error>> {
    let mut model = animated_particle_m2_bytes()?;
    let particle_offset = m2_array_offset(&model, 0x128)?;
    let timestamps = m2_array_offset(&model, particle_offset + 0x104)?;
    model[timestamps..timestamps + 2].copy_from_slice(&(i16::MAX as u16).to_le_bytes());
    model[timestamps + 2..timestamps + 4].copy_from_slice(&0_u16.to_le_bytes());
    let skin = skin_bytes(32, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\BadParticleLifetime.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\BadParticleLifetime00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\BadParticleLifetime.m2")?;

    assert!(matches!(
        DecodedM2Model::load(&mut store, &path),
        Err(AssetError::ModelDecode { path: failed, message })
            if failed == path && message.contains("color timestamps are not ordered")
    ));
    Ok(())
}

/// Lifetime ramp keys are signed fixed16 values in build 12340.
#[test]
fn m2_particle_lifetime_timestamps_must_fit_signed_fixed16() -> Result<(), Box<dyn Error>> {
    let mut model = animated_particle_m2_bytes()?;
    let particle_offset = m2_array_offset(&model, 0x128)?;
    let timestamps = m2_array_offset(&model, particle_offset + 0x104)?;
    model[timestamps + 2..timestamps + 4].copy_from_slice(&0x8000_u16.to_le_bytes());
    let skin = skin_bytes(32, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\BadParticleLifetime.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\BadParticleLifetime00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\BadParticleLifetime.m2")?;

    assert!(matches!(
        DecodedM2Model::load(&mut store, &path),
        Err(AssetError::ModelDecode { path: failed, message })
            if failed == path && message.contains("color timestamp exceeds the stock signed fixed16 domain")
    ));
    Ok(())
}

/// Packed multi-texture slots are each validated against the model texture table.
#[test]
fn m2_particle_multi_texture_references_do_not_receive_a_fallback() -> Result<(), Box<dyn Error>> {
    let mut model = animated_particle_m2_bytes()?;
    let particle_offset = m2_array_offset(&model, 0x128)?;
    model[particle_offset + 0x16..particle_offset + 0x18].copy_from_slice(&2_u16.to_le_bytes());
    let skin = skin_bytes(32, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\BadParticle.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\BadParticle00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\BadParticle.m2")?;

    let result = DecodedM2Model::load(&mut store, &path);
    assert!(
        matches!(
        &result,
        Err(AssetError::ModelDecode { path: failed, message })
            if failed == &path && message.contains("particle 0 references missing texture 2")
        ),
        "unexpected particle validation result: {result:?}"
    );
    Ok(())
}

/// Version-264 model lights use the exact 156-byte seven-track record.
#[test]
fn m2_lights_decode_wotlk_record_and_channels() -> Result<(), Box<dyn Error>> {
    let model = animated_light_m2_bytes()?;
    let skin = skin_bytes(32, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\Light.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\Light00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\Light.m2")?;
    let model = DecodedM2Model::load(&mut store, &path)?;
    let light = model
        .animations()
        .lights()
        .first()
        .ok_or("light is absent")?;

    assert_eq!(light.kind(), solarity_asset::M2LightKind::Point);
    assert_eq!(light.bone_index(), Some(0));
    assert_eq!(light.position(), glam::Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(
        light.ambient_color().channels()[0].values(),
        &[
            glam::Vec3::new(0.1, 0.2, 0.3),
            glam::Vec3::new(0.4, 0.5, 0.6)
        ]
    );
    assert_eq!(
        light.ambient_intensity().channels()[0].values(),
        &[0.5, 1.0]
    );
    assert_eq!(
        light.diffuse_color().channels()[0].values(),
        &[
            glam::Vec3::new(0.7, 0.8, 0.9),
            glam::Vec3::new(1.0, 0.9, 0.8)
        ]
    );
    assert_eq!(
        light.diffuse_intensity().channels()[0].values(),
        &[1.0, 2.0]
    );
    assert_eq!(
        light.attenuation_start().channels()[0].values(),
        &[3.0, 4.0]
    );
    assert_eq!(light.attenuation_end().channels()[0].values(), &[5.0, 6.0]);
    assert_eq!(light.visibility().channels()[0].values(), &[1, 0]);
    Ok(())
}

/// Later light selectors are rejected instead of being coerced to point lights.
#[test]
fn m2_light_type_has_no_later_version_fallback() -> Result<(), Box<dyn Error>> {
    let mut model = animated_light_m2_bytes()?;
    let light_offset = m2_array_offset(&model, 0x108)?;
    model[light_offset..light_offset + 2].copy_from_slice(&2_u16.to_le_bytes());
    let skin = skin_bytes(32, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\BadLight.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\BadLight00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\BadLight.m2")?;

    assert!(matches!(
        DecodedM2Model::load(&mut store, &path),
        Err(AssetError::ModelDecode { path: failed, message })
            if failed == path && message.contains("light 0 has unsupported type 2")
    ));
    Ok(())
}

/// Version-264 cameras retain exact base values, tracks, and signed lookup slots.
#[test]
fn m2_cameras_decode_wotlk_record_tracks_and_lookup() -> Result<(), Box<dyn Error>> {
    let model = animated_camera_m2_bytes()?;
    let skin = skin_bytes(32, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\Camera.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\Camera00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\Camera.m2")?;
    let model = DecodedM2Model::load(&mut store, &path)?;
    let animations = model.animations();
    let camera = animations.cameras().first().ok_or("camera is absent")?;

    assert_eq!(camera.kind(), -1);
    assert_eq!(camera.field_of_view_radians(), 1.0);
    assert_eq!((camera.near_clip(), camera.far_clip()), (0.25, 500.0));
    assert_eq!(
        camera.position().channels()[0].values(),
        &[
            glam::Vec3::new(0.0, 1.0, 2.0),
            glam::Vec3::new(3.0, 4.0, 5.0)
        ]
    );
    assert_eq!(camera.position_base(), glam::Vec3::new(6.0, 7.0, 8.0));
    assert_eq!(
        camera.target_position().channels()[0].values(),
        &[
            glam::Vec3::new(9.0, 10.0, 11.0),
            glam::Vec3::new(12.0, 13.0, 14.0)
        ]
    );
    assert_eq!(
        camera.target_position_base(),
        glam::Vec3::new(15.0, 16.0, 17.0)
    );
    assert_eq!(camera.roll_radians().channels()[0].values(), &[0.0, 0.5]);
    assert_eq!(animations.camera_lookup(), &[None, Some(0)]);
    Ok(())
}

/// Camera lookup entries never fall back to the first authored camera.
#[test]
fn m2_camera_lookup_rejects_a_missing_camera() -> Result<(), Box<dyn Error>> {
    let mut model = animated_camera_m2_bytes()?;
    let lookup_offset = m2_array_offset(&model, 0x118)?;
    model[lookup_offset + 2..lookup_offset + 4].copy_from_slice(&1_i16.to_le_bytes());
    let skin = skin_bytes(32, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\BadCamera.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\BadCamera00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\BadCamera.m2")?;

    assert!(matches!(
        DecodedM2Model::load(&mut store, &path),
        Err(AssetError::ModelDecode { path: failed, message })
            if failed == path && message.contains("camera lookup 1 references missing camera 1")
    ));
    Ok(())
}

/// Version-264 events retain four-byte IDs and timestamp-only nested channels.
#[test]
fn m2_events_decode_wotlk_record_and_timeline() -> Result<(), Box<dyn Error>> {
    let model = animated_event_m2_bytes()?;
    let skin = skin_bytes(32, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\Event.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\Event00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\Event.m2")?;
    let model = DecodedM2Model::load(&mut store, &path)?;
    let event = model
        .animations()
        .events()
        .first()
        .ok_or("event is absent")?;

    assert_eq!(event.identifier(), *b"$SND");
    assert_eq!(event.data(), 42);
    assert_eq!(event.bone_index(), Some(0));
    assert_eq!(event.position(), glam::Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(
        event.timeline().interpolation(),
        solarity_asset::M2Interpolation::Step
    );
    assert_eq!(event.timeline().global_sequence(), None);
    assert_eq!(event.timeline().channels(), &[vec![125, 750]]);
    Ok(())
}

/// Event bone references do not fall back to the model root.
#[test]
fn m2_event_rejects_a_missing_bone() -> Result<(), Box<dyn Error>> {
    let mut model = animated_event_m2_bytes()?;
    let event_offset = m2_array_offset(&model, 0x100)?;
    model[event_offset + 8..event_offset + 12].copy_from_slice(&1_u32.to_le_bytes());
    let skin = skin_bytes(32, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\BadEvent.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "Creature\\Solarity\\BadEvent00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\BadEvent.m2")?;

    assert!(matches!(
        DecodedM2Model::load(&mut store, &path),
        Err(AssetError::ModelDecode { path: failed, message })
            if failed == path && message.contains("event 0 references missing bone 1")
    ));
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

/// Stock replacement slots may encode one NUL byte instead of a zero array.
#[test]
fn empty_m2_replacement_filename_decodes_as_absent() -> Result<(), Box<dyn Error>> {
    let mut model = m2_bytes("EmptyReplacement", 1)?;
    let texture_offset = m2_array_offset(&model, 0x50)?;
    let replacement_offset = texture_offset + 16;
    let empty_name_offset = u32::try_from(model.len())?;
    model[replacement_offset + 8..replacement_offset + 12].copy_from_slice(&1_u32.to_le_bytes());
    model[replacement_offset + 12..replacement_offset + 16]
        .copy_from_slice(&empty_name_offset.to_le_bytes());
    model.push(0);
    let skin = skin_bytes(32, &[0, 1, 2])?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "patch-A.MPQ",
            path: "Character\\Solarity\\EmptyReplacement.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "patch-A.MPQ",
            path: "Character\\Solarity\\EmptyReplacement00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Character/Solarity/EmptyReplacement.m2")?;

    let decoded = DecodedM2Model::load(&mut store, &path)?;

    assert_eq!(decoded.textures()[1].kind(), M2TextureKind::Monster1);
    assert_eq!(decoded.textures()[1].filename(), None);
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

/// Invalid authored bounds are rejected before they reach culling or GPU work.
#[test]
fn skin_submesh_nonfinite_bounds_are_rejected() -> Result<(), Box<dyn Error>> {
    let model = m2_bytes("BadBounds", 1)?;
    let mut skin = skin_bytes(32, &[0, 1, 2])?;
    let submesh_offset = u32::from_le_bytes(skin[32..36].try_into()?) as usize;
    skin[submesh_offset + 44..submesh_offset + 48].copy_from_slice(&f32::NAN.to_le_bytes());
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "patch-A.MPQ",
            path: "Creature\\Solarity\\BadBounds.m2",
            bytes: &model,
        },
        FixtureFile {
            archive: "patch-A.MPQ",
            path: "Creature\\Solarity\\BadBounds00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let model_path = AssetPath::new("Creature/Solarity/BadBounds.m2")?;

    assert!(matches!(
        DecodedM2Model::load(&mut store, &model_path),
        Err(AssetError::ModelDecode { path, message })
            if path.as_str() == "CREATURE\\SOLARITY\\BADBOUNDS00.SKIN"
                && message.contains("submesh 0 has invalid bounds")
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
        for component in [0.0_f32, 0.0, 1.0] {
            model
                .raw_data
                .bounding_normals
                .extend_from_slice(&component.to_le_bytes());
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
    model.raw_data.texture_units = vec![0, u16::MAX];
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
    if include_camera_metadata {
        append_build_12340_attachment_metadata(&mut bytes)?;
    }
    Ok(bytes)
}

/// Appends one exact stock bone, 40-byte attachment, and semantic lookup.
fn append_build_12340_attachment_metadata(bytes: &mut Vec<u8>) -> Result<(), Box<dyn Error>> {
    let bone_offset = bytes.len();
    let mut bone = [0_u8; 88];
    bone[0..4].copy_from_slice(&(-1_i32).to_le_bytes());
    bone[8..10].copy_from_slice(&(-1_i16).to_le_bytes());
    bone[18..20].copy_from_slice(&(-1_i16).to_le_bytes());
    bone[38..40].copy_from_slice(&(-1_i16).to_le_bytes());
    bone[58..60].copy_from_slice(&(-1_i16).to_le_bytes());
    bytes.extend_from_slice(&bone);
    set_header_array(bytes, 0x2c, 1, bone_offset)?;

    let key_bone_lookup_offset = bytes.len();
    for value in [-1_i16, 0, -1] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    set_header_array(bytes, 0x34, 3, key_bone_lookup_offset)?;

    let replaceable_texture_lookup_offset = bytes.len();
    for slot in 0..12 {
        let value = if slot == 11 { 1 } else { u16::MAX };
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    set_header_array(bytes, 0x68, 12, replaceable_texture_lookup_offset)?;

    let attachment_offset = bytes.len();
    let mut attachment = [0_u8; 40];
    attachment[0..4].copy_from_slice(&17_u32.to_le_bytes());
    attachment[4..6].copy_from_slice(&0_u16.to_le_bytes());
    attachment[6..8].copy_from_slice(&0x1234_u16.to_le_bytes());
    attachment[8..12].copy_from_slice(&0.25_f32.to_le_bytes());
    attachment[12..16].copy_from_slice(&0.5_f32.to_le_bytes());
    attachment[16..20].copy_from_slice(&1.75_f32.to_le_bytes());
    attachment[22..24].copy_from_slice(&(-1_i16).to_le_bytes());
    bytes.extend_from_slice(&attachment);
    set_header_array(bytes, 0xf0, 1, attachment_offset)?;

    let lookup_offset = bytes.len();
    for slot in 0..18 {
        let value = if slot == 17 { 0 } else { u16::MAX };
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    set_header_array(bytes, 0xf8, 18, lookup_offset)?;
    Ok(())
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
    let animation_lookup = u32::try_from(bytes.len())?;
    for sequence in [u16::MAX, u16::MAX, u16::MAX, u16::MAX, u16::MAX, 0] {
        bytes.extend_from_slice(&sequence.to_le_bytes());
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
    bytes[0x24..0x28].copy_from_slice(&6_u32.to_le_bytes());
    bytes[0x28..0x2c].copy_from_slice(&animation_lookup.to_le_bytes());
    bytes[0x2c..0x30].copy_from_slice(&1_u32.to_le_bytes());
    bytes[0x30..0x34].copy_from_slice(&bone_offset.to_le_bytes());
    let attachment_offset = m2_array_offset(&bytes, 0xf0)?;
    append_linear_track(&mut bytes, attachment_offset + 20, &[0, 1_000], &[1, 0], 1)?;
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

/// Adds one exact 176-byte ribbon record to the internal-sequence fixture.
fn animated_ribbon_m2_bytes() -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = animated_m2_bytes()?;
    let ribbon_offset = bytes.len();
    bytes.resize(ribbon_offset + 176, 0);
    bytes[ribbon_offset..ribbon_offset + 4].copy_from_slice(&0x5249_424E_u32.to_le_bytes());
    bytes[ribbon_offset + 4..ribbon_offset + 8].copy_from_slice(&0_u32.to_le_bytes());
    bytes[ribbon_offset + 8..ribbon_offset + 20].copy_from_slice(&f32_values(&[1.0, 2.0, 3.0]));

    let texture_indices = bytes.len();
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    set_header_array(&mut bytes, ribbon_offset + 20, 1, texture_indices)?;
    let material_indices = bytes.len();
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    set_header_array(&mut bytes, ribbon_offset + 28, 1, material_indices)?;

    append_linear_track(
        &mut bytes,
        ribbon_offset + 36,
        &[0, 1_000],
        &f32_values(&[1.0, 0.5, 0.25, 0.0, 1.0, 0.5]),
        12,
    )?;
    append_linear_track(
        &mut bytes,
        ribbon_offset + 56,
        &[0, 1_000],
        &i16_values(&[32_767, 16_384]),
        2,
    )?;
    append_linear_track(
        &mut bytes,
        ribbon_offset + 76,
        &[0, 1_000],
        &f32_values(&[1.0, 2.0]),
        4,
    )?;
    append_linear_track(
        &mut bytes,
        ribbon_offset + 96,
        &[0, 1_000],
        &f32_values(&[0.5, 1.5]),
        4,
    )?;
    bytes[ribbon_offset + 116..ribbon_offset + 120].copy_from_slice(&20.0_f32.to_le_bytes());
    bytes[ribbon_offset + 120..ribbon_offset + 124].copy_from_slice(&1.25_f32.to_le_bytes());
    bytes[ribbon_offset + 124..ribbon_offset + 128].copy_from_slice(&9.8_f32.to_le_bytes());
    bytes[ribbon_offset + 128..ribbon_offset + 130].copy_from_slice(&2_u16.to_le_bytes());
    bytes[ribbon_offset + 130..ribbon_offset + 132].copy_from_slice(&4_u16.to_le_bytes());
    append_linear_track(
        &mut bytes,
        ribbon_offset + 132,
        &[0, 1_000],
        &[0, 0, 3, 0],
        2,
    )?;
    append_linear_track(&mut bytes, ribbon_offset + 152, &[0, 1_000], &[1, 0], 1)?;
    bytes[ribbon_offset + 172..ribbon_offset + 174].copy_from_slice(&(-3_i16).to_le_bytes());
    bytes[ribbon_offset + 174] = -1_i8 as u8;
    bytes[ribbon_offset + 175] = -1_i8 as u8;
    set_header_array(&mut bytes, 0x120, 1, ribbon_offset)?;
    Ok(bytes)
}

/// Adds one complete 476-byte WotLK particle record to the internal sequence fixture.
fn animated_particle_m2_bytes() -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = animated_m2_bytes()?;
    let particle_offset = bytes.len();
    bytes.resize(particle_offset + 476, 0);
    bytes[particle_offset..particle_offset + 4].copy_from_slice(&0x5041_5254_u32.to_le_bytes());
    bytes[particle_offset + 4..particle_offset + 8].copy_from_slice(&0x9000_8042_u32.to_le_bytes());
    bytes[particle_offset + 8..particle_offset + 20].copy_from_slice(&f32_values(&[1.0, 2.0, 3.0]));
    bytes[particle_offset + 0x14..particle_offset + 0x16].copy_from_slice(&0_u16.to_le_bytes());
    bytes[particle_offset + 0x16..particle_offset + 0x18].copy_from_slice(&0_u16.to_le_bytes());

    let geometry_path = bytes.len();
    bytes.extend_from_slice(b"Spells\\ParticleGeometry.m2\0");
    let geometry_path_length = u32::try_from(bytes.len() - geometry_path)?;
    set_header_array(
        &mut bytes,
        particle_offset + 0x18,
        geometry_path_length,
        geometry_path,
    )?;
    let child_path = bytes.len();
    bytes.extend_from_slice(b"Spells\\ChildEmitter.m2\0");
    let child_path_length = u32::try_from(bytes.len() - child_path)?;
    set_header_array(
        &mut bytes,
        particle_offset + 0x20,
        child_path_length,
        child_path,
    )?;

    bytes[particle_offset + 0x28] = 2;
    bytes[particle_offset + 0x29] = 3;
    bytes[particle_offset + 0x2a..particle_offset + 0x2c].copy_from_slice(&12_u16.to_le_bytes());
    bytes[particle_offset + 0x2c] = 1;
    bytes[particle_offset + 0x2d] = 2;
    bytes[particle_offset + 0x2e..particle_offset + 0x30].copy_from_slice(&(-4_i16).to_le_bytes());
    bytes[particle_offset + 0x30..particle_offset + 0x32].copy_from_slice(&4_u16.to_le_bytes());
    bytes[particle_offset + 0x32..particle_offset + 0x34].copy_from_slice(&8_u16.to_le_bytes());

    for (track_offset, values) in [
        (0x034, [2.0, 4.0]),
        (0x048, [0.1, 0.2]),
        (0x05c, [0.3, 0.4]),
        (0x070, [0.5, 0.6]),
        (0x084, [9.0, 8.0]),
        (0x098, [1.0, 2.0]),
        (0x0b0, [10.0, 20.0]),
        (0x0c8, [3.0, 4.0]),
        (0x0dc, [5.0, 6.0]),
        (0x0f0, [7.0, 8.0]),
    ] {
        append_linear_track(
            &mut bytes,
            particle_offset + track_offset,
            &[0, 1_000],
            &f32_values(&values),
            4,
        )?;
    }
    bytes[particle_offset + 0x0ac..particle_offset + 0x0b0]
        .copy_from_slice(&0.25_f32.to_le_bytes());
    bytes[particle_offset + 0x0c4..particle_offset + 0x0c8].copy_from_slice(&0.5_f32.to_le_bytes());

    append_lifetime_track(
        &mut bytes,
        particle_offset + 0x104,
        &[0, i16::MAX as u16],
        &f32_values(&[1.0, 0.5, 0.25, 0.0, 1.0, 0.5]),
        12,
    )?;
    append_lifetime_track(
        &mut bytes,
        particle_offset + 0x114,
        &[0, i16::MAX as u16],
        &i16_values(&[32_767, 16_384]),
        2,
    )?;
    append_lifetime_track(
        &mut bytes,
        particle_offset + 0x124,
        &[0, i16::MAX as u16],
        &f32_values(&[1.0, 2.0, 3.0, 4.0]),
        8,
    )?;
    bytes[particle_offset + 0x134..particle_offset + 0x13c]
        .copy_from_slice(&f32_values(&[0.25, 0.5]));
    append_lifetime_track(
        &mut bytes,
        particle_offset + 0x13c,
        &[0, i16::MAX as u16],
        &[1, 0, 2, 0],
        2,
    )?;
    append_lifetime_track(
        &mut bytes,
        particle_offset + 0x14c,
        &[0, i16::MAX as u16],
        &[3, 0, 4, 0],
        2,
    )?;

    for (relative, value) in [
        (0x15c, 1.5_f32),
        (0x160, 2.5),
        (0x164, 0.75),
        (0x170, 0.6),
        (0x174, 0.7),
        (0x178, 0.8),
        (0x17c, 0.9),
        (0x180, 1.1),
        (0x184, 1.2),
        (0x1ac, 1.3),
        (0x1b0, 1.4),
        (0x1b4, 1.5),
        (0x1b8, 1.6),
        (0x1bc, 1.7),
    ] {
        bytes[particle_offset + relative..particle_offset + relative + 4]
            .copy_from_slice(&value.to_le_bytes());
    }
    bytes[particle_offset + 0x168..particle_offset + 0x170]
        .copy_from_slice(&f32_values(&[0.5, 1.5]));
    bytes[particle_offset + 0x188..particle_offset + 0x1a0]
        .copy_from_slice(&f32_values(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]));
    bytes[particle_offset + 0x1a0..particle_offset + 0x1ac]
        .copy_from_slice(&f32_values(&[7.0, 8.0, 9.0]));

    let spline_points = bytes.len();
    bytes.extend_from_slice(&f32_values(&[1.0, 3.0, 5.0, 2.0, 4.0, 6.0]));
    set_header_array(&mut bytes, particle_offset + 0x1c0, 2, spline_points)?;
    append_linear_track(&mut bytes, particle_offset + 0x1c8, &[0, 1_000], &[1, 0], 1)?;
    set_header_array(&mut bytes, 0x128, 1, particle_offset)?;
    Ok(bytes)
}

/// Adds one exact WotLK model-light record to the internal sequence fixture.
fn animated_light_m2_bytes() -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = animated_m2_bytes()?;
    let light_offset = bytes.len();
    bytes.resize(light_offset + 156, 0);
    bytes[light_offset..light_offset + 2].copy_from_slice(&1_u16.to_le_bytes());
    bytes[light_offset + 2..light_offset + 4].copy_from_slice(&0_i16.to_le_bytes());
    bytes[light_offset + 4..light_offset + 16].copy_from_slice(&f32_values(&[1.0, 2.0, 3.0]));
    append_linear_track(
        &mut bytes,
        light_offset + 0x10,
        &[0, 1_000],
        &f32_values(&[0.1, 0.2, 0.3, 0.4, 0.5, 0.6]),
        12,
    )?;
    append_linear_track(
        &mut bytes,
        light_offset + 0x24,
        &[0, 1_000],
        &f32_values(&[0.5, 1.0]),
        4,
    )?;
    append_linear_track(
        &mut bytes,
        light_offset + 0x38,
        &[0, 1_000],
        &f32_values(&[0.7, 0.8, 0.9, 1.0, 0.9, 0.8]),
        12,
    )?;
    append_linear_track(
        &mut bytes,
        light_offset + 0x4c,
        &[0, 1_000],
        &f32_values(&[1.0, 2.0]),
        4,
    )?;
    append_linear_track(
        &mut bytes,
        light_offset + 0x60,
        &[0, 1_000],
        &f32_values(&[3.0, 4.0]),
        4,
    )?;
    append_linear_track(
        &mut bytes,
        light_offset + 0x74,
        &[0, 1_000],
        &f32_values(&[5.0, 6.0]),
        4,
    )?;
    append_linear_track(&mut bytes, light_offset + 0x88, &[0, 1_000], &[1, 0], 1)?;
    set_header_array(&mut bytes, 0x108, 1, light_offset)?;
    Ok(bytes)
}

/// Adds one exact WotLK camera and its signed semantic lookup table.
fn animated_camera_m2_bytes() -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = animated_m2_bytes()?;
    let camera_offset = bytes.len();
    bytes.resize(camera_offset + 100, 0);
    bytes[camera_offset..camera_offset + 4].copy_from_slice(&(-1_i32).to_le_bytes());
    bytes[camera_offset + 4..camera_offset + 8].copy_from_slice(&1.0_f32.to_le_bytes());
    bytes[camera_offset + 8..camera_offset + 12].copy_from_slice(&500.0_f32.to_le_bytes());
    bytes[camera_offset + 12..camera_offset + 16].copy_from_slice(&0.25_f32.to_le_bytes());
    append_linear_track(
        &mut bytes,
        camera_offset + 16,
        &[0, 1_000],
        &f32_values(&[0.0, 1.0, 2.0, 3.0, 4.0, 5.0]),
        12,
    )?;
    bytes[camera_offset + 36..camera_offset + 48].copy_from_slice(&f32_values(&[6.0, 7.0, 8.0]));
    append_linear_track(
        &mut bytes,
        camera_offset + 48,
        &[0, 1_000],
        &f32_values(&[9.0, 10.0, 11.0, 12.0, 13.0, 14.0]),
        12,
    )?;
    bytes[camera_offset + 68..camera_offset + 80].copy_from_slice(&f32_values(&[15.0, 16.0, 17.0]));
    append_linear_track(
        &mut bytes,
        camera_offset + 80,
        &[0, 1_000],
        &f32_values(&[0.0, 0.5]),
        4,
    )?;
    let lookup_offset = bytes.len();
    bytes.extend_from_slice(&(-1_i16).to_le_bytes());
    bytes.extend_from_slice(&0_i16.to_le_bytes());
    set_header_array(&mut bytes, 0x110, 1, camera_offset)?;
    set_header_array(&mut bytes, 0x118, 2, lookup_offset)?;
    Ok(bytes)
}

/// Adds one exact WotLK event with one timestamp-only sequence channel.
fn animated_event_m2_bytes() -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = animated_m2_bytes()?;
    let event_offset = bytes.len();
    bytes.resize(event_offset + 36, 0);
    bytes[event_offset..event_offset + 4].copy_from_slice(b"$SND");
    bytes[event_offset + 4..event_offset + 8].copy_from_slice(&42_u32.to_le_bytes());
    bytes[event_offset + 8..event_offset + 12].copy_from_slice(&0_u32.to_le_bytes());
    bytes[event_offset + 12..event_offset + 24].copy_from_slice(&f32_values(&[1.0, 2.0, 3.0]));
    append_event_track(&mut bytes, event_offset + 24, &[125, 750])?;
    set_header_array(&mut bytes, 0x100, 1, event_offset)?;
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

/// Appends one timestamp-only M2 event channel and patches its 12-byte header.
fn append_event_track(
    bytes: &mut Vec<u8>,
    track_offset: usize,
    timestamps: &[u32],
) -> Result<(), Box<dyn Error>> {
    let timestamp_refs = bytes.len();
    bytes.extend_from_slice(&u32::try_from(timestamps.len())?.to_le_bytes());
    let timestamp_data_word = bytes.len();
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    let timestamp_data = bytes.len();
    for timestamp in timestamps {
        bytes.extend_from_slice(&timestamp.to_le_bytes());
    }
    bytes[timestamp_data_word..timestamp_data_word + 4]
        .copy_from_slice(&u32::try_from(timestamp_data)?.to_le_bytes());
    bytes[track_offset..track_offset + 2].copy_from_slice(&0_u16.to_le_bytes());
    bytes[track_offset + 2..track_offset + 4].copy_from_slice(&(-1_i16).to_le_bytes());
    bytes[track_offset + 4..track_offset + 8].copy_from_slice(&1_u32.to_le_bytes());
    bytes[track_offset + 8..track_offset + 12]
        .copy_from_slice(&u32::try_from(timestamp_refs)?.to_le_bytes());
    Ok(())
}

/// Appends one header-less WotLK particle lifetime ramp.
fn append_lifetime_track(
    bytes: &mut Vec<u8>,
    track_offset: usize,
    timestamps: &[u16],
    values: &[u8],
    value_stride: usize,
) -> Result<(), Box<dyn Error>> {
    if values.len() != timestamps.len() * value_stride {
        return Err("fixture lifetime-track value count differs from timestamps".into());
    }
    let timestamp_data = bytes.len();
    for timestamp in timestamps {
        bytes.extend_from_slice(&timestamp.to_le_bytes());
    }
    let value_data = bytes.len();
    bytes.extend_from_slice(values);
    set_header_array(
        bytes,
        track_offset,
        u32::try_from(timestamps.len())?,
        timestamp_data,
    )?;
    set_header_array(
        bytes,
        track_offset + 8,
        u32::try_from(timestamps.len())?,
        value_data,
    )?;
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
