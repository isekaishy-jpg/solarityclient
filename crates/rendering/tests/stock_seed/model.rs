//! External stock-compatibility tests for character-model render preparation.

use std::error::Error;
use std::io::Cursor;

use glam::{Mat4, Vec3, Vec4};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, BlpTextureCache, BlpTextureSource,
    CharacterAppearanceCatalog, CharacterCustomization, CharacterRaceCatalog, ClientDataRoot,
    DecodedM2Model, HelmetGeosetVisibilityCatalog, ItemDefinitionCatalog, ItemDisplayCatalog,
    Locale, M2BlendMode,
};
use solarity_ecs::PlayerEquipmentSlot;
use solarity_rendering::{
    BlpColorSpace, BlpTextureStorage, CharacterAtlasLayerKind, CharacterAtlasRegion,
    CharacterAttachmentPlan, CharacterAttachmentPoint, CharacterEquipmentItem,
    CharacterGeosetContext, CharacterGeosetPlan, CharacterRangedHand, CharacterTabardMode,
    CharacterTexturePlan, CharacterWeaponPose, CharacterWeaponState, M2AnimationClock, M2BonePose,
    M2DrawPushConstants, M2LocalLightCount, M2LocalLightState, M2MaterialPose, M2MaterialState,
    M2MaterialUniform, M2MeshPlan, M2MeshPlanError, M2ParticleLifetimePose,
    M2ParticleLifetimePoseError, M2ParticleMeshPlan, M2ParticlePose, M2ParticleRandom,
    M2ParticleRotationPose, M2ParticleSimulation, M2ParticleState, M2PixelShader,
    M2RibbonControlPoint, M2RibbonMeshPlan, M2RibbonPose, M2RibbonRenderVertex,
    M2RibbonSpirvCompiler, M2RibbonTrail, M2SampledTexture, M2SceneUniform, M2ShaderPermutation,
    M2ShaderPlan, M2ShadowFiltering, M2ShadowPermutation, M2SpirvCompiler, M2TextureAddressMode,
    M2TextureSet, M2VertexShader, TerrainSceneUniform, VulkanBootstrap, VulkanError, WorldCamera,
    WorldFrameScene, WorldModelSceneUniform,
};
use wow_m2::chunks::material::{
    M2BlendMode as RawBlendMode, M2Material as RawMaterial, M2RenderFlags,
};
use wow_m2::chunks::texture::{M2Texture as RawTexture, M2TextureFlags, M2TextureType};
use wow_m2::chunks::vertex::M2Vertex as RawM2Vertex;
use wow_m2::common::{C2Vector, C3Vector, FixedString, M2Array, M2ArrayString};
use wow_m2::header::{M2Header, M2ModelFlags};
use wow_m2::skin::{OldSkinHeader, SkinBatch, SkinSubmesh};
use wow_m2::{M2Model, M2Version, OldSkin};

use crate::support::{Fixture, FixtureFile};

/// Base player customization produces stock atlas and M2 replacement bindings.
#[test]
fn character_texture_plan_preserves_stock_regions_and_layer_order() -> Result<(), Box<dyn Error>> {
    let tables = character_tables(0, 0);
    let skin = solid_raw3_blp(
        512,
        512,
        &[
            0xFFFF_0000,
            0xFF00_00FF,
            0xFF00_00FF,
            0xFF00_00FF,
            0xFF00_00FF,
            0xFF00_00FF,
            0xFF00_00FF,
            0xFF00_00FF,
            0xFF00_00FF,
            0xFF00_00FF,
        ],
    );
    let overlay = solid_raw3_blp(
        256,
        128,
        &[
            0x80FF_0000,
            0x8000_FF00,
            0x8000_FF00,
            0x8000_FF00,
            0x8000_FF00,
            0x8000_FF00,
            0x8000_FF00,
            0x8000_FF00,
            0x8000_FF00,
        ],
    );
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "DBFilesClient\\CharSections.dbc",
            bytes: &tables.sections,
        },
        FixtureFile {
            path: "DBFilesClient\\CharHairGeosets.dbc",
            bytes: &tables.hair_geosets,
        },
        FixtureFile {
            path: "DBFilesClient\\CharacterFacialHairStyles.dbc",
            bytes: &tables.facial_hair,
        },
        FixtureFile {
            path: "Character\\Human\\Male\\Skin.blp",
            bytes: &skin,
        },
        FixtureFile {
            path: "Character\\Human\\Male\\FaceLower.blp",
            bytes: &overlay,
        },
        FixtureFile {
            path: "Character\\Human\\Male\\FaceUpper.blp",
            bytes: &overlay,
        },
        FixtureFile {
            path: "Character\\Human\\Male\\FacialLower.blp",
            bytes: &overlay,
        },
        FixtureFile {
            path: "Character\\Human\\Male\\FacialUpper.blp",
            bytes: &overlay,
        },
        FixtureFile {
            path: "Character\\Human\\Male\\HairLower.blp",
            bytes: &overlay,
        },
        FixtureFile {
            path: "Character\\Human\\Male\\HairUpper.blp",
            bytes: &overlay,
        },
        FixtureFile {
            path: "Character\\Human\\Male\\UnderwearLower.blp",
            bytes: &overlay,
        },
        FixtureFile {
            path: "Character\\Human\\Male\\UnderwearUpper.blp",
            bytes: &overlay,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let characters = CharacterAppearanceCatalog::load(&mut store)?;
    let appearance = characters.resolve_player(1, 0, CharacterCustomization::new(2, 3, 4, 5, 6))?;

    let plan = CharacterTexturePlan::base(&appearance)?;

    assert_eq!(plan.atlas_size(), 256);
    assert_eq!(plan.atlas_layers().len(), 16);
    let visible_order = plan
        .atlas_layers()
        .iter()
        .map(|layer| (layer.region(), layer.kind()))
        .collect::<Vec<_>>();
    assert_eq!(
        visible_order,
        [
            (
                CharacterAtlasRegion::ArmUpper,
                CharacterAtlasLayerKind::Skin
            ),
            (
                CharacterAtlasRegion::ArmLower,
                CharacterAtlasLayerKind::Skin
            ),
            (CharacterAtlasRegion::Hand, CharacterAtlasLayerKind::Skin),
            (
                CharacterAtlasRegion::TorsoUpper,
                CharacterAtlasLayerKind::Skin
            ),
            (
                CharacterAtlasRegion::TorsoUpper,
                CharacterAtlasLayerKind::Underwear,
            ),
            (
                CharacterAtlasRegion::TorsoLower,
                CharacterAtlasLayerKind::Skin
            ),
            (
                CharacterAtlasRegion::LegUpper,
                CharacterAtlasLayerKind::Skin
            ),
            (
                CharacterAtlasRegion::LegUpper,
                CharacterAtlasLayerKind::Underwear,
            ),
            (
                CharacterAtlasRegion::LegLower,
                CharacterAtlasLayerKind::Skin
            ),
            (CharacterAtlasRegion::Foot, CharacterAtlasLayerKind::Skin),
            (
                CharacterAtlasRegion::HeadUpper,
                CharacterAtlasLayerKind::Face
            ),
            (
                CharacterAtlasRegion::HeadUpper,
                CharacterAtlasLayerKind::FacialHair,
            ),
            (
                CharacterAtlasRegion::HeadUpper,
                CharacterAtlasLayerKind::Hair
            ),
            (
                CharacterAtlasRegion::HeadLower,
                CharacterAtlasLayerKind::Face
            ),
            (
                CharacterAtlasRegion::HeadLower,
                CharacterAtlasLayerKind::FacialHair,
            ),
            (
                CharacterAtlasRegion::HeadLower,
                CharacterAtlasLayerKind::Hair
            ),
        ]
    );
    let head_lower = CharacterAtlasRegion::HeadLower.rect();
    assert_eq!(
        (
            head_lower.x(),
            head_lower.y(),
            head_lower.width(),
            head_lower.height(),
        ),
        (0, 192, 128, 64)
    );
    assert_eq!(
        plan.hair().map(|path| path.as_str()),
        Some("CHARACTER\\HUMAN\\MALE\\HAIR.BLP")
    );
    assert_eq!(
        plan.extra_skin().map(|path| path.as_str()),
        Some("CHARACTER\\HUMAN\\MALE\\SKINEXTRA.BLP")
    );

    let mut texture_cache = BlpTextureCache::new();
    let atlas = plan.compose(&mut store, &mut texture_cache)?;
    assert_eq!(atlas.mips().len(), 9);
    assert_eq!(atlas.mip(0).map(|mip| mip.width()), Some(256));
    assert_eq!(atlas.mip(8).map(|mip| mip.width()), Some(1));
    let top = atlas.mip(0).ok_or("top character atlas mip is absent")?;
    // The 512-pixel HD skin selects authored mip one instead of resampling mip
    // zero. Underwear and head overlays likewise select their authored mip one.
    assert_eq!(
        rgba8_pixel(top.rgba8(), top.width(), 10, 10),
        [0, 0, 255, 255]
    );
    assert_eq!(
        rgba8_pixel(top.rgba8(), top.width(), 130, 10),
        [0, 128, 127, 255]
    );
    assert_eq!(
        rgba8_pixel(top.rgba8(), top.width(), 10, 170),
        [0, 223, 0, 255]
    );
    assert_eq!(texture_cache.len(), 9);
    Ok(())
}

