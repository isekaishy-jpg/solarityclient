//! Removed hierarchies keep their GPU/effect state independently of GUID reuse.

use super::equipment_residency::advance;
use super::*;

#[test]
fn removed_mount_keeps_rider_attachment_and_retires_an_unready_hierarchy()
-> Result<(), Box<dyn Error>> {
    let _sdl_guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let fixture = crate::test_support::unit_models::fixture_with_mount_effects()?;
    let mut presentation = unit_presentation(&fixture)?;
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.,
    ));
    add_unit(&mut world, 30, ObjectKind::Unit, 0)?;
    world.update_fields(30, [(69, 102)])?;
    solarity_systems::project_object_fields(&mut world, 30, [(69, 102)])?;
    presentation.synchronize_creatures(Some(&world), |_| None)?;
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
    assert_eq!(frame.placements.len(), 2);
    let camera = WorldCamera::orthographic(
        Vec3::new(20., 0., 0.),
        Vec3::ZERO,
        Vec3::Z,
        [-10., 10.],
        [-10., 10.],
        0.1,
        100.,
    )
    .frame(1.)?;
    advance(&mut frame, &renderer, camera, 1200., &mut random)?;
    let old_body = frame.placements[1].transform;
    let old_mount = frame.placements[0].transform;
    assert_ne!(
        old_body, old_mount,
        "the authored saddle must position the rider"
    );
    presentation.set_animation_scene_time(1200);
    world.remove_object(30)?;
    presentation.synchronize_creatures(Some(&world), |_| None)?;
    frame.replace_creatures(
        &mut renderer,
        &presentation.resident_creature_frame_inputs(),
        &mut random,
    )?;
    assert_eq!(frame.placements.len(), 2);
    assert!(
        frame.placements[1]
            .retirement
            .as_ref()
            .ok_or("retired rider")?
            .parent()
            .is_some()
    );
    advance(&mut frame, &renderer, camera, 1700., &mut random)?;
    assert_eq!(frame.placements[0].transform, old_mount);
    assert_eq!(frame.placements[1].transform, old_body);
    assert_eq!(frame.placements[0].last_effect_time_ms, 1700);
    // 823E40 checks recursive readiness, so losing a child also retires its root.
    let child_source = frame.placements[1].source_index;
    frame.sources[child_source] = None;
    advance(&mut frame, &renderer, camera, 1800., &mut random)?;
    assert!(frame.placements.is_empty());
    assert!(frame.sources.is_empty());
    // A new identity removed before its first visible opacity byte never enters
    // the detached list (783630 requires initial opacity >= 0.01).
    add_unit(&mut world, 30, ObjectKind::Unit, 0)?;
    presentation.set_animation_scene_time(1800);
    presentation.synchronize_creatures(Some(&world), |_| None)?;
    frame.replace_creatures(
        &mut renderer,
        &presentation.resident_creature_frame_inputs(),
        &mut random,
    )?;
    world.remove_object(30)?;
    presentation.synchronize_creatures(Some(&world), |_| None)?;
    frame.replace_creatures(
        &mut renderer,
        &presentation.resident_creature_frame_inputs(),
        &mut random,
    )?;
    assert!(frame.placements.is_empty());
    Ok(())
}

