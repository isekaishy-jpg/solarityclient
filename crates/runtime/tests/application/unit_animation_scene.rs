//! Scene culling and GPU replacement cannot restart a unit's primary timer.

#[path = "equipment_residency.rs"]
mod equipment_residency;

use super::super::{M2PlaybackStorage, m2_gpu_placement};
use super::*;
use crate::application::unit_animation::{UnitAnimationBehavior, UnitAnimationInput};
use glam::Mat4;
use solarity_ecs::UnitAnimationTier;
use solarity_rendering::{M2CameraEffectScale, WorldCamera, WorldFrustum, WorldScreenWindow};
use solarity_systems::UnitLocomotionAnimation;
use std::rc::Rc;

#[test]
fn replicated_units_retain_cpu_and_gpu_generations_when_neighbors_change()
-> Result<(), Box<dyn Error>> {
    let _sdl_guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let fixture = crate::test_support::unit_models::fixture()?;
    let mut presentation = unit_presentation(&fixture)?;
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.0,
    ));
    add_unit(&mut world, 20, ObjectKind::Player, 1)?;
    add_unit(&mut world, 30, ObjectKind::Unit, 3)?;
    presentation.synchronize_creatures(Some(&world))?;
    presentation.synchronize_remote_players(Some(&world))?;
    let remote = Rc::clone(
        presentation.resident_remote_player_frame_inputs()[0]
            .unit_animation()
            .ok_or("remote owner")?,
    );
    let creature = Rc::clone(
        presentation.resident_creature_frame_inputs()[0]
            .unit_animation()
            .ok_or("creature owner")?,
    );
    assert!(!Rc::ptr_eq(&remote, &creature));
    let remote_generation = presentation.resident_remote_player_frame_inputs()[0]
        .generation()
        .clone();
    let creature_generation = presentation.resident_creature_frame_inputs()[0]
        .generation()
        .clone();
    let platform = SdlPlatform::start(WindowConfiguration::new(128, 128, WindowMode::Windowed))?;
    let mut renderer = renderer(&platform)?;
    let mut random = CrtRand::new();
    let mut frame = M2Frame::prepare(
        &mut renderer,
        &ResidentM2Scene::default(),
        fixture_animations(&fixture)?,
        &mut random,
        Arc::new(M2ParticleTwinkleTable::new(1)),
    )?;
    frame.replace_creatures(
        &mut renderer,
        &presentation.resident_creature_frame_inputs(),
        &mut random,
    )?;
    frame.replace_remote_players(
        &mut renderer,
        &presentation.resident_remote_player_frame_inputs(),
        &mut random,
    )?;
    assert_eq!(remote.playback().borrow().animation_id, 96);
    assert_eq!(creature.playback().borrow().animation_id, 99);
    let remote_source = unit_source(&frame, M2GpuPlacementOwner::RemotePlayerBody { guid: 20 })?;
    let creature_source = unit_source(&frame, M2GpuPlacementOwner::CreatureBody { guid: 30 })?;
    let camera = WorldCamera::orthographic(
        Vec3::new(8.0, 0.0, 0.0),
        Vec3::ZERO,
        Vec3::Z,
        [-4.0, 4.0],
        [-2.0, 2.0],
        0.1,
        100.0,
    )
    .frame(1.0)?;
    frame.prepare_visible_draws(
        &renderer,
        WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
        camera,
        Vec3::ZERO,
        1500.0,
        1500.0,
        M2CameraEffectScale::EXTERNAL_CAMERA,
        &mut random,
        None,
    )?;
    assert_eq!(remote.playback().borrow().animation_id, 97);
    assert_eq!(creature.playback().borrow().animation_id, 100);
    let mut expected = random;
    for _ in 0..4 {
        let _new_owner_roll = expected.next_u15();
    }
    add_unit(&mut world, 21, ObjectKind::Player, 1)?;
    add_unit(&mut world, 31, ObjectKind::Unit, 3)?;
    presentation.synchronize_creatures(Some(&world))?;
    presentation.synchronize_remote_players(Some(&world))?;
    assert!(
        remote_generation
            .matches(presentation.resident_remote_player_frame_inputs()[0].generation())
    );
    assert!(
        creature_generation.matches(presentation.resident_creature_frame_inputs()[0].generation())
    );
    frame.replace_creatures(
        &mut renderer,
        &presentation.resident_creature_frame_inputs(),
        &mut random,
    )?;
    frame.replace_remote_players(
        &mut renderer,
        &presentation.resident_remote_player_frame_inputs(),
        &mut random,
    )?;
    assert_eq!(
        random, expected,
        "arrivals roll only their own initial sequences"
    );
    assert_eq!(
        unit_source(&frame, M2GpuPlacementOwner::RemotePlayerBody { guid: 20 })?,
        remote_source
    );
    assert_eq!(
        unit_source(&frame, M2GpuPlacementOwner::CreatureBody { guid: 30 })?,
        creature_source
    );
    assert_eq!(remote.playback().borrow().animation_id, 97);
    assert_eq!(creature.playback().borrow().animation_id, 100);

    presentation.set_component_texture_level(
        solarity_rendering::CharacterComponentTextureLevel::new(8).ok_or("texture level")?,
    );
    presentation.synchronize_remote_players(Some(&world))?;
    let rebuilt = presentation.resident_remote_player_frame_inputs();
    assert!(!remote_generation.matches(rebuilt[0].generation()));
    assert!(Rc::ptr_eq(
        &remote,
        rebuilt[0].unit_animation().ok_or("retained remote owner")?
    ));
    frame.replace_remote_players(&mut renderer, &rebuilt, &mut random)?;
    assert_eq!(
        random, expected,
        "material replacement borrows the old timer"
    );

    for guid in [20, 30] {
        world.update_fields(guid, [(74, 0)])?;
        solarity_systems::project_object_fields(&mut world, guid, [(74, 0)])?;
    }
    presentation.synchronize_creatures(Some(&world))?;
    presentation.synchronize_remote_players(Some(&world))?;
    frame.update_creature_states(
        &presentation.resident_creature_frame_inputs(),
        1700.0,
        &mut random,
    )?;
    frame.update_remote_player_states(
        &presentation.resident_remote_player_frame_inputs(),
        1700.0,
        &mut random,
    )?;
    assert_eq!(remote.playback().borrow().animation_id, 98);
    assert_eq!(creature.playback().borrow().animation_id, 101);
    frame.prepare_visible_draws(
        &renderer,
        WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
        camera,
        Vec3::ZERO,
        3000.0,
        3000.0,
        M2CameraEffectScale::EXTERNAL_CAMERA,
        &mut random,
        None,
    )?;
    assert_eq!(remote.playback().borrow().animation_id, 0);
    assert_eq!(creature.playback().borrow().animation_id, 0);

    world.remove_object(20)?;
    add_unit(&mut world, 20, ObjectKind::Player, 1)?;
    presentation.synchronize_remote_players(Some(&world))?;
    let replaced = presentation.resident_remote_player_frame_inputs();
    assert!(!Rc::ptr_eq(
        &remote,
        replaced[0].unit_animation().ok_or("replacement owner")?
    ));
    frame.replace_remote_players(&mut renderer, &replaced, &mut random)?;
    assert_eq!(
        replaced[0]
            .unit_animation()
            .ok_or("replacement owner")?
            .playback()
            .borrow()
            .animation_id,
        96
    );
    world.remove_object(30)?;
    presentation.synchronize_creatures(Some(&world))?;
    frame.replace_creatures(
        &mut renderer,
        &presentation.resident_creature_frame_inputs(),
        &mut random,
    )?;
    assert!(
        !frame
            .placements
            .iter()
            .any(|placement| placement.owner == (M2GpuPlacementOwner::CreatureBody { guid: 30 }))
    );
    assert!(
        frame.placements.iter().any(
            |placement| placement.owner == (M2GpuPlacementOwner::RemotePlayerBody { guid: 20 })
        )
    );
    Ok(())
}