/// Equipped textures follow stock priorities and universal-first suffix lookup.
#[test]
fn equipped_character_plan_orders_item_components() -> Result<(), Box<dyn Error>> {
    let characters = character_tables(0, 1);
    let items = equipment_tables();
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "DBFilesClient\\CharSections.dbc",
            bytes: &characters.sections,
        },
        FixtureFile {
            path: "DBFilesClient\\CharHairGeosets.dbc",
            bytes: &characters.hair_geosets,
        },
        FixtureFile {
            path: "DBFilesClient\\CharacterFacialHairStyles.dbc",
            bytes: &characters.facial_hair,
        },
        FixtureFile {
            path: "DBFilesClient\\Item.dbc",
            bytes: &items.definitions,
        },
        FixtureFile {
            path: "DBFilesClient\\ItemDisplayInfo.dbc",
            bytes: &items.displays,
        },
        FixtureFile {
            path: "Item\\TextureComponents\\ArmUpperTexture\\ShirtAU_U.blp",
            bytes: b"universal shirt",
        },
        FixtureFile {
            path: "Item\\TextureComponents\\ArmLowerTexture\\ShirtAL_U.blp",
            bytes: b"universal shirt",
        },
        FixtureFile {
            path: "Item\\TextureComponents\\TorsoUpperTexture\\ShirtTU_U.blp",
            bytes: b"universal shirt",
        },
        FixtureFile {
            path: "Item\\TextureComponents\\ArmUpperTexture\\ChestAU_U.blp",
            bytes: b"universal chest",
        },
        FixtureFile {
            path: "Item\\TextureComponents\\ArmUpperTexture\\ChestAU_F.blp",
            bytes: b"female chest must lose to universal",
        },
        FixtureFile {
            path: "Item\\TextureComponents\\ArmLowerTexture\\ChestAL_U.blp",
            bytes: b"universal chest",
        },
        FixtureFile {
            path: "Item\\TextureComponents\\TorsoUpperTexture\\ChestTU_U.blp",
            bytes: b"universal chest",
        },
        FixtureFile {
            path: "Item\\TextureComponents\\ArmLowerTexture\\GloveAL_F.blp",
            bytes: b"female gloves",
        },
        FixtureFile {
            path: "Item\\TextureComponents\\HandTexture\\GloveHA_F.blp",
            bytes: b"female gloves",
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let character_catalog = CharacterAppearanceCatalog::load(&mut store)?;
    let definitions = ItemDefinitionCatalog::load(&mut store)?;
    let displays = ItemDisplayCatalog::load(&mut store)?;
    let appearance =
        character_catalog.resolve_player(1, 1, CharacterCustomization::new(2, 3, 4, 5, 6))?;
    let shirt_definition = definitions.item(50_001).ok_or("shirt item is absent")?;
    let chest_definition = definitions.item(50_002).ok_or("chest item is absent")?;
    let glove_definition = definitions.item(50_003).ok_or("glove item is absent")?;
    let shirt_display = displays.display(55_001).ok_or("shirt display is absent")?;
    let chest_display = displays.display(55_002).ok_or("chest display is absent")?;
    let glove_display = displays.display(55_003).ok_or("glove display is absent")?;

    let plan = CharacterTexturePlan::equipped(
        &appearance,
        &store,
        [
            CharacterEquipmentItem::new(
                PlayerEquipmentSlot::Shirt,
                shirt_definition,
                shirt_display,
            ),
            CharacterEquipmentItem::new(
                PlayerEquipmentSlot::Chest,
                chest_definition,
                chest_display,
            ),
            CharacterEquipmentItem::new(
                PlayerEquipmentSlot::Hands,
                glove_definition,
                glove_display,
            ),
        ],
    )?;

    let item_layers = plan
        .atlas_layers()
        .iter()
        .filter(|layer| layer.kind() == CharacterAtlasLayerKind::Item)
        .map(|layer| (layer.region(), layer.path().as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        item_layers,
        [
            (
                CharacterAtlasRegion::ArmUpper,
                "ITEM\\TEXTURECOMPONENTS\\ARMUPPERTEXTURE\\SHIRTAU_U.BLP",
            ),
            (
                CharacterAtlasRegion::ArmUpper,
                "ITEM\\TEXTURECOMPONENTS\\ARMUPPERTEXTURE\\CHESTAU_U.BLP",
            ),
            (
                CharacterAtlasRegion::ArmLower,
                "ITEM\\TEXTURECOMPONENTS\\ARMLOWERTEXTURE\\SHIRTAL_U.BLP",
            ),
            (
                CharacterAtlasRegion::ArmLower,
                "ITEM\\TEXTURECOMPONENTS\\ARMLOWERTEXTURE\\CHESTAL_U.BLP",
            ),
            (
                CharacterAtlasRegion::ArmLower,
                "ITEM\\TEXTURECOMPONENTS\\ARMLOWERTEXTURE\\GLOVEAL_F.BLP",
            ),
            (
                CharacterAtlasRegion::Hand,
                "ITEM\\TEXTURECOMPONENTS\\HANDTEXTURE\\GLOVEHA_F.BLP",
            ),
            (
                CharacterAtlasRegion::TorsoUpper,
                "ITEM\\TEXTURECOMPONENTS\\TORSOUPPERTEXTURE\\SHIRTTU_U.BLP",
            ),
            (
                CharacterAtlasRegion::TorsoUpper,
                "ITEM\\TEXTURECOMPONENTS\\TORSOUPPERTEXTURE\\CHESTTU_U.BLP",
            ),
        ]
    );
    assert!(!plan.atlas_layers().iter().any(|layer| {
        layer.region() == CharacterAtlasRegion::TorsoUpper
            && layer.kind() == CharacterAtlasLayerKind::Underwear
    }));
    assert!(plan.atlas_layers().iter().any(|layer| {
        layer.region() == CharacterAtlasRegion::LegUpper
            && layer.kind() == CharacterAtlasLayerKind::Underwear
    }));
    Ok(())
}

/// Ribbon animation uses the model sequence clock while retaining discrete selectors.
#[test]
fn m2_ribbon_pose_samples_placement_effect_values() -> Result<(), Box<dyn Error>> {
    let model = render_m2_bytes("RibbonPose", 1)?;
    let skin = render_skin_bytes()?;
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Creature\\Solarity\\RibbonPose.m2",
            bytes: &model,
        },
        FixtureFile {
            path: "Creature\\Solarity\\RibbonPose00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\RibbonPose.m2")?;
    let model = DecodedM2Model::load(&mut store, &path)?;
    let emitter = model
        .animations()
        .ribbons()
        .first()
        .ok_or("ribbon emitter is absent")?;
    let ribbon_material = model.materials()[usize::from(emitter.material_indices()[0])];
    let ribbon_compiler = M2RibbonSpirvCompiler::new()?;
    let ribbon_spirv = ribbon_compiler.compile(M2MaterialState::from_material(ribbon_material))?;
    assert_spirv_1_6(ribbon_spirv.vertex_words());
    assert_spirv_1_6(ribbon_spirv.fragment_words());

    let pose = M2RibbonPose::sample(
        model.animations(),
        emitter,
        M2AnimationClock::new(0, 500.0, 0.0),
    )?;
    assert_eq!(pose.color().truncate(), Vec3::new(2.0, 1.5, 1.0));
    assert!((pose.color().w - 0.75).abs() < 0.000_1);
    assert_eq!(pose.height_above(), 2.0);
    assert_eq!(pose.height_below(), 1.0);
    assert_eq!(pose.texture_slot(), 1);
    assert!(pose.visible());

    let mut trail = M2RibbonTrail::new(emitter)?;
    assert_eq!(trail.capacity(), 27);
    let first = M2RibbonControlPoint::new(Vec3::ZERO, Vec3::Y, Vec3::X);
    trail.advance(4.0, first, pose)?;
    assert_eq!(trail.sections().len(), 2);
    let first_sections = trail.sections().copied().collect::<Vec<_>>();
    assert_eq!(first_sections[0].above(), Vec3::new(0.0, 2.0, 0.0));
    assert_eq!(first_sections[0].below(), Vec3::new(0.0, -1.0, 0.0));

    let second = M2RibbonControlPoint::new(Vec3::X, Vec3::Y, Vec3::X);
    trail.advance(0.05, second, pose)?;
    assert_eq!(trail.sections().len(), 3);
    let aged = trail.sections().next().ok_or("ribbon trail is empty")?;
    assert!((aged.age_seconds() - 0.05).abs() < 0.000_1);
    assert!((aged.above().z - 0.0245).abs() < 0.000_1);
    assert_eq!(trail.texture_slot(), 1);

    let mesh = M2RibbonMeshPlan::prepare(emitter, &trail)?;
    assert_eq!(mesh.vertices().len(), 6);
    assert_eq!(
        mesh.vertex_bytes().len(),
        6 * M2RibbonRenderVertex::BYTE_SIZE
    );
    let oldest_above = mesh.vertices().first().ok_or("ribbon mesh is empty")?;
    assert_eq!(oldest_above.position(), aged.above().to_array());
    assert_eq!(oldest_above.color_bgra(), [255, 255, 255, 191]);
    assert!((oldest_above.texture_coordinates()[0] - 0.26).abs() < 0.000_1);
    assert_eq!(oldest_above.texture_coordinates()[1], 0.0);
    assert_eq!(mesh.vertices()[1].texture_coordinates()[1], 0.5);
    assert_eq!(
        mesh.vertices()
            .last()
            .ok_or("ribbon mesh is empty")?
            .texture_coordinates(),
        [0.25, 0.5]
    );

    let final_pose = M2RibbonPose::sample(
        model.animations(),
        emitter,
        M2AnimationClock::new(0, 1_000.0, 0.0),
    )?;
    assert_eq!(final_pose.texture_slot(), 3);
    assert!(!final_pose.visible());
    trail.advance(0.1, second, final_pose)?;
    assert_eq!(trail.sections().len(), 3);
    Ok(())
}

/// Emitter-clock tracks and per-particle lifetime ramps retain separate clocks.
#[test]
fn m2_particle_poses_sample_stock_track_domains() -> Result<(), Box<dyn Error>> {
    let bytes = render_m2_bytes("Particle.blp", 1)?;
    let skin = render_skin_bytes()?;
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Creature\\Solarity\\ParticlePose.m2",
            bytes: &bytes,
        },
        FixtureFile {
            path: "Creature\\Solarity\\ParticlePose00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\ParticlePose.m2")?;
    let model = DecodedM2Model::load(&mut store, &path)?;
    let emitter = model
        .animations()
        .particles()
        .first()
        .ok_or("particle emitter is absent")?;

    let pose = M2ParticlePose::sample(
        model.animations(),
        emitter,
        M2AnimationClock::new(0, 500.0, 0.0),
    )?;
    assert_eq!(pose.emission_speed(), 3.0);
    assert_eq!(pose.speed_variation(), 0.3);
    assert_eq!(pose.vertical_range(), 0.6);
    assert!((pose.horizontal_range() - 0.9).abs() < f32::EPSILON * 2.0);
    assert_eq!(pose.gravity(), 9.0);
    assert_eq!(pose.lifespan(), 2.0);
    assert_eq!(pose.emission_rate(), 15.0);
    assert_eq!(pose.emission_area_width(), 6.0);
    assert_eq!(pose.emission_area_length(), 4.0);
    assert_eq!(pose.z_source(), 2.0);
    assert!(pose.enabled());
    let disabled = M2ParticlePose::sample(
        model.animations(),
        emitter,
        M2AnimationClock::new(0, 1_000.0, 0.0),
    )?;
    assert!(!disabled.enabled());

    let random_word = 0x1234;
    let lifetime = M2ParticleLifetimePose::sample(emitter, 0.5, random_word)?;
    assert!(
        (lifetime.color() - Vec4::new(0.5, 0.0, 0.5, 0.75))
            .abs()
            .max_element()
            < 0.0001
    );
    assert!(
        {
            let mut random = M2ParticleRandom::new(u32::from(random_word));
            let expected = glam::Vec2::new(
                2.0 * (1.0 + random.next_signed() * 0.5),
                3.0 * (1.0 + random.next_signed() * 0.25),
            );
            lifetime.scale() - expected
        }
        .abs()
        .max_element()
            < 0.0001
    );
    assert_eq!(lifetime.head_texture_cell(), 2);
    assert_eq!(lifetime.tail_texture_cell(), 4);
    let rotation = M2ParticleRotationPose::sample(emitter, random_word);
    let mut rotation_random = M2ParticleRandom::new(u32::from(random_word));
    let expected_initial = 0.8 + rotation_random.next_signed() * 0.9;
    let expected_speed = 1.1 + rotation_random.next_signed() * 1.2;
    assert_eq!(rotation.initial_radians(), expected_initial);
    assert_eq!(rotation.radians_per_second(), expected_speed);
    assert_eq!(
        rotation.angle_radians(0.5),
        expected_speed * 0.5 + expected_initial
    );
    assert_eq!(
        M2ParticleLifetimePose::sample(emitter, f32::NAN, random_word),
        Err(M2ParticleLifetimePoseError::NonFiniteAge)
    );
    Ok(())
}

/// An absent head ramp uses stock's multiply-high random atlas selection.
#[test]
fn m2_particle_pose_selects_random_head_cell_in_stock_order() -> Result<(), Box<dyn Error>> {
    let mut bytes = render_m2_bytes("Particle.blp", 1)?;
    let particle_offset = usize::try_from(u32::from_le_bytes(bytes[0x12c..0x130].try_into()?))?;
    bytes[particle_offset + 4..particle_offset + 8].copy_from_slice(&0x0010_0000_u32.to_le_bytes());
    bytes[particle_offset + 0x13c..particle_offset + 0x140].copy_from_slice(&0_u32.to_le_bytes());
    bytes[particle_offset + 0x144..particle_offset + 0x148].copy_from_slice(&0_u32.to_le_bytes());
    let skin = render_skin_bytes()?;
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Creature\\Solarity\\RandomParticleCell.m2",
            bytes: &bytes,
        },
        FixtureFile {
            path: "Creature\\Solarity\\RandomParticleCell00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\RandomParticleCell.m2")?;
    let model = DecodedM2Model::load(&mut store, &path)?;
    let emitter = model
        .animations()
        .particles()
        .first()
        .ok_or("particle emitter is absent")?;
    let random_word = 0x1234;
    let pose = M2ParticleLifetimePose::sample(emitter, 0.5, random_word)?;
    let mut random = M2ParticleRandom::new(u32::from(random_word));
    let expected_cell = ((u64::from(random.next_u32()) * 8) >> 32) as u32;
    let expected_scale = glam::Vec2::new(2.0, 3.0) * (1.0 + random.next_signed() * 0.5);

    assert_eq!(pose.head_texture_cell(), expected_cell);
    assert!((pose.scale() - expected_scale).abs().max_element() < 0.0001);
    Ok(())
}