#[test]
fn retired_game_object_follows_transport_then_freezes_when_parent_leaves()
-> Result<(), Box<dyn Error>> {
    let _sdl_guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let fixture = ClientFixture::with_common_files(&[
        (
            "World\\GameObject.m2",
            &models::model_with_animations(&[0])?,
        ),
        ("World\\GameObject00.skin", &models::skin()?),
        (
            "DBFilesClient\\GameObjectDisplayInfo.dbc",
            &models::displays(),
        ),
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let displays = GameObjectDisplayCatalog::load(&mut store)?;
    let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
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
        0.,
    ));
    add_object(&mut world, 99, 42)?;
    add_object(&mut world, 30, 42)?;
    world.update_game_object_movement(
        30,
        GameObjectMovement::new(
            0,
            Some(GameObjectTransport {
                guid: 99,
                position: Vec3::X,
                orientation: 0.,
            }),
        ),
    )?;
    objects.synchronize(Some(&world))?;
    let platform = SdlPlatform::start(WindowConfiguration::new(128, 128, WindowMode::Windowed))?;
    let mut renderer = renderer(&platform)?;
    let mut random = CrtRand::new();
    let mut frame = M2Frame::prepare(
        &mut renderer,
        &ResidentM2Scene::default(),
        animations,
        &mut random,
        Arc::new(M2ParticleTwinkleTable::new(1)),
    )?;
    objects.synchronize_animations(Some(&world), &mut random)?;
    frame.synchronize_game_objects(
        &mut renderer,
        objects.frame_input(Some(&world)),
        &mut random,
    )?;
    let camera = WorldCamera::orthographic(
        Vec3::new(100., 8., 0.),
        Vec3::new(100., 0., 0.),
        Vec3::Z,
        [-20., 20.],
        [-10., 10.],
        0.1,
        100.,
    )
    .frame(1.)?;
    let draw = |renderer: &VulkanRenderer,
                frame: &mut M2Frame,
                objects: &RuntimeGameObjectPresentation,
                world: &ActiveWorld,
                time: f32,
                random: &mut CrtRand|
     -> Result<(), Box<dyn Error>> {
        frame.prepare_visible_draws(
            renderer,
            WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
            camera,
            solarity_rendering::M2TransparentPass::One,
            Vec3::ZERO,
            time,
            M2CameraEffectScale::EXTERNAL_CAMERA,
            random,
            Some(objects.frame_input(Some(world))),
        )?;
        Ok(())
    };
    draw(&renderer, &mut frame, &objects, &world, 1200., &mut random)?;
    world.remove_object(30)?;
    objects.synchronize(Some(&world))?;
    frame.synchronize_game_objects(
        &mut renderer,
        objects.frame_input(Some(&world)),
        &mut random,
    )?;
    let owner = frame
        .placements
        .iter()
        .find(|placement| placement.retirement.is_some())
        .ok_or("retired child")?
        .owner;
    // Capture the native inverse-parent-relative matrix at transfer.
    draw(&renderer, &mut frame, &objects, &world, 1200., &mut random)?;
    let position = |frame: &M2Frame| -> Result<Vec3, Box<dyn Error>> {
        Ok(frame
            .placements
            .iter()
            .find(|placement| placement.owner == owner)
            .ok_or("retired pose")?
            .transform
            .w_axis
            .truncate())
    };
    assert!((position(&frame)?.x - 100.).abs() < 0.0001);
    world.update_transform(99, WorldTransform::new(Vec3::X * 109., 0.))?;
    objects.synchronize(Some(&world))?;
    draw(&renderer, &mut frame, &objects, &world, 1700., &mut random)?;
    assert!((position(&frame)?.x - 110.).abs() < 0.0001);
    world.remove_object(99)?;
    objects.synchronize(Some(&world))?;
    frame.synchronize_game_objects(
        &mut renderer,
        objects.frame_input(Some(&world)),
        &mut random,
    )?;
    draw(&renderer, &mut frame, &objects, &world, 1800., &mut random)?;
    assert!((position(&frame)?.x - 110.).abs() < 0.0001);
    add_object(&mut world, 99, 42)?;
    world.update_transform(99, WorldTransform::new(Vec3::X * 999., 0.))?;
    objects.synchronize(Some(&world))?;
    objects.synchronize_animations(Some(&world), &mut random)?;
    frame.synchronize_game_objects(
        &mut renderer,
        objects.frame_input(Some(&world)),
        &mut random,
    )?;
    assert_eq!(
        frame.sources.len(),
        1,
        "a new prop reuses the authored GPU source held by retired props"
    );
    draw(&renderer, &mut frame, &objects, &world, 1900., &mut random)?;
    assert!(
        (position(&frame)?.x - 110.).abs() < 0.0001,
        "a lost parent cannot be reacquired"
    );
    Ok(())
}