#[test]
fn unit_material_replacement_retains_live_effects_but_new_lifetimes_start_empty()
-> Result<(), Box<dyn Error>> {
    let _sdl_guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let fixture = crate::test_support::unit_models::fixture_with_effects()?;
    let mut presentation = unit_presentation(&fixture)?;
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.0,
    ));
    for (guid, kind) in [
        (7, ObjectKind::Player),
        (20, ObjectKind::Player),
        (30, ObjectKind::Unit),
    ] {
        add_unit(&mut world, guid, kind, 0)?;
    }
    presentation.synchronize(Some(&world))?;
    presentation.synchronize_creatures(Some(&world))?;
    presentation.synchronize_remote_players(Some(&world))?;
    let platform = SdlPlatform::start(WindowConfiguration::new(128, 128, WindowMode::Windowed))?;
    let mut renderer = renderer(&platform)?;
    let mut random = CrtRand::new();
    let mut frame = M2Frame::prepare(
        &mut renderer,
        &ResidentM2Scene::default(),
        fixture_animations(&fixture)?,
        &mut random,
        Arc::new(M2ParticleTwinkleTable::new(1)),
    )?;
    let publish =
        |frame: &mut M2Frame,
         renderer: &mut VulkanRenderer,
         presentation: &crate::application::player_coordinator::RuntimePlayerPresentation,
         random: &mut CrtRand|
         -> Result<(), Box<dyn Error>> {
            frame.replace_player(renderer, presentation.resident_frame_input(), random)?;
            frame.replace_creatures(
                renderer,
                &presentation.resident_creature_frame_inputs(),
                random,
            )?;
            frame.replace_remote_players(
                renderer,
                &presentation.resident_remote_player_frame_inputs(),
                random,
            )?;
            Ok(())
        };
    publish(&mut frame, &mut renderer, &presentation, &mut random)?;
    let camera = WorldCamera::orthographic(
        Vec3::new(8.0, 0.0, 0.0),
        Vec3::ZERO,
        Vec3::Z,
        [-4.0, 4.0],
        [-2.0, 2.0],
        0.1,
        100.0,
    )
    .frame(1.0)?;
    for time in [100.0, 300.0] {
        frame.prepare_visible_draws(
            &renderer,
            WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
            camera,
            Vec3::ZERO,
            time,
            time,
            M2CameraEffectScale::EXTERNAL_CAMERA,
            &mut random,
            None,
        )?;
    }
    let owners = [
        M2GpuPlacementOwner::PlayerBody { guid: 7 },
        M2GpuPlacementOwner::RemotePlayerBody { guid: 20 },
        M2GpuPlacementOwner::CreatureBody { guid: 30 },
    ];
    let before = owners
        .map(|owner| effects(&frame, owner))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    for state in &before {
        assert!(
            !state.particles.is_empty(),
            "fixture must emit live particles"
        );
        assert!(
            state.ribbons.len() > 1,
            "fixture must accumulate ribbon history"
        );
    }
    let sources = owners
        .map(|owner| unit_source(&frame, owner))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    let expected_random = random;

    // Texture quality changes rebuild both player atlases. A display alias
    // changes the creature's material generation while retaining its model.
    presentation.set_component_texture_level(
        solarity_rendering::CharacterComponentTextureLevel::new(8).ok_or("texture level")?,
    );
    world.update_fields(30, [(67, 101)])?;
    solarity_systems::project_object_fields(&mut world, 30, [(67, 101)])?;
    presentation.synchronize(Some(&world))?;
    presentation.synchronize_creatures(Some(&world))?;
    presentation.synchronize_remote_players(Some(&world))?;
    publish(&mut frame, &mut renderer, &presentation, &mut random)?;
    assert_eq!(random, expected_random);
    for (index, owner) in owners.into_iter().enumerate() {
        assert_ne!(
            unit_source(&frame, owner)?,
            sources[index],
            "actual GPU replacement"
        );
        assert_eq!(effects(&frame, owner)?, before[index], "{owner:?}");
    }
    frame.prepare_visible_draws(
        &renderer,
        WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
        camera,
        Vec3::ZERO,
        350.0,
        350.0,
        M2CameraEffectScale::EXTERNAL_CAMERA,
        &mut random,
        None,
    )?;
    for (index, owner) in owners.into_iter().enumerate() {
        let after = effects(&frame, owner)?;
        assert_eq!(after.particle_allocation, before[index].particle_allocation);
        assert!(after.particles[0].age_seconds() > before[index].particles[0].age_seconds());
        assert!(after.ribbons[0].age_seconds() > before[index].ribbons[0].age_seconds());
    }

    // Reusing a GUID or changing model must not inherit these live histories.
    world.remove_object(20)?;
    add_unit(&mut world, 20, ObjectKind::Player, 0)?;
    world.update_fields(30, [(67, 102)])?;
    solarity_systems::project_object_fields(&mut world, 30, [(67, 102)])?;
    presentation.synchronize_creatures(Some(&world))?;
    presentation.synchronize_remote_players(Some(&world))?;
    publish(&mut frame, &mut renderer, &presentation, &mut random)?;
    for owner in [owners[1], owners[2]] {
        let state = effects(&frame, owner)?;
        assert!(state.particles.is_empty());
        assert!(state.ribbons.is_empty());
    }
    assert!(!effects(&frame, owners[0])?.particles.is_empty());

    world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.0,
    ));
    add_unit(&mut world, 7, ObjectKind::Player, 0)?;
    presentation.synchronize(Some(&world))?;
    frame.replace_player(
        &mut renderer,
        presentation.resident_frame_input(),
        &mut random,
    )?;
    let state = effects(&frame, owners[0])?;
    assert!(state.particles.is_empty());
    assert!(state.ribbons.is_empty());
    Ok(())
}