/// Planar emission grows its placement-local pool from the stock estimate.
#[test]
fn m2_planar_particle_simulation_grows_stock_capacity() -> Result<(), Box<dyn Error>> {
    let bytes = render_m2_bytes("Particle.blp", 1)?;
    let skin = render_skin_bytes()?;
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Creature\\Solarity\\ParticleSimulation.m2",
            bytes: &bytes,
        },
        FixtureFile {
            path: "Creature\\Solarity\\ParticleSimulation00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\ParticleSimulation.m2")?;
    let model = DecodedM2Model::load(&mut store, &path)?;
    let emitter = model
        .animations()
        .particles()
        .first()
        .ok_or("particle emitter is absent")?;
    let pose = M2ParticlePose::sample(
        model.animations(),
        emitter,
        M2AnimationClock::new(0, 500.0, 0.0),
    )?;
    let initial_pose = M2ParticlePose::sample(
        model.animations(),
        emitter,
        M2AnimationClock::new(0, 0.0, 0.0),
    )?;
    let mut simulation = M2ParticleSimulation::new(0x0029_4823);
    let initial = simulation.advance_planar(emitter, initial_pose, 0.0, Mat4::IDENTITY, 1.0)?;
    assert_eq!(initial.live(), 0);
    // The executable constant is the float immediately below 1.15, so the
    // nominal 11.5 estimate rounds down after extended-precision evaluation.
    assert_eq!(simulation.capacity(), 11);
    let report = simulation.advance_planar(
        emitter,
        pose,
        0.2,
        Mat4::from_translation(Vec3::new(10.0, 20.0, 30.0)),
        1.0,
    )?;

    assert_eq!(report.emitted(), 4);
    assert_eq!(report.deaths(), 0);
    assert_eq!(report.live(), 4);
    assert_eq!(simulation.capacity(), 34);
    assert_eq!(simulation.particles().len(), 4);
    assert_eq!(simulation.emission_remainder(), -1.0);
    assert!(
        simulation.particles().iter().all(|particle| {
            particle.position().is_finite()
                && particle.velocity().is_finite()
                && particle.position().x >= 7.0
                && particle.position().x <= 13.0
                && particle.position().y >= 16.0
                && particle.position().y <= 24.0
        }),
        "{:?}",
        simulation.particles()
    );
    let camera = WorldCamera::stock(Vec3::ZERO, Vec3::X, Vec3::Z, 100.0).frame(1.0)?;
    let mesh = M2ParticleMeshPlan::prepare(emitter, pose, simulation.particles(), camera, 1.0)?;
    assert_eq!(mesh.vertices().len(), 32);
    assert_eq!(mesh.indices().len(), 48);
    assert_eq!(&mesh.indices()[..6], &[0, 1, 2, 2, 1, 3]);
    assert!(
        mesh.vertices()
            .iter()
            .all(|vertex| vertex.normal() == Vec3::NEG_X.to_array())
    );
    assert_eq!(mesh.vertex_bytes().len(), 32 * 36);

    let particle = M2ParticleState::new(
        0.5,
        Vec3::new(10.0, 20.0, 30.0),
        Vec3::new(0.0, 2.0, 0.0),
        0x2483,
    )?;
    let mesh = M2ParticleMeshPlan::prepare(emitter, pose, &[particle], camera, 1.0)?;
    let endpoint = Vec3::from_array(mesh.vertices()[6].position())
        .midpoint(Vec3::from_array(mesh.vertices()[7].position()));
    assert_eq!(endpoint, particle.position() - particle.velocity() * 1.5);
    Ok(())
}

/// Spherical emission samples a bounded shell and its dedicated launch path.
#[test]
fn m2_sphere_particle_simulation_uses_authored_shell() -> Result<(), Box<dyn Error>> {
    let mut bytes = render_m2_bytes("Particle.blp", 1)?;
    let particle_offset = usize::try_from(u32::from_le_bytes(bytes[0x12c..0x130].try_into()?))?;
    bytes[particle_offset + 4..particle_offset + 8].copy_from_slice(&0x8000_u32.to_le_bytes());
    bytes[particle_offset + 41] = 2;
    let skin = render_skin_bytes()?;
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Creature\\Solarity\\SphereParticle.m2",
            bytes: &bytes,
        },
        FixtureFile {
            path: "Creature\\Solarity\\SphereParticle00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\SphereParticle.m2")?;
    let model = DecodedM2Model::load(&mut store, &path)?;
    let emitter = model
        .animations()
        .particles()
        .first()
        .ok_or("particle emitter is absent")?;
    assert_eq!(emitter.flags(), 0x8000);
    let pose = M2ParticlePose::sample(
        model.animations(),
        emitter,
        M2AnimationClock::new(0, 500.0, 0.0),
    )?;
    let mut simulation = M2ParticleSimulation::new(0x0029_4823);
    let report = simulation.advance_sphere(emitter, pose, 0.1, Mat4::IDENTITY, 1.0)?;

    assert_eq!(report.emitted(), 2);
    assert_eq!(report.deaths(), 0);
    assert_eq!(report.live(), 2);
    assert!(simulation.particles().iter().all(|particle| {
        let position = particle.position();
        let horizontal_radius = position.truncate().length();
        position.is_finite()
            && particle.velocity().is_finite()
            && horizontal_radius <= pose.emission_area_width() + pose.emission_speed() * 0.1
    }));
    Ok(())
}

