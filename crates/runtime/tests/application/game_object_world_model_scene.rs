//! Real GPU pixels and retained CPU clocks for replicated WMO attachments.

use super::*;
use glam::{Mat4, Vec4};
use solarity_rendering::{
    M2CameraEffectScale, M2LocalLightState, M2SceneUniform, TerrainSceneUniform, WorldCamera,
    WorldCameraFrame, WorldFrameScene, WorldFrustum, WorldModelSceneUniform, WorldScreenWindow,
};

#[test]
fn replicated_wmo_doodads_share_gpu_sources_and_keep_cpu_timers_through_parent_changes()
-> Result<(), Box<dyn Error>> {
    let _sdl_guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let fixture = ClientFixture::with_common_files(&[
        (
            "World\\GameObject.m2",
            &models::model_with_animations(&[0])?,
        ),
        ("World\\GameObject00.skin", &models::skin()?),
        ("World\\Attached.wmo", &root()),
        ("World\\Attached_000.wmo", &group()),
        ("DBFilesClient\\GameObjectDisplayInfo.dbc", &displays()),
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let displays = GameObjectDisplayCatalog::load(&mut store)?;
    let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
    let mut objects =
        RuntimeGameObjectPresentation::new(AssetStoreHandle::new(store), displays, animations);
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Attached",
        Vec3::ZERO,
        0.,
    ));
    add_root(&mut world, 90)?;
    world.update_game_object_movement(
        90,
        GameObjectMovement::new(
            0,
            Some(GameObjectTransport {
                guid: 99,
                position: Vec3::ZERO,
                orientation: 0.,
            }),
        ),
    )?;
    objects.synchronize(Some(&world))?;
    let mut random = CrtRand::new();
    objects
        .frame_input(Some(&world))
        .advance_scene(2000., &mut random)?;
    objects.synchronize_animations(Some(&world), &mut random)?;
    let identity = world.object_identity(90).ok_or("root lifetime")?;
    let state = objects
        .frame_input(Some(&world))
        .get(identity)
        .and_then(|instance| instance.world_model_state())
        .ok_or("CPU root state missing")?;
    let first = state.playback(1).ok_or("referenced doodad timer missing")?;
    let second = state.playback(0).ok_or("referenced doodad timer missing")?;
    assert!(!std::rc::Rc::ptr_eq(&first, &second));
    assert_eq!(first.borrow().cycle_started_ms, 2001.);
    assert!(
        state.playback(2).is_none(),
        "unreferenced missing M2 must not load or start a timer"
    );
    let mut expected_random = CrtRand::new();
    for _ in 0..4 {
        let _ = expected_random.next_u15();
    }
    assert_eq!(
        random, expected_random,
        "one variation and one cycle roll per referenced owner, before GPU placement"
    );
    let platform = SdlPlatform::start(WindowConfiguration::new(128, 128, WindowMode::Windowed))?;
    let mut renderer = renderer(&platform)?;
    let mut frame = M2Frame::prepare(
        &mut renderer,
        &ResidentM2Scene::default(),
        Arc::clone(objects.frame_input(Some(&world)).animations()),
        &mut random,
        Arc::new(M2ParticleTwinkleTable::new(1)),
    )?;
    frame.synchronize_game_objects(
        &mut renderer,
        objects.frame_input(Some(&world)),
        &mut random,
    )?;
    assert!(
        frame.placements.is_empty(),
        "unresolved parent cannot publish a placed mesh"
    );
    let parent = world.create_object(
        99,
        ObjectKind::GameObject,
        Some(WorldTransform::new(Vec3::ZERO, 0.)),
        [],
    )?;
    world
        .storage_mut()
        .add_component(parent, (ObjectPresentation::new(0, 1.),));
    objects.synchronize(Some(&world))?;
    objects.synchronize_animations(Some(&world), &mut random)?;
    frame.synchronize_game_objects(
        &mut renderer,
        objects.frame_input(Some(&world)),
        &mut random,
    )?;
    assert_eq!(frame.placements.len(), 2);
    assert_eq!(frame.sources.len(), 1);
    assert_eq!(
        random, expected_random,
        "GPU admission borrows the CPU timer without another roll"
    );
    assert!(matches!(
        frame.placements[0].owner,
        M2GpuPlacementOwner::GameObjectWorldModelDoodad {
            doodad_index: 1,
            ..
        }
    ));
    assert!(matches!(
        frame.placements[1].owner,
        M2GpuPlacementOwner::GameObjectWorldModelDoodad {
            doodad_index: 0,
            ..
        }
    ));
    let mesh = frame.sources[0]
        .as_ref()
        .ok_or("GPU source")?
        .mesh
        .ok_or("triangle mesh")?;
    let child_transform = frame.placements[0].local_transform;
    assert_eq!(frame.placements[0].transform, child_transform);
    assert_eq!(frame.placements[0].color, [255, 96, 32, 255]);
    assert_eq!(frame.placements[1].color, [32, 96, 255, 255]);
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
    let initial = capture(
        &mut frame,
        &mut renderer,
        objects.frame_input(Some(&world)),
        camera,
        2500.,
        &mut random,
    )?;
    if let Some(path) = std::env::var_os("SOLARITY_WMO_DOODAD_CAPTURE") {
        std::fs::write(path, initial.rgba8())?;
    }
    assert_unlit_doodad(&initial, camera, Vec3::new(0., 2., 0.))?;
    assert_unlit_doodad(&initial, camera, Vec3::new(0., -2., 0.))?;
    let event_scene_time = first.borrow().previous_event_scene_time_ms;
    assert_eq!(event_scene_time, 2500);
    let random_before_motion = random;
    world.update_transform(99, WorldTransform::new(Vec3::Y * 0.5, 0.))?;
    objects.synchronize(Some(&world))?;
    objects.synchronize_animations(Some(&world), &mut random)?;
    frame.synchronize_game_objects(
        &mut renderer,
        objects.frame_input(Some(&world)),
        &mut random,
    )?;
    frame.update_game_object_states(objects.frame_input(Some(&world)), 2500., &mut random)?;
    assert_eq!(
        frame.placements[0].transform,
        Mat4::from_translation(Vec3::Y * 0.5) * child_transform
    );
    assert_eq!(frame.sources[0].as_ref().ok_or("source")?.mesh, Some(mesh));
    assert_eq!(
        first.borrow().previous_event_scene_time_ms,
        event_scene_time
    );
    assert_eq!(random, random_before_motion);
    let moved = capture(
        &mut frame,
        &mut renderer,
        objects.frame_input(Some(&world)),
        camera,
        2600.,
        &mut random,
    )?;
    assert_unlit_doodad(&moved, camera, Vec3::new(0., 2.5, 0.))?;
    world.remove_object(99)?;
    objects.synchronize(Some(&world))?;
    objects.synchronize_animations(Some(&world), &mut random)?;
    frame.synchronize_game_objects(
        &mut renderer,
        objects.frame_input(Some(&world)),
        &mut random,
    )?;
    frame.update_game_object_states(objects.frame_input(Some(&world)), 2700., &mut random)?;
    assert_eq!(frame.placements.len(), 2);
    assert!(
        frame
            .placements
            .iter()
            .all(|placement| !placement.placement_valid)
    );
    let hidden = frame.prepare_visible_draws(
        &renderer,
        WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
        camera,
        solarity_rendering::M2TransparentPass::One,
        Vec3::ZERO,
        2700.,
        M2CameraEffectScale::EXTERNAL_CAMERA,
        &mut random,
        Some(objects.frame_input(Some(&world))),
    )?;
    assert!(hidden.draws.is_empty());
    assert!(hidden.particle_draws.is_empty());
    assert!(hidden.ribbon_draws.is_empty());
    let parent = world.create_object(
        99,
        ObjectKind::GameObject,
        Some(WorldTransform::new(Vec3::ZERO, 0.)),
        [],
    )?;
    world
        .storage_mut()
        .add_component(parent, (ObjectPresentation::new(0, 1.),));
    objects.synchronize(Some(&world))?;
    objects.synchronize_animations(Some(&world), &mut random)?;
    frame.synchronize_game_objects(
        &mut renderer,
        objects.frame_input(Some(&world)),
        &mut random,
    )?;
    frame.update_game_object_states(objects.frame_input(Some(&world)), 2800., &mut random)?;
    assert_eq!(frame.sources[0].as_ref().ok_or("source")?.mesh, Some(mesh));
    assert!(std::rc::Rc::ptr_eq(
        &state,
        &frame.placements[0]
            .world_model_state
            .clone()
            .ok_or("state")?
    ));
    let restored = capture(
        &mut frame,
        &mut renderer,
        objects.frame_input(Some(&world)),
        camera,
        2800.,
        &mut random,
    )?;
    assert_unlit_doodad(&restored, camera, Vec3::new(0., 2., 0.))?;
    objects
        .frame_input(Some(&world))
        .advance_scene(3300., &mut random)?;
    let neighbor_random = random;
    let retained_event_scene_time = first.borrow().previous_event_scene_time_ms;
    add_root(&mut world, 91)?;
    objects.synchronize(Some(&world))?;
    objects.synchronize_animations(Some(&world), &mut random)?;
    let neighbor = objects
        .frame_input(Some(&world))
        .get(world.object_identity(91).ok_or("neighbor lifetime")?)
        .and_then(|instance| instance.world_model_state())
        .ok_or("neighbor CPU state")?;
    assert!(!std::rc::Rc::ptr_eq(&state, &neighbor));
    let neighbor_timer = neighbor.playback(1).ok_or("neighbor timer")?;
    assert!(!std::rc::Rc::ptr_eq(&first, &neighbor_timer));
    assert_eq!(neighbor_timer.borrow().cycle_started_ms, 3301.);
    assert_eq!(first.borrow().cycle_started_ms, 2001.);
    assert_eq!(
        first.borrow().previous_event_scene_time_ms,
        retained_event_scene_time
    );
    let mut expected_neighbor_random = neighbor_random;
    for _ in 0..4 {
        let _ = expected_neighbor_random.next_u15();
    }
    assert_eq!(random, expected_neighbor_random);
    frame.synchronize_game_objects(
        &mut renderer,
        objects.frame_input(Some(&world)),
        &mut random,
    )?;
    assert_eq!(frame.placements.len(), 4);
    assert_eq!(frame.sources.len(), 1);
    assert_eq!(frame.sources[0].as_ref().ok_or("source")?.mesh, Some(mesh));
    assert_eq!(random, expected_neighbor_random);
    world.remove_object(91)?;
    world.remove_object(90)?;
    add_root(&mut world, 90)?;
    objects.synchronize(Some(&world))?;
    objects.synchronize_animations(Some(&world), &mut random)?;
    frame.synchronize_game_objects(
        &mut renderer,
        objects.frame_input(Some(&world)),
        &mut random,
    )?;
    assert_eq!(frame.placements.len(), 2);
    assert!(!std::rc::Rc::ptr_eq(
        &state,
        &frame.placements[0]
            .world_model_state
            .clone()
            .ok_or("state")?
    ));
    assert_ne!(world.object_identity(90), Some(identity));
    objects.disconnect();
    frame.synchronize_game_objects(
        &mut renderer,
        objects.frame_input(Some(&world)),
        &mut random,
    )?;
    assert!(frame.placements.is_empty());
    assert!(frame.sources.is_empty());
    renderer.shutdown()?;
    Ok(())
}