#[derive(Debug, PartialEq)]
struct UnitEffectsSnapshot {
    last_update_ms: u32,
    particles: Vec<solarity_rendering::M2ParticleState>,
    particle_allocation: usize,
    ribbons: Vec<solarity_rendering::M2RibbonSection>,
}

fn effects(
    frame: &M2Frame,
    owner: M2GpuPlacementOwner,
) -> Result<UnitEffectsSnapshot, Box<dyn Error>> {
    let placement = frame
        .placements
        .iter()
        .find(|placement| placement.owner == owner)
        .ok_or("unit placement")?;
    let particles = placement.particles[0].simulation.particles();
    Ok(UnitEffectsSnapshot {
        last_update_ms: placement.last_effect_time_ms,
        particles: particles.to_vec(),
        particle_allocation: particles.as_ptr().addr(),
        ribbons: placement.ribbons[0].sections().copied().collect(),
    })
}

fn fixture_animations(
    fixture: &ClientFixture,
) -> Result<Arc<AnimationDataCatalog>, Box<dyn Error>> {
    let archive =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(archive)?;
    Ok(Arc::new(AnimationDataCatalog::load(&mut store)?))
}

fn unit_presentation(
    fixture: &ClientFixture,
) -> Result<crate::application::player_coordinator::RuntimePlayerPresentation, Box<dyn Error>> {
    use crate::application::player_coordinator::{
        RuntimePlayerCatalogs, RuntimePlayerItemCatalogs, RuntimePlayerPresentation,
    };
    use solarity_asset::{
        CharacterAppearanceCatalog, CharacterRaceCatalog, CharacterStartOutfitCatalog,
        CreatureCatalog, CreatureFamilyCatalog, HelmetGeosetVisibilityCatalog,
        ItemDefinitionCatalog, ItemDisplayCatalog, ItemVisualCatalog, ParticleColorCatalog,
    };
    let archive =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(archive)?;
    let catalogs = RuntimePlayerCatalogs::new(
        AnimationDataCatalog::load(&mut store)?,
        CreatureCatalog::load(&mut store)?,
        CreatureFamilyCatalog::default(),
        CharacterAppearanceCatalog::load(&mut store)?,
        CharacterRaceCatalog::load(&mut store)?,
        HelmetGeosetVisibilityCatalog::load(&mut store)?,
        CharacterStartOutfitCatalog::load(&mut store)?,
        RuntimePlayerItemCatalogs::new(
            ItemDefinitionCatalog::load(&mut store)?,
            ItemDisplayCatalog::load(&mut store)?,
            ItemVisualCatalog::load(&mut store)?,
        ),
        ParticleColorCatalog::load(&mut store)?,
    );
    Ok(RuntimePlayerPresentation::new(
        AssetStoreHandle::new(store),
        catalogs,
    ))
}