/// M2 geometry resolves SKIN indirection and reaches renderer-owned GPU buffers.
#[test]
// SDL and the renderer require an explicit ownership transfer for the native
// surface; both unsafe calls are constrained to the live window/instance below.
#[allow(unsafe_code)]
fn m2_mesh_plan_prepares_direct_gpu_geometry() -> Result<(), Box<dyn Error>> {
    let model = render_m2_bytes("Renderable", 1)?;
    let skin = render_skin_bytes()?;
    let texture = solid_raw3_blp(2, 2, &[0xFFFF_0000, 0xFF00_FF00]);
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Creature\\Solarity\\Renderable.m2",
            bytes: &model,
        },
        FixtureFile {
            path: "Creature\\Solarity\\Renderable00.skin",
            bytes: &skin,
        },
        FixtureFile {
            path: "Creature\\Solarity\\Renderable.blp",
            bytes: &texture,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Creature\\Solarity\\Renderable.m2")?;
    let model = DecodedM2Model::load(&mut store, &path)?;
    let texture_path = AssetPath::new("Creature\\Solarity\\Renderable.blp")?;
    let texture_source = BlpTextureSource::load(&mut store, &texture_path)?;
    let pose = M2BonePose::compose(model.animations(), M2AnimationClock::new(0, 500.0, 0.0))?;
    assert_eq!(pose.transforms().len(), 3);
    assert_eq!(
        pose.transforms()[2].transform_point3(Vec3::ZERO),
        Vec3::new(2.0, 0.0, 0.0)
    );

    let plan = M2MeshPlan::prepare(&model, 0)?;
    assert_eq!(plan.path(), &path);
    assert_eq!(plan.profile_index(), 0);
    assert_eq!(plan.vertices().len(), 3);
    assert_eq!(plan.vertex_bytes().len(), 3 * 52);
    assert_eq!(plan.indices(), &[0, 1, 2]);
    assert_eq!(plan.index_bytes(), [0, 0, 1, 0, 2, 0]);
    assert_eq!(plan.vertices()[0].position(), [2.0, 2.25, 2.5]);
    assert_eq!(plan.vertices()[0].bone_weights(), [255, 0, 0, 0]);
    assert_eq!(plan.vertices()[0].bone_indices(), [2, 0, 0, 0]);
    let draw = plan.draws().first().ok_or("M2 draw is absent")?;
    let material_pose =
        M2MaterialPose::sample(&model, &plan, 0, M2AnimationClock::new(0, 500.0, 0.0))?;
    assert_eq!(material_pose.mesh_color().truncate(), Vec3::splat(1.5));
    assert!((material_pose.mesh_color().w - 0.5625).abs() < 0.000_1);
    for transform in material_pose.texture_transforms() {
        assert_eq!(
            transform.transform_point3(Vec3::ZERO),
            Vec3::new(0.25, 0.0, 0.0)
        );
    }
    assert_eq!(draw.geoset_id(), 402);
    assert_eq!(draw.first_index(), 0);
    assert_eq!(draw.index_count(), 3);
    assert_eq!(draw.batch().shader_id, 0x8001);
    assert_eq!(draw.batch().priority_plane, -2);
    assert!(draw.transparent_sort_unit());
    assert_eq!(draw.material().blend_mode(), M2BlendMode::AlphaKey);
    assert_eq!(
        draw.texture_bindings()
            .iter()
            .map(|binding| {
                (
                    binding.stage(),
                    binding.texture_index(),
                    binding.texture_coordinate(),
                )
            })
            .collect::<Vec<_>>(),
        [(0, 0, 0), (1, 1, -1)]
    );
    let specialized = M2ShaderPlan::resolve(&model, draw)?;
    assert_eq!(specialized.requested_shader_id(), 0x8001);
    assert_eq!(specialized.resolved_shader_id(), 0x8001);
    assert_eq!(specialized.texture_count(), 2);
    assert_eq!(specialized.vertex_shader(), M2VertexShader::DiffuseT1Env);
    assert_eq!(
        specialized.pixel_shader(),
        M2PixelShader::OpaqueMod2xNoAlphaAlpha
    );
    assert!(!specialized.used_stock_fallback());
    let state = specialized.material();
    assert!(!state.blend_enabled());
    assert!(!state.cull_enabled());
    assert!(!state.depth_test_enabled());
    assert!(!state.depth_write_enabled());
    assert!(!state.is_unlit());
    assert!(state.is_unfogged());
    assert!((state.alpha_reference(0.5) - (112.0 / 255.0)).abs() < f32::EPSILON);
    let faded = specialized.with_runtime_alpha_fade().material();
    assert!(faded.blend_enabled());
    assert_eq!(
        faded.source_blend(),
        solarity_rendering::M2BlendFactor::SourceAlpha
    );
    assert_eq!(
        faded.destination_blend(),
        solarity_rendering::M2BlendFactor::OneMinusSourceAlpha
    );
    assert!(!faded.depth_write_enabled());

    let simple = M2ShaderPlan::resolve(&model, &plan.draws()[1])?;
    assert_eq!(simple.requested_shader_id(), 0);
    assert_eq!(simple.resolved_shader_id(), 0x000E);
    assert_eq!(simple.vertex_shader(), M2VertexShader::DiffuseT1Env);
    assert_eq!(simple.pixel_shader(), M2PixelShader::OpaqueMod2xNoAlpha);
    assert!(!simple.used_stock_fallback());

    let fallback = M2ShaderPlan::resolve(&model, &plan.draws()[2])?;
    assert_eq!(fallback.requested_shader_id(), 2);
    assert_eq!(fallback.resolved_shader_id(), 0x11);
    assert_eq!(fallback.vertex_shader(), M2VertexShader::DiffuseT1T2);
    assert_eq!(fallback.pixel_shader(), M2PixelShader::ModMod);
    assert!(fallback.used_stock_fallback());
    assert!(fallback.material().is_unlit());

    let lit_permutation = M2ShaderPermutation::resolve(
        draw,
        M2LocalLightCount::Three,
        M2ShadowPermutation::Disabled,
        M2ShadowFiltering::Direct,
    );
    assert_eq!(draw.bone_count(), 1);
    assert_eq!(draw.bone_start(), 0);
    assert_eq!(draw.bone_influence(), 1);
    assert_eq!(lit_permutation.vertex_index(), 17);
    assert_eq!(lit_permutation.pixel_index(), 8);
    let scene_uniform = M2SceneUniform::new(
        Mat4::IDENTITY,
        Vec3::new(1.0, 2.0, 3.0),
        Vec3::splat(0.1),
        Vec3::splat(0.8),
        Vec3::Z,
        Vec4::new(10.0, 100.0, 1.0, 1.0),
        [M2LocalLightState::disabled(); 4],
    );
    assert_eq!(scene_uniform.to_bytes().len(), M2SceneUniform::BYTE_SIZE);
    let material_uniform = M2MaterialUniform::new(
        Mat4::IDENTITY,
        [Mat4::IDENTITY; 2],
        Mat4::IDENTITY,
        Vec4::ONE,
        Vec4::ZERO,
        Vec4::new(state.alpha_reference(0.5), 0.0, 0.0, 0.0),
    );
    assert_eq!(
        material_uniform.to_bytes().len(),
        M2MaterialUniform::BYTE_SIZE
    );
    let push_constants = M2DrawPushConstants::new(64, draw, 0x20);
    assert_eq!(
        push_constants.to_bytes(),
        [64, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 0x20, 0, 0, 0]
    );
    let compiler = M2SpirvCompiler::new()?;
    let spirv = compiler.compile(specialized, lit_permutation)?;
    assert_eq!(spirv.key().plan(), specialized);
    assert_spirv_1_6(spirv.vertex_words());
    assert_spirv_1_6(spirv.fragment_words());
    let unlit_draw = &plan.draws()[2];
    let unlit_permutation = M2ShaderPermutation::resolve(
        unlit_draw,
        M2LocalLightCount::Four,
        M2ShadowPermutation::Mode3,
        M2ShadowFiltering::Pcf,
    );
    assert_eq!(unlit_permutation.vertex_index(), 70);
    assert_eq!(unlit_permutation.pixel_index(), 15);
    assert_eq!(
        M2ShadowPermutation::from_stock_quality(0),
        Some(M2ShadowPermutation::Disabled)
    );
    assert_eq!(
        M2ShadowPermutation::from_stock_quality(1),
        Some(M2ShadowPermutation::Mode1)
    );
    assert_eq!(
        M2ShadowPermutation::from_stock_quality(2),
        Some(M2ShadowPermutation::Mode1)
    );
    assert_eq!(
        M2ShadowPermutation::from_stock_quality(3),
        Some(M2ShadowPermutation::Mode2)
    );
    assert_eq!(
        M2ShadowPermutation::from_stock_quality(4),
        Some(M2ShadowPermutation::Mode2)
    );
    assert_eq!(
        M2ShadowPermutation::from_stock_quality(5),
        Some(M2ShadowPermutation::Mode3)
    );
    assert_eq!(
        M2ShadowPermutation::from_stock_quality(6),
        Some(M2ShadowPermutation::Mode3)
    );
    assert_eq!(M2ShadowPermutation::from_stock_quality(7), None);
    for shadow in [
        M2ShadowPermutation::Disabled,
        M2ShadowPermutation::Mode1,
        M2ShadowPermutation::Mode2,
        M2ShadowPermutation::Mode3,
    ] {
        for filtering in [M2ShadowFiltering::Direct, M2ShadowFiltering::Pcf] {
            let permutation = M2ShaderPermutation::resolve(
                unlit_draw,
                M2LocalLightCount::Four,
                shadow,
                filtering,
            );
            let program = compiler.compile(fallback, permutation)?;
            assert_spirv_1_6(program.vertex_words());
            assert_spirv_1_6(program.fragment_words());
        }
    }
    assert!(matches!(
        M2MeshPlan::prepare(&model, 1),
        Err(M2MeshPlanError::MissingProfile {
            profile_index: 1,
            ..
        })
    ));

    let _sdl_test = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let mut window_builder = video.window("Solarity M2 upload test", 64, 64);
    window_builder.vulkan().hidden();
    let window = window_builder.build()?;
    let extensions = window.vulkan_instance_extensions()?;
    let bootstrap = VulkanBootstrap::start(&extensions)?;
    // SAFETY: The bootstrap enabled the exact extensions reported by this
    // window, and the window remains live through renderer destruction.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    // SAFETY: SDL created the surface from this exact live instance and
    // transfers its sole ownership into the renderer immediately.
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (64, 64), 0) }?;
    let handle = renderer.upload_m2_mesh(&plan)?;
    assert_eq!(renderer.upload_m2_mesh(&plan)?, handle);
    let info = renderer
        .m2_mesh_info(handle)
        .ok_or("uploaded M2 resource is absent")?;
    assert_eq!(info.path(), &path);
    assert_eq!(info.profile_index(), 0);
    assert_eq!(info.vertex_count(), 3);
    assert_eq!(info.index_count(), 3);
    assert_eq!(info.vertex_byte_count(), 156);
    assert_eq!(info.index_byte_count(), 6);
    assert_eq!(info.max_bone_index(), Some(2));
    let pipeline = renderer.prepare_m2_pipeline(specialized, lit_permutation)?;
    assert_eq!(
        renderer.prepare_m2_pipeline(specialized, lit_permutation)?,
        pipeline
    );
    let pipeline_info = renderer
        .m2_pipeline_info(pipeline)
        .ok_or("uploaded M2 pipeline handle did not resolve")?;
    assert_eq!(pipeline_info.vertex_shader(), M2VertexShader::DiffuseT1Env);
    assert_eq!(
        pipeline_info.pixel_shader(),
        M2PixelShader::OpaqueMod2xNoAlphaAlpha
    );
    assert_eq!(pipeline_info.texture_count(), 2);
    assert_eq!(pipeline_info.permutation(), lit_permutation);
    let fade_pipeline =
        renderer.prepare_m2_pipeline(specialized.with_runtime_alpha_fade(), lit_permutation)?;
    let fade_pipeline_info = renderer
        .m2_pipeline_info(fade_pipeline)
        .ok_or("runtime-alpha M2 pipeline handle did not resolve")?;
    assert!(fade_pipeline_info.material().blend_enabled());
    assert!(!fade_pipeline_info.material().depth_write_enabled());
    let texture_handle = renderer.upload_blp_texture(&texture_source, BlpColorSpace::Srgb)?;
    assert_eq!(
        renderer.upload_blp_texture(&texture_source, BlpColorSpace::Srgb)?,
        texture_handle
    );
    let texture_info = renderer
        .blp_texture_info(texture_handle)
        .ok_or("uploaded BLP texture handle did not resolve")?;
    assert_eq!(texture_info.path(), &texture_path);
    assert_eq!(texture_info.color_space(), BlpColorSpace::Srgb);
    assert_eq!(texture_info.storage(), BlpTextureStorage::Rgba8);
    assert_eq!(texture_info.extent(), (2, 2));
    assert_eq!(texture_info.mip_count(), 2);
    assert_eq!(texture_info.upload_byte_count(), 20);
    let wrap_u_sampler = renderer.prepare_m2_sampler(&model.textures()[0])?;
    assert_eq!(
        renderer.prepare_m2_sampler(&model.textures()[0])?,
        wrap_u_sampler
    );
    let wrap_u_info = renderer
        .m2_sampler_info(wrap_u_sampler)
        .ok_or("M2 sampler handle did not resolve")?;
    assert_eq!(wrap_u_info.address_u(), M2TextureAddressMode::Repeat);
    assert_eq!(wrap_u_info.address_v(), M2TextureAddressMode::Clamp);
    let clamped_sampler = renderer.prepare_m2_sampler(&model.textures()[1])?;
    assert_ne!(wrap_u_sampler, clamped_sampler);
    let clamped_info = renderer
        .m2_sampler_info(clamped_sampler)
        .ok_or("clamped M2 sampler handle did not resolve")?;
    assert_eq!(clamped_info.address_u(), M2TextureAddressMode::Clamp);
    assert_eq!(clamped_info.address_v(), M2TextureAddressMode::Clamp);
    let one_stage = M2TextureSet::One(M2SampledTexture::new(texture_handle, wrap_u_sampler));
    let two_stage = M2TextureSet::Two([
        M2SampledTexture::new(texture_handle, wrap_u_sampler),
        M2SampledTexture::new(texture_handle, clamped_sampler),
    ]);
    let texture_sets = renderer.prepare_m2_texture_sets(&[two_stage, one_stage, two_stage])?;
    assert_eq!(texture_sets.len(), 3);
    assert_eq!(texture_sets[0], texture_sets[2]);
    assert_ne!(texture_sets[0], texture_sets[1]);
    assert_eq!(
        renderer
            .m2_texture_set_info(texture_sets[0])
            .ok_or("two-stage M2 texture set did not resolve")?
            .stage_count(),
        2
    );
    assert_eq!(
        renderer
            .m2_texture_set_info(texture_sets[1])
            .ok_or("one-stage M2 texture set did not resolve")?
            .stage_count(),
        1
    );
    assert_eq!(
        renderer.prepare_m2_texture_sets(&[two_stage, one_stage])?,
        texture_sets[..2]
    );
    let particle_emitter = model
        .animations()
        .particles()
        .first()
        .ok_or("particle emitter is absent")?;
    let particle_pipeline = renderer
        .prepare_m2_particle_pipeline(particle_emitter.blending_type(), particle_emitter.flags())?;
    assert_eq!(
        renderer.prepare_m2_particle_pipeline(
            particle_emitter.blending_type(),
            particle_emitter.flags(),
        )?,
        particle_pipeline
    );
    assert_eq!(
        renderer
            .m2_particle_pipeline_info(particle_pipeline)
            .ok_or("particle pipeline did not resolve")?
            .material(),
        M2MaterialState::from_particle(particle_emitter.blending_type(), particle_emitter.flags())
    );
    let particle_pose = M2ParticlePose::sample(
        model.animations(),
        particle_emitter,
        M2AnimationClock::new(0, 500.0, 0.0),
    )?;
    let particle_state = M2ParticleState::new(
        0.5,
        Vec3::new(10.0, 20.0, 30.0),
        Vec3::new(0.0, 2.0, 0.0),
        0x2483,
    )?;
    let particle_camera = WorldCamera::stock(Vec3::ZERO, Vec3::X, Vec3::Z, 100.0).frame(1.0)?;
    let particle_mesh = M2ParticleMeshPlan::prepare(
        particle_emitter,
        particle_pose,
        &[particle_state],
        particle_camera,
        1.0,
    )?;
    let particle_draw = renderer.prepare_m2_particle_draw(
        particle_pipeline,
        texture_sets[1],
        particle_emitter.blending_type(),
        particle_emitter.flags(),
        0,
        0,
        &particle_mesh,
    )?;
    assert_eq!(particle_draw.vertex_offset(), 0);
    assert_eq!(particle_draw.first_index(), 0);
    assert_eq!(
        particle_draw.index_count() as usize,
        particle_mesh.indices().len()
    );
    assert!(matches!(
        renderer.prepare_m2_particle_draw(
            particle_pipeline,
            texture_sets[0],
            particle_emitter.blending_type(),
            particle_emitter.flags(),
            0,
            0,
            &particle_mesh,
        ),
        Err(VulkanError::M2ParticleDrawTextureSetMismatch)
    ));
    let ribbon_emitter = model
        .animations()
        .ribbons()
        .first()
        .ok_or("ribbon emitter is absent")?;
    let ribbon_material = model.materials()[usize::from(ribbon_emitter.material_indices()[0])];
    let ribbon_pipeline = renderer.prepare_m2_ribbon_pipeline(ribbon_material)?;
    assert_eq!(
        renderer.prepare_m2_ribbon_pipeline(ribbon_material)?,
        ribbon_pipeline
    );
    assert_eq!(
        renderer
            .m2_ribbon_pipeline_info(ribbon_pipeline)
            .ok_or("ribbon pipeline did not resolve")?
            .material(),
        M2MaterialState::from_material(ribbon_material)
    );
    let ribbon_pose = M2RibbonPose::sample(
        model.animations(),
        ribbon_emitter,
        M2AnimationClock::new(0, 500.0, 0.0),
    )?;
    let mut ribbon_trail = M2RibbonTrail::new(ribbon_emitter)?;
    ribbon_trail.advance(
        0.05,
        M2RibbonControlPoint::new(Vec3::ZERO, Vec3::Y, Vec3::X),
        ribbon_pose,
    )?;
    let ribbon_mesh = M2RibbonMeshPlan::prepare(ribbon_emitter, &ribbon_trail)?;
    let ribbon_draw = renderer.prepare_m2_ribbon_draw(
        ribbon_pipeline,
        texture_sets[1],
        ribbon_material,
        0,
        &ribbon_mesh,
    )?;
    assert_eq!(ribbon_draw.first_vertex(), 0);
    assert_eq!(
        ribbon_draw.vertex_count() as usize,
        ribbon_mesh.vertices().len()
    );
    let prepared_draw = renderer.prepare_m2_draw(
        handle,
        pipeline,
        texture_sets[0],
        &plan,
        0,
        false,
        material_uniform,
        64,
        0x20,
    )?;
    assert_eq!(prepared_draw.mesh(), handle);
    assert_eq!(prepared_draw.pipeline(), pipeline);
    assert_eq!(prepared_draw.texture_set(), texture_sets[0]);
    assert_eq!(prepared_draw.first_index(), draw.first_index());
    assert_eq!(prepared_draw.index_count(), draw.index_count());
    assert_eq!(prepared_draw.material(), material_uniform);
    assert_eq!(prepared_draw.push_constants(), push_constants);
    assert_eq!(prepared_draw.required_bone_transforms(), 67);
    let faded_draw = renderer.prepare_m2_draw(
        handle,
        fade_pipeline,
        texture_sets[0],
        &plan,
        0,
        true,
        material_uniform,
        64,
        0x20,
    )?;
    assert_eq!(faded_draw.pipeline(), fade_pipeline);
    assert!(matches!(
        renderer.prepare_m2_draw(
            handle,
            fade_pipeline,
            texture_sets[0],
            &plan,
            0,
            false,
            material_uniform,
            64,
            0x20,
        ),
        Err(VulkanError::M2DrawPipelineMismatch)
    ));
    let bone_transforms = vec![Mat4::IDENTITY; prepared_draw.required_bone_transforms()];
    for _ in 0..=renderer.report().swapchain_image_count() {
        let frame = renderer.present_m2(scene_uniform, &bone_transforms, &[prepared_draw])?;
        assert_eq!(frame.draw_count(), 1);
        assert_eq!(
            frame.bone_transform_count(),
            prepared_draw.required_bone_transforms()
        );
    }
    let world_scene = WorldFrameScene::new(
        TerrainSceneUniform::new(Mat4::IDENTITY, Vec3::ZERO, Vec3::ZERO, Vec3::Z),
        WorldModelSceneUniform::new(
            Mat4::IDENTITY,
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::Z,
            Vec4::ZERO,
        ),
        scene_uniform,
    );
    let ribbon_frame = renderer.present_world_frame(
        world_scene,
        &[],
        &[],
        &[],
        &[],
        particle_mesh.vertices(),
        particle_mesh.indices(),
        &[particle_draw],
        ribbon_mesh.vertices(),
        &[ribbon_draw],
    )?;
    assert_eq!(ribbon_frame.ribbon_draw_count(), 1);
    assert_eq!(ribbon_frame.particle_draw_count(), 1);
    assert_eq!(
        ribbon_frame.particle_vertex_count(),
        particle_mesh.vertices().len()
    );
    assert_eq!(
        ribbon_frame.particle_index_count(),
        particle_mesh.indices().len()
    );
    assert_eq!(
        ribbon_frame.ribbon_vertex_count(),
        ribbon_mesh.vertices().len()
    );
    let shadow_pipeline = renderer.prepare_m2_pipeline(fallback, unlit_permutation)?;
    let shadow_draw = renderer.prepare_m2_draw(
        handle,
        shadow_pipeline,
        texture_sets[0],
        &plan,
        2,
        false,
        material_uniform,
        0,
        0,
    )?;
    assert!(matches!(
        renderer.present_m2(scene_uniform, &bone_transforms, &[shadow_draw]),
        Err(VulkanError::M2ShadowResourcesUnavailable)
    ));
    renderer.shutdown()?;
    Ok(())
}