fn capture(
    frame: &mut M2Frame,
    renderer: &mut VulkanRenderer,
    objects: super::super::GameObjectFrameInput<'_>,
    camera: WorldCameraFrame,
    time: f32,
    random: &mut CrtRand,
) -> Result<solarity_rendering::CapturedFrame, Box<dyn Error>> {
    let visible = frame.prepare_visible_draws(
        renderer,
        WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
        camera,
        solarity_rendering::M2TransparentPass::One,
        Vec3::ZERO,
        time,
        M2CameraEffectScale::EXTERNAL_CAMERA,
        random,
        Some(objects),
    )?;
    let fog = Vec4::new(90., 100., 0., 1.);
    let scene = WorldFrameScene::new(
        TerrainSceneUniform::new(camera.view_projection(), Vec3::ONE, Vec3::ZERO, Vec3::Z),
        WorldModelSceneUniform::new(
            camera.view_projection(),
            camera.camera().position(),
            Vec3::ONE,
            Vec3::ZERO,
            Vec3::Z,
            fog,
        ),
        M2SceneUniform::new(
            camera.view_projection(),
            camera.view(),
            camera.camera().position(),
            Vec3::ONE,
            Vec3::ZERO,
            Vec3::Z,
            fog,
            Vec3::ZERO,
            [M2LocalLightState::disabled(); 4],
        ),
    );
    renderer.request_frame_capture()?;
    renderer.present_world_frame(
        scene,
        visible.bone_transforms,
        &[],
        &[],
        visible.draws,
        visible.particle_vertices,
        visible.particle_indices,
        visible.particle_draws,
        visible.ribbon_vertices,
        visible.ribbon_draws,
    )?;
    renderer
        .take_captured_frame()?
        .ok_or_else(|| "missing captured frame".into())
}