#[test]
fn removed_equipment_hierarchy_fades_without_rebinding_to_a_reused_guid()
-> Result<(), Box<dyn Error>> {
    let _sdl_guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let fixture = crate::test_support::unit_models::fixture_with_equipment()?;
    let mut presentation = unit_presentation(&fixture)?;
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.,
    ));
    add_unit(&mut world, 20, ObjectKind::Player, 0)?;
    let equipment = [(122, 1), (283, 1000), (287, 2000), (313, 3000), (314, 900)];
    world.update_fields(20, equipment)?;
    solarity_systems::project_object_fields(&mut world, 20, equipment)?;
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
    let camera = WorldCamera::orthographic(
        Vec3::new(8., 0., 0.),
        Vec3::ZERO,
        Vec3::Z,
        [-4., 4.],
        [-2., 2.],
        0.1,
        100.,
    )
    .frame(1.)?;
    presentation.synchronize_remote_players(Some(&world))?;
    frame.replace_remote_players(
        &mut renderer,
        &presentation.resident_remote_player_frame_inputs(),
        &mut random,
    )?;
    advance(&mut frame, &renderer, camera, 1200., &mut random)?;
    assert_eq!(frame.placements.len(), 9);
    let old_owner = Rc::downgrade(
        frame
            .placements
            .iter()
            .find_map(|placement| placement.unit_animation.as_ref())
            .ok_or("unit owner")?,
    );
    let old_sources = frame
        .placements
        .iter()
        .map(|placement| placement.source_index)
        .collect::<Vec<_>>();
    let effects = frame
        .placements
        .iter()
        .map(|placement| {
            (
                placement
                    .particles
                    .iter()
                    .map(|particle| particle.simulation.particles().len())
                    .sum::<usize>(),
                placement
                    .ribbons
                    .iter()
                    .map(|ribbon| ribbon.sections().count())
                    .sum::<usize>(),
            )
        })
        .collect::<Vec<_>>();
    assert!(effects.iter().any(|(particles, _)| *particles > 0));
    presentation.set_animation_scene_time(1200);
    world.remove_object(20)?;
    add_unit(&mut world, 20, ObjectKind::Player, 0)?;
    world.update_fields(20, equipment)?;
    solarity_systems::project_object_fields(&mut world, 20, equipment)?;
    world.update_transform(20, WorldTransform::new(Vec3::new(0., 50., 0.), 1.))?;
    presentation.synchronize_remote_players(Some(&world))?;
    frame.replace_remote_players(
        &mut renderer,
        &presentation.resident_remote_player_frame_inputs(),
        &mut random,
    )?;
    assert!(
        old_owner.upgrade().is_none(),
        "retirement cannot retain the unit animation callback owner"
    );
    let retired = frame
        .placements
        .iter()
        .filter(|placement| placement.retirement.is_some())
        .collect::<Vec<_>>();
    assert_eq!(retired.len(), 9);
    for (index, placement) in retired.iter().enumerate() {
        assert_eq!(placement.source_index, old_sources[index]);
        assert!(placement.unit_animation.is_none());
        assert!(placement.entity_opacity.is_none());
        assert_eq!(
            placement
                .particles
                .iter()
                .map(|particle| particle.simulation.particles().len())
                .sum::<usize>(),
            effects[index].0
        );
    }
    advance(&mut frame, &renderer, camera, 2200., &mut random)?;
    assert!(
        frame
            .visible_draws
            .iter()
            .any(|draw| (draw.material().alpha() - 0.5).abs() < 0.000001)
    );
    let mut attached = 0;
    for placement in &frame.placements {
        let Some(retired) = &placement.retirement else {
            continue;
        };
        assert!((retired.opacity() - 0.5).abs() < 0.000001);
        assert!(
            placement.transform.w_axis.y.abs() < 10.,
            "old child must not attach to the new GUID's body"
        );
        assert_eq!(placement.last_effect_time_ms, 2200);
        attached += usize::from(retired.parent().is_some());
    }
    assert_eq!(attached, 8);
    advance(&mut frame, &renderer, camera, 3200., &mut random)?;
    assert_eq!(
        frame
            .placements
            .iter()
            .filter(|placement| placement.retirement.is_some())
            .count(),
        9,
        "native exact endpoint remains resident"
    );
    advance(&mut frame, &renderer, camera, 3201., &mut random)?;
    assert!(
        frame
            .placements
            .iter()
            .all(|placement| placement.retirement.is_none())
    );
    assert_eq!(frame.placements.len(), 9);
    assert!(frame.sources.iter().all(Option::is_some));
    Ok(())
}