/// Spherical billboard bones replace view rotation without moving the pivot.
#[test]
fn m2_bone_pose_applies_stock_view_space_billboard() -> Result<(), Box<dyn Error>> {
    let mut bytes = render_m2_bytes("Billboard.blp", 1)?;
    let bone_offset = usize::try_from(u32::from_le_bytes(bytes[0x30..0x34].try_into()?))?;
    bytes[bone_offset + 4..bone_offset + 8].copy_from_slice(&0x8_u32.to_le_bytes());
    let path = AssetPath::new("Creature\\Solarity\\Billboard.m2")?;
    let skin = render_skin_bytes()?;
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Creature\\Solarity\\Billboard.m2",
            bytes: &bytes,
        },
        FixtureFile {
            path: "Creature\\Solarity\\Billboard00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let model = DecodedM2Model::load(&mut store, &path)?;
    assert_eq!(
        M2BonePose::compose(model.animations(), M2AnimationClock::new(0, 500.0, 0.0)),
        Err(solarity_rendering::M2BonePoseError::BillboardViewRequired { bone: 0 })
    );

    let camera = solarity_rendering::WorldCamera::stock(
        Vec3::new(0.0, -10.0, 0.0),
        Vec3::ZERO,
        Vec3::Z,
        100.0,
    )
    .frame(1.0)?;
    let pose = M2BonePose::compose_with_model_view(
        model.animations(),
        M2AnimationClock::new(0, 500.0, 0.0),
        camera.view(),
    )?;
    let view_bone = camera.view() * pose.transforms()[0];
    assert_eq!(view_bone.x_axis.truncate(), Vec3::new(0.0, 0.0, -1.0));
    assert_eq!(view_bone.y_axis.truncate(), Vec3::X);
    assert_eq!(view_bone.z_axis.truncate(), Vec3::Y);
    Ok(())
}

/// Confirms shaderc emitted the pinned SPIR-V binary header, not its default.
fn assert_spirv_1_6(words: &[u32]) {
    assert!(words.len() >= 5);
    assert_eq!(words[0], 0x0723_0203);
    assert_eq!(words[1], 0x0001_0600);
}

/// Helmet masks and death-knight eyes follow the exact build-12340 slot order.
#[test]
fn character_geosets_apply_helmet_masks_before_eye_glow() -> Result<(), Box<dyn Error>> {
    let mut characters = character_tables(0, 0);
    characters.hair_geosets = create_wdbc(1, 6, &[90, 1, 0, 4, 12, 1], b"\0");
    characters.facial_hair = create_wdbc(1, 8, &[91, 1, 0, 6, 4, 5, 6, 7], b"\0");
    let definitions = create_wdbc(1, 8, &[70_001, 4, 0, u32::MAX, 1, 71_001, 1, 0], b"\0");
    let mut head_display = item_display_fields(71_001, [0, 0, 0], [0; 8]);
    head_display[13] = 72_001;
    head_display[14] = 72_001;
    let displays = create_wdbc(1, 25, &head_display, b"\0");
    let race_one_bit = 1_u32 << 1;
    let helmet_visibility = create_wdbc(
        1,
        8,
        &[
            72_001,
            race_one_bit,
            race_one_bit,
            race_one_bit,
            race_one_bit,
            race_one_bit,
            race_one_bit,
            race_one_bit,
        ],
        b"\0",
    );
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "DBFilesClient\\CharSections.dbc",
            bytes: &characters.sections,
        },
        FixtureFile {
            path: "DBFilesClient\\CharHairGeosets.dbc",
            bytes: &characters.hair_geosets,
        },
        FixtureFile {
            path: "DBFilesClient\\CharacterFacialHairStyles.dbc",
            bytes: &characters.facial_hair,
        },
        FixtureFile {
            path: "DBFilesClient\\Item.dbc",
            bytes: &definitions,
        },
        FixtureFile {
            path: "DBFilesClient\\ItemDisplayInfo.dbc",
            bytes: &displays,
        },
        FixtureFile {
            path: "DBFilesClient\\HelmetGeosetVisData.dbc",
            bytes: &helmet_visibility,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let character_catalog = CharacterAppearanceCatalog::load(&mut store)?;
    let definitions = ItemDefinitionCatalog::load(&mut store)?;
    let displays = ItemDisplayCatalog::load(&mut store)?;
    let helmet_visibility = HelmetGeosetVisibilityCatalog::load(&mut store)?;
    let appearance =
        character_catalog.resolve_player(1, 0, CharacterCustomization::new(2, 3, 4, 5, 6))?;
    let head_definition = definitions.item(70_001).ok_or("head item is absent")?;
    let head_display = displays.display(71_001).ok_or("head display is absent")?;

    let plan = CharacterGeosetPlan::equipped(
        &appearance,
        CharacterGeosetContext::new(6, CharacterTabardMode::Equipment),
        &helmet_visibility,
        [CharacterEquipmentItem::new(
            PlayerEquipmentSlot::Head,
            head_definition,
            head_display,
        )],
    )?;

    for hidden in [12, 106, 205, 304, 702, 1606, 1707] {
        assert!(!plan.visible_geosets().contains(&hidden));
    }
    for visible in [1, 101, 201, 301, 701, 1601, 1703] {
        assert!(plan.visible_geosets().contains(&visible));
    }
    Ok(())
}

/// Equipped body geometry preserves stock priority, tabard, and robe branches.
#[test]
fn character_geosets_preserve_equipment_branching() -> Result<(), Box<dyn Error>> {
    let characters = character_tables(0, 0);
    let definitions = create_wdbc(1, 8, &[80_001, 4, 0, u32::MAX, 1, 81_001, 5, 0], b"\0");
    let mut strings = vec![0];
    let outer_arm = append_string(&mut strings, "OuterArm");
    let rows = [
        item_display_fields(81_001, [2, 3, 0], [0, outer_arm, 0, 0, 0, 0, 0, 0]),
        item_display_fields(81_002, [4, 5, 0], [0, outer_arm, 0, 0, 0, 0, 0, 0]),
        item_display_fields(81_003, [2, 3, 0], [0; 8]),
        item_display_fields(81_004, [4, 0, 0], [0; 8]),
        item_display_fields(81_005, [2, 0, 0], [0, outer_arm, 0, 0, 0, 0, 0, 0]),
        item_display_fields(81_006, [1, 0, 0], [0; 8]),
        item_display_fields(81_007, [2, 0, 0], [0; 8]),
        item_display_fields(81_008, [3, 0, 0], [0; 8]),
        item_display_fields(81_009, [4, 5, 6], [0, outer_arm, 0, 0, 0, 0, 0, 0]),
    ];
    let display_fields = rows.into_iter().flatten().collect::<Vec<_>>();
    let displays = create_wdbc(9, 25, &display_fields, &strings);
    let helmet_visibility = create_wdbc(0, 8, &[], b"\0");
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "DBFilesClient\\CharSections.dbc",
            bytes: &characters.sections,
        },
        FixtureFile {
            path: "DBFilesClient\\CharHairGeosets.dbc",
            bytes: &characters.hair_geosets,
        },
        FixtureFile {
            path: "DBFilesClient\\CharacterFacialHairStyles.dbc",
            bytes: &characters.facial_hair,
        },
        FixtureFile {
            path: "DBFilesClient\\Item.dbc",
            bytes: &definitions,
        },
        FixtureFile {
            path: "DBFilesClient\\ItemDisplayInfo.dbc",
            bytes: &displays,
        },
        FixtureFile {
            path: "DBFilesClient\\HelmetGeosetVisData.dbc",
            bytes: &helmet_visibility,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let character_catalog = CharacterAppearanceCatalog::load(&mut store)?;
    let definitions = ItemDefinitionCatalog::load(&mut store)?;
    let displays = ItemDisplayCatalog::load(&mut store)?;
    let helmet_visibility = HelmetGeosetVisibilityCatalog::load(&mut store)?;
    let appearance =
        character_catalog.resolve_player(1, 0, CharacterCustomization::new(2, 3, 4, 5, 6))?;
    let definition = definitions.item(80_001).ok_or("item is absent")?;
    let item = |slot, display_id| {
        displays
            .display(display_id)
            .map(|display| CharacterEquipmentItem::new(slot, definition, display))
            .ok_or("item display is absent")
    };
    let equipment = [
        item(PlayerEquipmentSlot::Shirt, 81_001)?,
        item(PlayerEquipmentSlot::Chest, 81_002)?,
        item(PlayerEquipmentSlot::Legs, 81_003)?,
        item(PlayerEquipmentSlot::Feet, 81_004)?,
        item(PlayerEquipmentSlot::Hands, 81_005)?,
        item(PlayerEquipmentSlot::Tabard, 81_006)?,
        item(PlayerEquipmentSlot::Back, 81_007)?,
        item(PlayerEquipmentSlot::Waist, 81_008)?,
    ];
    let plan = CharacterGeosetPlan::equipped(
        &appearance,
        CharacterGeosetContext::new(1, CharacterTabardMode::CustomGuild),
        &helmet_visibility,
        equipment,
    )?;
    for visible in [403, 505, 904, 1201, 1202, 1503, 1804] {
        assert!(plan.visible_geosets().contains(&visible));
    }
    for hidden in [401, 501, 803, 1004, 1103, 1501, 1801] {
        assert!(!plan.visible_geosets().contains(&hidden));
    }

    let robe_plan = CharacterGeosetPlan::equipped(
        &appearance,
        CharacterGeosetContext::new(1, CharacterTabardMode::CustomGuild),
        &helmet_visibility,
        [
            item(PlayerEquipmentSlot::Shirt, 81_001)?,
            item(PlayerEquipmentSlot::Chest, 81_009)?,
            item(PlayerEquipmentSlot::Legs, 81_003)?,
            item(PlayerEquipmentSlot::Tabard, 81_006)?,
            item(PlayerEquipmentSlot::Back, 81_007)?,
            item(PlayerEquipmentSlot::Waist, 81_008)?,
        ],
    )?;
    for visible in [805, 1201, 1307, 1503, 1804] {
        assert!(robe_plan.visible_geosets().contains(&visible));
    }
    for hidden in [501, 902, 1101, 1202, 1301] {
        assert!(!robe_plan.visible_geosets().contains(&hidden));
    }
    Ok(())
}