fn assert_unlit_doodad(
    capture: &solarity_rendering::CapturedFrame,
    camera: WorldCameraFrame,
    point: Vec3,
) -> Result<(), Box<dyn Error>> {
    let clip = camera.view_projection() * point.extend(1.);
    let x = ((clip.x / clip.w * 0.5 + 0.5) * 128.) as usize;
    let y = ((0.5 - clip.y / clip.w * 0.5) * 128.) as usize;
    let pixel = capture
        .rgba8()
        .get((y * 128 + x) * 4..)
        .and_then(|bytes| bytes.get(..4))
        .ok_or("pixel")?;
    // MODD colors belong to the lighting callback; this unlit fixture keeps
    // its white mesh while parent movement still determines the exact pixels.
    assert_eq!(pixel, &[255; 4], "unlit doodad missing at {point:?}");
    Ok(())
}

fn add_root(world: &mut ActiveWorld, guid: u64) -> Result<(), Box<dyn Error>> {
    let entity = world.create_object(
        guid,
        ObjectKind::GameObject,
        Some(WorldTransform::new(Vec3::ZERO, 0.)),
        [],
    )?;
    world.storage_mut().add_component(
        entity,
        (
            ObjectPresentation::new(1, 1.),
            GameObjectPresentation::from_fields(42, 0, u32::from_le_bytes([1, 11, 0, 0])),
        ),
    );
    Ok(())
}