fn add_unit(
    world: &mut ActiveWorld,
    guid: u64,
    kind: ObjectKind,
    stand: u8,
) -> Result<(), Box<dyn Error>> {
    let fields = [
        (4, 1.0_f32.to_bits()),
        (23, u32::from_le_bytes([1, 1, 0, 0])),
        (67, 100),
        (68, 100),
        (69, 0),
        (74, u32::from(stand)),
        (122, 0),
        (153, 0),
        (154, 0),
    ];
    world.create_object(
        guid,
        kind,
        Some(WorldTransform::new(Vec3::ZERO, 0.0)),
        fields,
    )?;
    solarity_systems::project_object_fields(world, guid, fields)?;
    Ok(())
}

fn unit_source(frame: &M2Frame, owner: M2GpuPlacementOwner) -> Result<usize, Box<dyn Error>> {
    let placement = frame
        .placements
        .iter()
        .find(|placement| placement.owner == owner)
        .ok_or("unit placement")?;
    assert!(
        frame.sources[placement.source_index]
            .as_ref()
            .ok_or("unit source")?
            .mesh
            .is_some()
    );
    Ok(placement.source_index)
}

#[test]
fn unit_completion_precedes_culling_and_survives_gpu_placement_replacement()
-> Result<(), Box<dyn Error>> {
    let _sdl_guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let mut bytes = models::model_with_animations(&[0, 96, 97])?;
    let sequences = u32::from_le_bytes(bytes[0x20..0x24].try_into()?) as usize;
    bytes[sequences + 64 + 12..sequences + 64 + 16].copy_from_slice(&0x21_u32.to_le_bytes());
    let mut dbc = b"WDBC".to_vec();
    for value in [3_u32, 8, 32, 1] {
        dbc.extend_from_slice(&value.to_le_bytes());
    }
    for id in [0, 96, 97] {
        for value in [id, 0, 0, 0, 0, 0, id, 0] {
            dbc.extend_from_slice(&u32::to_le_bytes(value));
        }
    }
    dbc.push(0);
    let fixture = ClientFixture::with_common_files(&[
        ("World\\GameObject.m2", &bytes),
        ("World\\GameObject00.skin", &models::skin()?),
        (
            "DBFilesClient\\GameObjectDisplayInfo.dbc",
            &models::displays(),
        ),
        ("DBFilesClient\\AnimationData.dbc", &dbc),
    ])?;
    let archive =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(archive)?;
    let model = Arc::new(DecodedM2Model::load(
        &mut store,
        &AssetPath::new("World\\GameObject.m2")?,
    )?);
    let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
    let displays = GameObjectDisplayCatalog::load(&mut store)?;
    let mut objects = RuntimeGameObjectPresentation::new(
        AssetStoreHandle::new(store),
        displays,
        Arc::clone(&animations),
    );
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.0,
    ));
    add_object(&mut world, 30, 42)?;
    objects.synchronize(Some(&world))?;
    let platform = SdlPlatform::start(WindowConfiguration::new(128, 128, WindowMode::Windowed))?;
    let mut renderer = renderer(&platform)?;
    let mut random = CrtRand::new();
    let mut frame = M2Frame::prepare(
        &mut renderer,
        &ResidentM2Scene::default(),
        fixture_animations(&fixture)?,
        &mut random,
        Arc::new(M2ParticleTwinkleTable::new(1)),
    )?;
    objects.synchronize_animations(Some(&world), &mut random)?;
    frame.synchronize_game_objects(
        &mut renderer,
        objects.frame_input(Some(&world)),
        &mut random,
    )?;
    let owner = Rc::new(UnitAnimationBehavior::new(
        world.object_identity(7).ok_or("local identity")?,
        Arc::clone(&model),
        animations,
        UnitAnimationInput {
            stand: 1,
            locomotion: UnitLocomotionAnimation::STAND,
            tier: UnitAnimationTier::Ground,
            movement_flags: 0,
            mounted: false,
        },
    ));
    owner.synchronize(100, &mut random)?;
    let playback = owner.playback();
    frame.placements.clear();
    let camera = WorldCamera::orthographic(
        Vec3::new(8.0, 0.0, 0.0),
        Vec3::ZERO,
        Vec3::Z,
        [-4.0, 4.0],
        [-2.0, 2.0],
        0.1,
        100.0,
    )
    .frame(1.0)?;
    let mut expected = random;
    let _variation = expected.next_u15();
    let _cycle = expected.next_u15();
    for (time, transform, visible) in [
        (1500.0, Mat4::from_translation(Vec3::Y * 10_000.0), false),
        (1600.0, Mat4::IDENTITY, true),
    ] {
        // This is the same placement constructor used by character GPU admission.
        frame.placements.clear();
        let mut placement = m2_gpu_placement(
            0,
            transform,
            M2GpuPlacementOwner::PlayerBody { guid: 7 },
            &model,
            Some(M2PlaybackStorage::Shared(owner.playback())),
            None,
            0,
        )?;
        placement.unit_animation = Some(Rc::clone(&owner));
        frame.placements.push(placement);
        let draws = frame.prepare_visible_draws(
            &renderer,
            WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
            camera,
            Vec3::ZERO,
            time,
            time,
            M2CameraEffectScale::EXTERNAL_CAMERA,
            &mut random,
            None,
        )?;
        assert_eq!(!draws.draws.is_empty(), visible);
        assert_eq!(playback.borrow().animation_id, 97);
        assert_eq!(
            playback
                .borrow()
                .script_timer
                .ok_or("primary timer")?
                .start_time_ms(),
            1500
        );
        assert_eq!(
            random, expected,
            "only completion's incoming sequence rolls"
        );
    }
    assert!(
        owner.take_scene_sample().is_none(),
        "visible draws consumed the prepared CPU sample"
    );
    Ok(())
}