/// Head and shoulder children use stock race suffixes, channels, and links.
#[test]
fn armor_attachment_plan_preserves_stock_component_models() -> Result<(), Box<dyn Error>> {
    let definitions = create_wdbc(
        2,
        8,
        &[
            90_001,
            4,
            0,
            u32::MAX,
            1,
            91_001,
            1,
            0,
            90_002,
            4,
            0,
            u32::MAX,
            1,
            91_002,
            3,
            0,
        ],
        b"\0",
    );
    let mut strings = vec![0];
    let helmet_model = append_string(&mut strings, "Helm_Test.mdx");
    let helmet_texture = append_string(&mut strings, "HelmTexture");
    let shoulder_zero_model = append_string(&mut strings, "ShoulderZero.mdx");
    let shoulder_one_model = append_string(&mut strings, "ShoulderOne.mdx");
    let shoulder_zero_texture = append_string(&mut strings, "ShoulderZeroBlue");
    let shoulder_one_texture = append_string(&mut strings, "ShoulderOneBlue");
    let mut helmet = item_display_model_fields(91_001, helmet_model, helmet_texture, 701, 801);
    helmet[2] = 0;
    let mut shoulder =
        item_display_model_fields(91_002, shoulder_zero_model, shoulder_zero_texture, 702, 802);
    shoulder[2] = shoulder_one_model;
    shoulder[4] = shoulder_one_texture;
    let displays = create_wdbc(
        2,
        25,
        &helmet.into_iter().chain(shoulder).collect::<Vec<_>>(),
        &strings,
    );
    let mut race_strings = vec![0];
    let client_prefix = append_string(&mut race_strings, "Hu");
    let client_file_string = append_string(&mut race_strings, "Human");
    let mut race_fields = [0_u32; 69];
    race_fields[0] = 1;
    race_fields[6] = client_prefix;
    race_fields[11] = client_file_string;
    let races = create_wdbc(1, 69, &race_fields, &race_strings);
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "DBFilesClient\\Item.dbc",
            bytes: &definitions,
        },
        FixtureFile {
            path: "DBFilesClient\\ItemDisplayInfo.dbc",
            bytes: &displays,
        },
        FixtureFile {
            path: "DBFilesClient\\ChrRaces.dbc",
            bytes: &races,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let definitions = ItemDefinitionCatalog::load(&mut store)?;
    let displays = ItemDisplayCatalog::load(&mut store)?;
    let races = CharacterRaceCatalog::load(&mut store)?;
    let race = races.race(1).ok_or("character race is absent")?;
    let head = CharacterEquipmentItem::new(
        PlayerEquipmentSlot::Head,
        definitions.item(90_001).ok_or("head item is absent")?,
        displays.display(91_001).ok_or("head display is absent")?,
    );
    let shoulders = CharacterEquipmentItem::new(
        PlayerEquipmentSlot::Shoulders,
        definitions.item(90_002).ok_or("shoulder item is absent")?,
        displays
            .display(91_002)
            .ok_or("shoulder display is absent")?,
    );

    let plan = CharacterAttachmentPlan::equipped_items(
        [head, shoulders],
        race,
        1,
        CharacterWeaponState::new(CharacterWeaponPose::Ready, CharacterRangedHand::Left),
    )?;
    let values = plan
        .attachments()
        .iter()
        .map(|attachment| {
            (
                attachment.point(),
                attachment.model().as_str(),
                attachment.texture().as_str(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        values,
        [
            (
                CharacterAttachmentPoint::Helmet,
                "ITEM\\OBJECTCOMPONENTS\\HEAD\\HELM_TEST_HUF.MDX",
                "ITEM\\OBJECTCOMPONENTS\\HEAD\\HELMTEXTURE.BLP",
            ),
            (
                CharacterAttachmentPoint::ShoulderRight,
                "ITEM\\OBJECTCOMPONENTS\\SHOULDER\\SHOULDERZERO.MDX",
                "ITEM\\OBJECTCOMPONENTS\\SHOULDER\\SHOULDERZEROBLUE.BLP",
            ),
            (
                CharacterAttachmentPoint::ShoulderLeft,
                "ITEM\\OBJECTCOMPONENTS\\SHOULDER\\SHOULDERONE.MDX",
                "ITEM\\OBJECTCOMPONENTS\\SHOULDER\\SHOULDERONEBLUE.BLP",
            ),
        ]
    );
    Ok(())
}

/// Held items use stock folders, channel zero, and exact hand/sheath links.
#[test]
fn held_item_plan_preserves_stock_attachment_behavior() -> Result<(), Box<dyn Error>> {
    let items = held_equipment_tables();
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "DBFilesClient\\Item.dbc",
            bytes: &items.definitions,
        },
        FixtureFile {
            path: "DBFilesClient\\ItemDisplayInfo.dbc",
            bytes: &items.displays,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let definitions = ItemDefinitionCatalog::load(&mut store)?;
    let displays = ItemDisplayCatalog::load(&mut store)?;
    let chest_definition = definitions.item(60_000).ok_or("chest item is absent")?;
    let main_definition = definitions.item(60_001).ok_or("main-hand item is absent")?;
    let off_definition = definitions.item(60_002).ok_or("off-hand item is absent")?;
    let ranged_definition = definitions.item(60_003).ok_or("ranged item is absent")?;
    let chest_display = displays.display(61_000).ok_or("chest display is absent")?;
    let main_display = displays
        .display(61_001)
        .ok_or("main-hand display is absent")?;
    let off_display = displays
        .display(61_002)
        .ok_or("off-hand display is absent")?;
    let ranged_display = displays.display(61_003).ok_or("ranged display is absent")?;
    let equipment = [
        CharacterEquipmentItem::new(PlayerEquipmentSlot::Chest, chest_definition, chest_display),
        CharacterEquipmentItem::new(PlayerEquipmentSlot::MainHand, main_definition, main_display),
        CharacterEquipmentItem::new(PlayerEquipmentSlot::OffHand, off_definition, off_display),
        CharacterEquipmentItem::new(
            PlayerEquipmentSlot::Ranged,
            ranged_definition,
            ranged_display,
        ),
    ];

    let ready = CharacterAttachmentPlan::held_items(
        equipment,
        CharacterWeaponState::new(CharacterWeaponPose::Ready, CharacterRangedHand::Left),
    )?;
    let ready_values = ready
        .attachments()
        .iter()
        .map(|attachment| {
            (
                attachment.slot(),
                attachment.point(),
                attachment.model().as_str(),
                attachment.texture().as_str(),
                attachment.item_visual_id(),
                attachment.particle_color_id(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        ready_values,
        [
            (
                PlayerEquipmentSlot::MainHand,
                CharacterAttachmentPoint::HandRight,
                "ITEM\\OBJECTCOMPONENTS\\WEAPON\\SWORD.MDX",
                "ITEM\\OBJECTCOMPONENTS\\WEAPON\\SWORDRED.BLP",
                701,
                801,
            ),
            (
                PlayerEquipmentSlot::OffHand,
                CharacterAttachmentPoint::Shield,
                "ITEM\\OBJECTCOMPONENTS\\SHIELD\\SHIELD.MDX",
                "ITEM\\OBJECTCOMPONENTS\\SHIELD\\SHIELDBLUE.BLP",
                702,
                802,
            ),
            (
                PlayerEquipmentSlot::Ranged,
                CharacterAttachmentPoint::HandLeft,
                "ITEM\\OBJECTCOMPONENTS\\WEAPON\\BOW.MDX",
                "ITEM\\OBJECTCOMPONENTS\\WEAPON\\BOWGREEN.BLP",
                703,
                803,
            ),
        ]
    );

    let sheathed = CharacterAttachmentPlan::held_items(
        equipment,
        CharacterWeaponState::new(CharacterWeaponPose::Sheathed, CharacterRangedHand::Left),
    )?;
    assert_eq!(
        sheathed
            .attachments()
            .iter()
            .map(|attachment| attachment.point())
            .collect::<Vec<_>>(),
        [
            CharacterAttachmentPoint::SheathMainHand,
            CharacterAttachmentPoint::SheathShield,
            CharacterAttachmentPoint::LargeWeaponRight,
        ]
    );
    Ok(())
}

/// Texture table bytes needed by one exact appearance lookup.
struct CharacterTables {
    sections: Vec<u8>,
    hair_geosets: Vec<u8>,
    facial_hair: Vec<u8>,
}

/// Builds character DBC rows with a caller-selected skin flag word.
fn character_tables(skin_flags: u32, gender_id: u32) -> CharacterTables {
    let mut strings = vec![0];
    let skin = append_string(&mut strings, "Character\\Human\\Male\\Skin.blp");
    let extra = append_string(&mut strings, "Character\\Human\\Male\\SkinExtra.blp");
    let face_lower = append_string(&mut strings, "Character\\Human\\Male\\FaceLower.blp");
    let face_upper = append_string(&mut strings, "Character\\Human\\Male\\FaceUpper.blp");
    let facial_lower = append_string(&mut strings, "Character\\Human\\Male\\FacialLower.blp");
    let facial_upper = append_string(&mut strings, "Character\\Human\\Male\\FacialUpper.blp");
    let hair = append_string(&mut strings, "Character\\Human\\Male\\Hair.blp");
    let hair_lower = append_string(&mut strings, "Character\\Human\\Male\\HairLower.blp");
    let hair_upper = append_string(&mut strings, "Character\\Human\\Male\\HairUpper.blp");
    let underwear_lower = append_string(&mut strings, "Character\\Human\\Male\\UnderwearLower.blp");
    let underwear_upper = append_string(&mut strings, "Character\\Human\\Male\\UnderwearUpper.blp");
    let fields = [
        10,
        1,
        gender_id,
        0,
        skin,
        extra,
        0,
        skin_flags,
        0,
        2,
        11,
        1,
        gender_id,
        1,
        face_lower,
        face_upper,
        0,
        0,
        3,
        2,
        12,
        1,
        gender_id,
        2,
        facial_lower,
        facial_upper,
        0,
        0,
        6,
        5,
        13,
        1,
        gender_id,
        3,
        hair,
        hair_lower,
        hair_upper,
        0,
        4,
        5,
        14,
        1,
        gender_id,
        4,
        underwear_lower,
        underwear_upper,
        0,
        0,
        0,
        2,
    ];
    CharacterTables {
        sections: create_wdbc(5, 10, &fields, &strings),
        hair_geosets: create_wdbc(0, 6, &[], b"\0"),
        facial_hair: create_wdbc(0, 8, &[], b"\0"),
    }
}

/// Exact item and item-display rows used by equipped texture planning.
struct EquipmentTables {
    definitions: Vec<u8>,
    displays: Vec<u8>,
}

/// Builds shirt, chest, and glove rows with overlapping component regions.
fn equipment_tables() -> EquipmentTables {
    let definitions = create_wdbc(
        3,
        8,
        &[
            50_001,
            4,
            0,
            u32::MAX,
            1,
            55_001,
            4,
            0,
            50_002,
            4,
            0,
            u32::MAX,
            1,
            55_002,
            5,
            0,
            50_003,
            4,
            0,
            u32::MAX,
            1,
            55_003,
            10,
            0,
        ],
        b"\0",
    );

    let mut strings = vec![0];
    let shirt_au = append_string(&mut strings, "ShirtAU");
    let shirt_al = append_string(&mut strings, "ShirtAL");
    let shirt_tu = append_string(&mut strings, "ShirtTU");
    let chest_au = append_string(&mut strings, "ChestAU");
    let chest_al = append_string(&mut strings, "ChestAL");
    let chest_tu = append_string(&mut strings, "ChestTU");
    let glove_al = append_string(&mut strings, "GloveAL");
    let glove_ha = append_string(&mut strings, "GloveHA");
    let mut display_fields = Vec::with_capacity(75);
    display_fields.extend(item_display_fields(
        55_001,
        [0, 0, 0],
        [shirt_au, shirt_al, 0, shirt_tu, 0, 0, 0, 0],
    ));
    display_fields.extend(item_display_fields(
        55_002,
        [1, 0, 0],
        [chest_au, chest_al, 0, chest_tu, 0, 0, 0, 0],
    ));
    display_fields.extend(item_display_fields(
        55_003,
        [1, 0, 0],
        [0, glove_al, glove_ha, 0, 0, 0, 0, 0],
    ));
    EquipmentTables {
        definitions,
        displays: create_wdbc(3, 25, &display_fields, &strings),
    }
}

/// Builds held items plus a dirty non-stock armor attachment candidate.
fn held_equipment_tables() -> EquipmentTables {
    let definitions = create_wdbc(
        4,
        8,
        &[
            60_000,
            4,
            0,
            u32::MAX,
            1,
            61_000,
            5,
            0,
            60_001,
            2,
            7,
            u32::MAX,
            1,
            61_001,
            13,
            1,
            60_002,
            4,
            6,
            u32::MAX,
            1,
            61_002,
            14,
            4,
            60_003,
            2,
            2,
            u32::MAX,
            1,
            61_003,
            15,
            2,
        ],
        b"\0",
    );
    let mut strings = vec![0];
    let dirty = append_string(&mut strings, "DirtyChest.mdx");
    let sword = append_string(&mut strings, "Sword.mdx");
    let sword_red = append_string(&mut strings, "SwordRed");
    let shield = append_string(&mut strings, "Shield.mdx");
    let shield_blue = append_string(&mut strings, "ShieldBlue");
    let bow = append_string(&mut strings, "Bow.mdx");
    let bow_green = append_string(&mut strings, "BowGreen");
    let mut fields = Vec::with_capacity(100);
    fields.extend(item_display_model_fields(61_000, dirty, 0, 700, 800));
    fields.extend(item_display_model_fields(
        61_001, sword, sword_red, 701, 801,
    ));
    fields.extend(item_display_model_fields(
        61_002,
        shield,
        shield_blue,
        702,
        802,
    ));
    fields.extend(item_display_model_fields(61_003, bow, bow_green, 703, 803));
    EquipmentTables {
        definitions,
        displays: create_wdbc(4, 25, &fields, &strings),
    }
}

/// Produces one complete 25-field item-display row.
fn item_display_fields(id: u32, geosets: [u32; 3], components: [u32; 8]) -> [u32; 25] {
    [
        id,
        0,
        0,
        0,
        0,
        0,
        0,
        geosets[0],
        geosets[1],
        geosets[2],
        0,
        0,
        0,
        0,
        0,
        components[0],
        components[1],
        components[2],
        components[3],
        components[4],
        components[5],
        components[6],
        components[7],
        0,
        0,
    ]
}

/// Produces one model-bearing item display row for attachment planning.
fn item_display_model_fields(
    id: u32,
    model: u32,
    texture: u32,
    item_visual_id: u32,
    particle_color_id: u32,
) -> [u32; 25] {
    [
        id,
        model,
        0,
        texture,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        item_visual_id,
        particle_color_id,
    ]
}

/// Generates one fixed-layout WDBC table.
fn create_wdbc(
    record_count: u32,
    field_count: u32,
    fields: &[u32],
    string_block: &[u8],
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(20 + fields.len() * 4 + string_block.len());
    bytes.extend_from_slice(b"WDBC");
    bytes.extend_from_slice(&record_count.to_le_bytes());
    bytes.extend_from_slice(&field_count.to_le_bytes());
    bytes.extend_from_slice(&(field_count * 4).to_le_bytes());
    bytes.extend_from_slice(&(string_block.len() as u32).to_le_bytes());
    for field in fields {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    bytes.extend_from_slice(string_block);
    bytes
}

/// Appends one NUL-terminated DBC string and returns its offset.
fn append_string(block: &mut Vec<u8>, value: &str) -> u32 {
    let offset = block.len() as u32;
    block.extend_from_slice(value.as_bytes());
    block.push(0);
    offset
}

/// Serializes one deterministic WotLK M2 with two texture stages.
fn render_m2_bytes(name: &str, skin_profiles: u32) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut model = M2Model {
        header: M2Header::new(M2Version::WotLK),
        name: Some(name.to_owned()),
        ..M2Model::default()
    };
    model.header.num_skin_profiles = Some(skin_profiles);
    model.header.flags |= M2ModelFlags::USE_TEXTURE_COMBINERS;
    model.header.texture_combiner_combos = Some(M2Array::new(0, 0));
    let texture_name = b"Creature\\Solarity\\Renderable.blp";
    model.textures = vec![
        RawTexture {
            texture_type: M2TextureType::Hardcoded,
            flags: M2TextureFlags::WRAP_X,
            filename: M2ArrayString {
                string: FixedString {
                    data: texture_name.to_vec(),
                },
                array: M2Array::new(u32::try_from(texture_name.len() + 1)?, 1),
            },
        },
        RawTexture {
            texture_type: M2TextureType::Monster1,
            flags: M2TextureFlags::empty(),
            filename: M2ArrayString::default(),
        },
    ];
    model.materials = vec![
        RawMaterial {
            flags: M2RenderFlags::UNFOGGED
                | M2RenderFlags::NO_BACKFACE_CULLING
                | M2RenderFlags::NO_ZBUFFER
                | M2RenderFlags::AFFECTED_BY_PROJECTION,
            blend_mode: RawBlendMode::ALPHA,
        },
        RawMaterial {
            flags: M2RenderFlags::UNFOGGED
                | M2RenderFlags::NO_BACKFACE_CULLING
                | M2RenderFlags::NO_ZBUFFER
                | M2RenderFlags::AFFECTED_BY_PROJECTION,
            blend_mode: RawBlendMode::ALPHA_KEY,
        },
        RawMaterial {
            flags: M2RenderFlags::empty(),
            blend_mode: RawBlendMode::MOD,
        },
    ];
    model.raw_data.texture_lookup_table = vec![0, 1];
    model.raw_data.texture_units = vec![0, u16::MAX];
    model.raw_data.bone_lookup_table = vec![2];
    for index in 0..3 {
        model.vertices.push(RawM2Vertex {
            position: C3Vector {
                x: index as f32,
                y: index as f32 + 0.25,
                z: index as f32 + 0.5,
            },
            bone_weights: [255, 0, 0, 0],
            bone_indices: [0, 1, 2, 3],
            normal: C3Vector {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
            tex_coords: C2Vector { x: 0.0, y: 0.5 },
            tex_coords2: Some(C2Vector { x: 1.0, y: 0.5 }),
        });
    }

    let mut cursor = Cursor::new(Vec::new());
    model.write(&mut cursor)?;
    let mut bytes = cursor.into_inner();
    let texture_offset = m2_array_offset(&bytes, 0x50)?;
    let filename_offset = u32::try_from(bytes.len())?;
    bytes[texture_offset + 8..texture_offset + 12]
        .copy_from_slice(&u32::try_from(texture_name.len() + 1)?.to_le_bytes());
    bytes[texture_offset + 12..texture_offset + 16].copy_from_slice(&filename_offset.to_le_bytes());
    bytes.extend_from_slice(texture_name);
    bytes.push(0);
    let combiners = [0_u16, 6, 3, 3];
    let combiner_offset = u32::try_from(bytes.len())?;
    bytes[0x130..0x134].copy_from_slice(&u32::try_from(combiners.len())?.to_le_bytes());
    bytes[0x134..0x138].copy_from_slice(&combiner_offset.to_le_bytes());
    for combiner in combiners {
        bytes.extend_from_slice(&combiner.to_le_bytes());
    }
    append_render_animation(&mut bytes)?;
    Ok(bytes)
}

/// Completes the fixture with one internal sequence and a three-bone hierarchy.
fn append_render_animation(bytes: &mut Vec<u8>) -> Result<(), Box<dyn Error>> {
    let sequence_offset = u32::try_from(bytes.len())?;
    let mut sequence = [0_u8; 64];
    sequence[4..8].copy_from_slice(&1_000_u32.to_le_bytes());
    sequence[12..16].copy_from_slice(&0x20_u32.to_le_bytes());
    sequence[16..18].copy_from_slice(&1_i16.to_le_bytes());
    sequence[60..62].copy_from_slice(&(-1_i16).to_le_bytes());
    bytes.extend_from_slice(&sequence);

    let bone_offset = u32::try_from(bytes.len())?;
    let mut bones = [[0_u8; 88]; 3];
    for (index, bone) in bones.iter_mut().enumerate() {
        bone[0..4].copy_from_slice(&(-1_i32).to_le_bytes());
        let parent = if index == 0 {
            -1_i16
        } else {
            (index - 1) as i16
        };
        bone[8..10].copy_from_slice(&parent.to_le_bytes());
        bone[18..20].copy_from_slice(&u16::MAX.to_le_bytes());
        bone[38..40].copy_from_slice(&u16::MAX.to_le_bytes());
        bone[58..60].copy_from_slice(&u16::MAX.to_le_bytes());
    }
    bones[0][16..18].copy_from_slice(&1_u16.to_le_bytes());
    for bone in bones {
        bytes.extend_from_slice(&bone);
    }

    let timestamp_refs = u32::try_from(bytes.len())?;
    bytes.extend_from_slice(&2_u32.to_le_bytes());
    let timestamp_offset_word = bytes.len();
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    let value_refs = u32::try_from(bytes.len())?;
    bytes.extend_from_slice(&2_u32.to_le_bytes());
    let value_offset_word = bytes.len();
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    let timestamp_data = u32::try_from(bytes.len())?;
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&1_000_u32.to_le_bytes());
    let value_data = u32::try_from(bytes.len())?;
    for value in [0.0_f32, 0.0, 0.0, 4.0, 0.0, 0.0] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes[timestamp_offset_word..timestamp_offset_word + 4]
        .copy_from_slice(&timestamp_data.to_le_bytes());
    bytes[value_offset_word..value_offset_word + 4].copy_from_slice(&value_data.to_le_bytes());

    let root = usize::try_from(bone_offset)?;
    bytes[root + 20..root + 24].copy_from_slice(&1_u32.to_le_bytes());
    bytes[root + 24..root + 28].copy_from_slice(&timestamp_refs.to_le_bytes());
    bytes[root + 28..root + 32].copy_from_slice(&1_u32.to_le_bytes());
    bytes[root + 32..root + 36].copy_from_slice(&value_refs.to_le_bytes());
    bytes[0x1c..0x20].copy_from_slice(&1_u32.to_le_bytes());
    bytes[0x20..0x24].copy_from_slice(&sequence_offset.to_le_bytes());
    bytes[0x2c..0x30].copy_from_slice(&3_u32.to_le_bytes());
    bytes[0x30..0x34].copy_from_slice(&bone_offset.to_le_bytes());
    append_render_material_tracks(bytes)?;
    append_render_ribbon(bytes)?;
    append_render_particle(bytes)?;
    Ok(())
}

/// Adds one build-12340 ribbon whose six tracks share the fixture sequence.
fn append_render_ribbon(bytes: &mut Vec<u8>) -> Result<(), Box<dyn Error>> {
    let ribbon_offset = bytes.len();
    bytes.resize(ribbon_offset + 176, 0);
    bytes[ribbon_offset..ribbon_offset + 4].copy_from_slice(&0x5249_424E_u32.to_le_bytes());
    bytes[ribbon_offset + 4..ribbon_offset + 8].copy_from_slice(&0_u32.to_le_bytes());
    bytes[ribbon_offset + 8..ribbon_offset + 20]
        .copy_from_slice(&render_f32_values(&[1.0, 2.0, 3.0]));

    let texture_indices = bytes.len();
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    set_render_header_array(bytes, ribbon_offset + 20, 1, texture_indices)?;
    let material_indices = bytes.len();
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    set_render_header_array(bytes, ribbon_offset + 28, 1, material_indices)?;

    append_render_track(
        bytes,
        ribbon_offset + 36,
        &[0, 1_000],
        &render_f32_values(&[1.0, 1.0, 1.0, 3.0, 2.0, 1.0]),
        12,
    )?;
    append_render_track(
        bytes,
        ribbon_offset + 56,
        &[0, 1_000],
        &render_i16_values(&[32_767, 16_384]),
        2,
    )?;
    append_render_track(
        bytes,
        ribbon_offset + 76,
        &[0, 1_000],
        &render_f32_values(&[1.0, 3.0]),
        4,
    )?;
    append_render_track(
        bytes,
        ribbon_offset + 96,
        &[0, 1_000],
        &render_f32_values(&[0.5, 1.5]),
        4,
    )?;
    bytes[ribbon_offset + 116..ribbon_offset + 120].copy_from_slice(&20.0_f32.to_le_bytes());
    bytes[ribbon_offset + 120..ribbon_offset + 124].copy_from_slice(&1.25_f32.to_le_bytes());
    bytes[ribbon_offset + 124..ribbon_offset + 128].copy_from_slice(&9.8_f32.to_le_bytes());
    bytes[ribbon_offset + 128..ribbon_offset + 130].copy_from_slice(&2_u16.to_le_bytes());
    bytes[ribbon_offset + 130..ribbon_offset + 132].copy_from_slice(&4_u16.to_le_bytes());
    append_render_track(bytes, ribbon_offset + 132, &[0, 1_000], &[1, 0, 3, 0], 2)?;
    append_render_track(bytes, ribbon_offset + 152, &[0, 1_000], &[1, 0], 1)?;
    bytes[ribbon_offset + 172..ribbon_offset + 174].copy_from_slice(&(-3_i16).to_le_bytes());
    bytes[ribbon_offset + 174] = u8::MAX;
    bytes[ribbon_offset + 175] = u8::MAX;
    set_render_header_array(bytes, 0x120, 1, ribbon_offset)?;
    Ok(())
}

/// Adds one planar particle emitter with both emitter-time and lifetime ramps.
fn append_render_particle(bytes: &mut Vec<u8>) -> Result<(), Box<dyn Error>> {
    let particle_offset = bytes.len();
    bytes.resize(particle_offset + 476, 0);
    bytes[particle_offset..particle_offset + 4].copy_from_slice(&0x5041_5254_u32.to_le_bytes());
    bytes[particle_offset + 4..particle_offset + 8].copy_from_slice(&0x0080_0000_u32.to_le_bytes());
    bytes[particle_offset + 20..particle_offset + 22].copy_from_slice(&0_u16.to_le_bytes());
    bytes[particle_offset + 22..particle_offset + 24].copy_from_slice(&0_u16.to_le_bytes());
    bytes[particle_offset + 40] = 2;
    bytes[particle_offset + 41] = 1;
    bytes[particle_offset + 45] = 2;
    bytes[particle_offset + 48..particle_offset + 50].copy_from_slice(&2_u16.to_le_bytes());
    bytes[particle_offset + 50..particle_offset + 52].copy_from_slice(&4_u16.to_le_bytes());

    let animated = [
        (0x034, [2.0_f32, 4.0_f32]),
        (0x048, [0.2, 0.4]),
        (0x05c, [0.4, 0.8]),
        (0x070, [0.6, 1.2]),
        (0x084, [8.0, 10.0]),
        (0x098, [1.0, 3.0]),
        (0x0b0, [10.0, 20.0]),
        (0x0c8, [4.0, 8.0]),
        (0x0dc, [2.0, 6.0]),
        (0x0f0, [1.0, 3.0]),
    ];
    for (field, values) in animated {
        append_render_track(
            bytes,
            particle_offset + field,
            &[0, 1_000],
            &render_f32_values(&values),
            4,
        )?;
    }
    append_render_lifetime_track(
        bytes,
        particle_offset + 0x104,
        &[0, i16::MAX as u16],
        &render_f32_values(&[1.0, 0.0, 0.0, 0.0, 0.0, 1.0]),
        12,
    )?;
    bytes[particle_offset + 0x134..particle_offset + 0x13c]
        .copy_from_slice(&render_f32_values(&[0.5, 0.25]));
    bytes[particle_offset + 0x15c..particle_offset + 0x160].copy_from_slice(&1.5_f32.to_le_bytes());
    bytes[particle_offset + 0x164..particle_offset + 0x168].copy_from_slice(&1.0_f32.to_le_bytes());
    append_render_lifetime_track(
        bytes,
        particle_offset + 0x114,
        &[0, i16::MAX as u16],
        &render_i16_values(&[32_767, 16_384]),
        2,
    )?;
    append_render_lifetime_track(
        bytes,
        particle_offset + 0x124,
        &[0, i16::MAX as u16],
        &render_f32_values(&[1.0, 1.0, 3.0, 5.0]),
        8,
    )?;
    append_render_lifetime_track(
        bytes,
        particle_offset + 0x13c,
        &[0, i16::MAX as u16],
        &render_u16_values(&[2, 7]),
        2,
    )?;
    append_render_lifetime_track(
        bytes,
        particle_offset + 0x14c,
        &[0, i16::MAX as u16],
        &render_u16_values(&[4, 9]),
        2,
    )?;
    for (relative, value) in [(0x178, 0.8_f32), (0x17c, 0.9), (0x180, 1.1), (0x184, 1.2)] {
        bytes[particle_offset + relative..particle_offset + relative + 4]
            .copy_from_slice(&value.to_le_bytes());
    }
    append_render_track(bytes, particle_offset + 0x1c8, &[0, 1_000], &[1, 0], 1)?;
    set_render_header_array(bytes, 0x128, 1, particle_offset)?;
    Ok(())
}

/// Adds the material tracks consumed by all three synthetic SKIN draws.
fn append_render_material_tracks(bytes: &mut Vec<u8>) -> Result<(), Box<dyn Error>> {
    let color_offset = bytes.len();
    bytes.resize(color_offset + 40, 0);
    let weight_offset = bytes.len();
    bytes.resize(weight_offset + 20, 0);
    let transform_offset = bytes.len();
    bytes.resize(transform_offset + 60, 0);

    append_render_track(
        bytes,
        color_offset,
        &[0, 1_000],
        &render_f32_values(&[1.0, 1.0, 1.0, 2.0, 2.0, 2.0]),
        12,
    )?;
    append_render_track(
        bytes,
        color_offset + 20,
        &[0, 1_000],
        &render_i16_values(&[32_767, 16_384]),
        2,
    )?;
    append_render_track(
        bytes,
        weight_offset,
        &[0, 1_000],
        &render_i16_values(&[32_767, 16_384]),
        2,
    )?;
    append_render_track(
        bytes,
        transform_offset,
        &[0, 1_000],
        &render_f32_values(&[0.0, 0.0, 0.0, 0.5, 0.0, 0.0]),
        12,
    )?;
    append_render_track(
        bytes,
        transform_offset + 20,
        &[0, 1_000],
        &render_i16_values(&[-32_768, -32_768, -32_768, -1, -32_768, -32_768, -32_768, -1]),
        8,
    )?;
    append_render_track(
        bytes,
        transform_offset + 40,
        &[0, 1_000],
        &render_f32_values(&[1.0, 1.0, 1.0, 1.0, 1.0, 1.0]),
        12,
    )?;

    let weight_lookup_offset = bytes.len();
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    let transform_lookup_offset = bytes.len();
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    set_render_header_array(bytes, 0x48, 1, color_offset)?;
    set_render_header_array(bytes, 0x58, 1, weight_offset)?;
    set_render_header_array(bytes, 0x60, 1, transform_offset)?;
    set_render_header_array(bytes, 0x90, 2, weight_lookup_offset)?;
    set_render_header_array(bytes, 0x98, 2, transform_lookup_offset)?;
    Ok(())
}

fn append_render_track(
    bytes: &mut Vec<u8>,
    track_offset: usize,
    timestamps: &[u32],
    values: &[u8],
    value_stride: usize,
) -> Result<(), Box<dyn Error>> {
    if values.len() != timestamps.len() * value_stride {
        return Err("render fixture track value count differs from timestamps".into());
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

fn append_render_lifetime_track(
    bytes: &mut Vec<u8>,
    track_offset: usize,
    timestamps: &[u16],
    values: &[u8],
    value_stride: usize,
) -> Result<(), Box<dyn Error>> {
    if values.len() != timestamps.len() * value_stride {
        return Err("render fixture lifetime value count differs from timestamps".into());
    }
    let timestamp_data = bytes.len();
    bytes.extend_from_slice(&render_u16_values(timestamps));
    let value_data = bytes.len();
    bytes.extend_from_slice(values);
    set_render_header_array(
        bytes,
        track_offset,
        u32::try_from(timestamps.len())?,
        timestamp_data,
    )?;
    set_render_header_array(
        bytes,
        track_offset + 8,
        u32::try_from(timestamps.len())?,
        value_data,
    )?;
    Ok(())
}

fn set_render_header_array(
    bytes: &mut [u8],
    pair_offset: usize,
    count: u32,
    offset: usize,
) -> Result<(), Box<dyn Error>> {
    bytes[pair_offset..pair_offset + 4].copy_from_slice(&count.to_le_bytes());
    bytes[pair_offset + 4..pair_offset + 8].copy_from_slice(&u32::try_from(offset)?.to_le_bytes());
    Ok(())
}

fn render_f32_values(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn render_i16_values(values: &[i16]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn render_u16_values(values: &[u16]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

/// Serializes one non-identity SKIN lookup and two-stage material batch.
fn render_skin_bytes() -> Result<Vec<u8>, Box<dyn Error>> {
    let skin = OldSkin {
        header: OldSkinHeader {
            bone_count_max: 32,
            ..OldSkinHeader::new()
        },
        indices: vec![2, 0, 1],
        triangles: vec![0, 1, 2],
        bone_indices: vec![0; 12],
        submeshes: vec![SkinSubmesh {
            id: 402,
            level: 0,
            vertex_start: 0,
            vertex_count: 3,
            triangle_start: 0,
            triangle_count: 3,
            bone_count: 1,
            bone_start: 0,
            bone_influence: 1,
            center: [0.0; 3],
            sort_center: [0.0; 3],
            bounding_radius: 1.0,
        }],
        batches: vec![
            SkinBatch {
                flags: 1,
                priority_plane: -2,
                shader_id: 0x8001,
                skin_section_index: 0,
                geoset_index: 0,
                color_index: 0,
                material_index: 1,
                material_layer: 1,
                texture_count: 2,
                texture_combo_index: 0,
                texture_coord_combo_index: 0,
                texture_weight_combo_index: 0,
                texture_transform_combo_index: 0,
            },
            SkinBatch {
                flags: 0,
                priority_plane: 0,
                shader_id: 0,
                skin_section_index: 0,
                geoset_index: 0,
                color_index: 0,
                material_index: 0,
                material_layer: 0,
                texture_count: 2,
                texture_combo_index: 0,
                texture_coord_combo_index: 0,
                texture_weight_combo_index: 0,
                texture_transform_combo_index: 0,
            },
            SkinBatch {
                flags: 0,
                priority_plane: 1,
                shader_id: 2,
                skin_section_index: 0,
                geoset_index: 0,
                color_index: 0,
                material_index: 2,
                material_layer: 0,
                texture_count: 2,
                texture_combo_index: 0,
                texture_coord_combo_index: 0,
                texture_weight_combo_index: 0,
                texture_transform_combo_index: 0,
            },
        ],
    };
    let mut cursor = Cursor::new(Vec::new());
    skin.write(&mut cursor)?;
    let mut bytes = cursor.into_inner();
    let submesh_offset = u32::from_le_bytes(bytes[32..36].try_into()?) as usize;
    bytes[submesh_offset + 18..submesh_offset + 20].copy_from_slice(&5_u16.to_le_bytes());
    // wow-m2 0.7 advances its synthetic SKIN writer by 40 bytes even though
    // the WotLK submesh it emits occupies 48 bytes. Repair the fixture's batch
    // offset so the decoder sees the values written after the whole submesh.
    let batch_offset = u32::try_from(submesh_offset + 48)?;
    bytes[40..44].copy_from_slice(&batch_offset.to_le_bytes());
    Ok(bytes)
}

/// Reads one M2 header array's physical byte offset for fixture repair.
fn m2_array_offset(bytes: &[u8], pair_offset: usize) -> Result<usize, Box<dyn Error>> {
    Ok(u32::from_le_bytes(bytes[pair_offset + 4..pair_offset + 8].try_into()?) as usize)
}

/// Builds a BLP2/RAW3 authored mip chain with one solid color per level.
fn solid_raw3_blp(width: u32, height: u32, colors: &[u32]) -> Vec<u8> {
    const HEADER_SIZE: u32 = 148;
    const PALETTE_SIZE: u32 = 256 * 4;
    const PIXEL_OFFSET: u32 = HEADER_SIZE + PALETTE_SIZE;

    let mut offsets = [0_u32; 16];
    let mut sizes = [0_u32; 16];
    let mut next_offset = PIXEL_OFFSET;
    for (level, _color) in colors.iter().take(16).enumerate() {
        let mip_width = (width >> level).max(1);
        let mip_height = (height >> level).max(1);
        let byte_size = mip_width.saturating_mul(mip_height).saturating_mul(4);
        offsets[level] = next_offset;
        sizes[level] = byte_size;
        next_offset = next_offset.saturating_add(byte_size);
    }

    let mut bytes = Vec::with_capacity(next_offset as usize);
    bytes.extend_from_slice(b"BLP2");
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&[3, 8, 8, u8::from(colors.len() > 1)]);
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    for offset in offsets {
        bytes.extend_from_slice(&offset.to_le_bytes());
    }
    for size in sizes {
        bytes.extend_from_slice(&size.to_le_bytes());
    }
    bytes.resize(PIXEL_OFFSET as usize, 0);
    for (level, color) in colors.iter().take(16).enumerate() {
        let pixel_count = (width >> level).max(1) * (height >> level).max(1);
        for _pixel in 0..pixel_count {
            bytes.extend_from_slice(&color.to_le_bytes());
        }
    }
    bytes
}

/// Reads one tightly packed RGBA8 fixture pixel.
fn rgba8_pixel(rgba8: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let index = ((y * width + x) * 4) as usize;
    [
        rgba8[index],
        rgba8[index + 1],
        rgba8[index + 2],
        rgba8[index + 3],
    ]
}