fn chunk(bytes: &mut Vec<u8>, tag: &[u8; 4], payload: &[u8]) {
    bytes.extend_from_slice(tag);
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(payload);
}
fn word(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn root() -> Vec<u8> {
    let mut bytes = Vec::new();
    chunk(&mut bytes, b"REVM", &17_u32.to_le_bytes());
    let mut header = [0; 64];
    word(&mut header, 4, 1);
    word(&mut header, 16, 2);
    word(&mut header, 20, 3);
    word(&mut header, 24, 1);
    for (slot, value) in [-4_f32, -4., -4., 4., 4., 4.].iter().enumerate() {
        word(&mut header, 36 + slot * 4, value.to_bits());
    }
    chunk(&mut bytes, b"DHOM", &header);
    let mut info = [0; 32];
    info[4..28].copy_from_slice(&header[36..60]);
    word(&mut info, 28, u32::MAX);
    chunk(&mut bytes, b"IGOM", &info);
    let paths = b"World\\GameObject.m2\0World\\Missing.m2\0";
    chunk(&mut bytes, b"NDOM", paths);
    let missing = b"World\\GameObject.m2\0".len();
    let mut records = Vec::new();
    for (index, (y, color)) in [
        // MODD stores BGRA: red at -Y and blue at +Y.
        (-2_f32, [32, 96, 255, 255]),
        (2., [255, 96, 32, 255]),
        (0., [255; 4]),
    ]
    .into_iter()
    .enumerate()
    {
        records.extend_from_slice(&(if index == 2 { missing as u32 } else { 0_u32 }).to_le_bytes());
        for value in [0., y, 0., 0., 0., 0., 1., 1.] {
            records.extend_from_slice(&value.to_le_bytes());
        }
        records.extend_from_slice(&color);
    }
    chunk(&mut bytes, b"DDOM", &records);
    let mut set = [0; 32];
    set[..18].copy_from_slice(b"Set_$DefaultGlobal");
    word(&mut set, 24, 3);
    chunk(&mut bytes, b"SDOM", &set);
    bytes
}
fn group() -> Vec<u8> {
    let mut bytes = Vec::new();
    chunk(&mut bytes, b"REVM", &17_u32.to_le_bytes());
    let mut header = vec![0; 68];
    for (slot, value) in [-4_f32, -4., -4., 4., 4., 4.].iter().enumerate() {
        word(&mut header, 12 + slot * 4, value.to_bits());
    }
    chunk(&mut header, b"RDOM", &[1, 0, 0, 0, 1, 0]);
    chunk(&mut bytes, b"PGOM", &header);
    bytes
}
fn displays() -> Vec<u8> {
    let path = b"\0World\\Attached.wmo\0";
    let mut bytes = b"WDBC".to_vec();
    for value in [1_u32, 19, 76, path.len() as u32] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    let mut record = [0_u8; 76];
    word(&mut record, 0, 42);
    word(&mut record, 4, 1);
    for (slot, value) in [-4_f32, -4., -4., 4., 4., 4.].iter().enumerate() {
        word(&mut record, 48 + slot * 4, value.to_bits());
    }
    bytes.extend(record);
    bytes.extend(path);
    bytes
}
