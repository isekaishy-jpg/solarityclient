//! Admitted exterior/portal doodads reach real pixels for static and moving WMOs.

#[path = "model_owner_fog_frame.rs"]
mod owner_fog_frame;

use super::*;
use crate::application::game_object_coordinator::RuntimeGameObjectPresentation;
use solarity_asset::GameObjectDisplayCatalog;
use solarity_ecs::{GameObjectPresentation, ObjectKind, ObjectPresentation, WorldTransform};
use solarity_rendering::{TerrainSceneUniform, WorldFrameScene, WorldModelSceneUniform};
use solarity_systems::MovementBspCacheMode;

const POSITIONS: [Vec3; 3] = [
    Vec3::new(-8., -2., 0.),
    Vec3::new(-1., 0., 0.),
    Vec3::new(-1., 12., 0.),
];

#[test]
fn wmo_doodad_portal_visibility_and_fog_reach_static_and_moving_pixels()
-> Result<(), Box<dyn Error>> {
    for moving in [false, true] {
        for publishes_light in [false, true] {
            verify(moving, publishes_light)?;
        }
    }
    Ok(())
}

#[allow(unsafe_code)] // The hidden test window transfers its surface to Vulkan.
fn verify(moving: bool, publishes_light: bool) -> Result<(), Box<dyn Error>> {
    let (_, floor, wdt, map) = fixture_files();
    let (root, outside, inside) = rooms(floor)?;
    let mut model_bytes = game_object_models::model_with_animations(&[0])?;
    let material = u32::from_le_bytes(model_bytes[0x74..0x78].try_into()?) as usize;
    model_bytes[material..material + 2].copy_from_slice(&5_u16.to_le_bytes()); // unlit, two-sided, fogged
    if publishes_light {
        add_point_light(&mut model_bytes);
    }
    let fixture = ClientFixture::with_common_files(&[
        ("DBFilesClient\\Map.dbc", &map),
        ("DBFilesClient\\GameObjectDisplayInfo.dbc", &display()),
        ("World\\Maps\\Light\\Light.wdt", &wdt),
        ("World\\Light.wmo", &root),
        ("World\\Light_000.wmo", &outside),
        ("World\\Light_001.wmo", &inside),
        ("Receiver.m2", &model_bytes),
        ("Receiver00.skin", &game_object_models::skin()?),
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let maps = MapCatalog::load(&mut store)?;
    let liquid_types = solarity_asset::LiquidTypeCatalog::load(&mut store)?;
    let displays = GameObjectDisplayCatalog::load(&mut store)?;
    let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
    let store = AssetStoreHandle::new(store);
    let mut objects =
        RuntimeGameObjectPresentation::new(store.clone(), displays, Arc::clone(&animations));
    let mut terrain = RuntimeTerrainCoordinator::new(store, maps);
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        1,
        "Light",
        Vec3::ZERO,
        0.,
    ));
    if moving {
        let entity = world.create_object(
            90,
            ObjectKind::GameObject,
            Some(WorldTransform::new(Vec3::ZERO, 0.)),
            [],
        )?;
        world.storage_mut().add_component(
            entity,
            (
                ObjectPresentation::new(1, 1.),
                GameObjectPresentation::from_fields(42, 0, u32::from_le_bytes([1, 35, 0, 0])),
            ),
        );
    }
    terrain.synchronize(Some(&world))?;
    objects.synchronize(Some(&world))?;
    terrain.synchronize_game_object_movement(
        Some(&world),
        &objects,
        MovementBspCacheMode::Enabled,
    )?;
    let _guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL lock poisoned")?;
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity WMO doodad scene", 128, 128)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: The live window owns this surface until ownership transfers below.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (128, 128), 0) }?;
    let mut random = CrtRand::new();
    objects
        .frame_input(Some(&world))
        .advance_scene(0., &mut random)?;
    objects.synchronize_animations(Some(&world), &mut random)?;
    let empty = ResidentM2Scene::default();
    // The global fixture root is far from the replicated root at world zero.
    // Publish only the owner exercised by this pass into the model frame.
    let resident = if moving {
        &empty
    } else {
        terrain.resident_m2_scene().map_or(&empty, Arc::as_ref)
    };
    let mut frame = M2Frame::prepare(
        &mut renderer,
        resident,
        animations,
        &mut random,
        Arc::new(M2ParticleTwinkleTable::new(1)),
    )?;
    frame.synchronize_game_objects(
        &mut renderer,
        objects.frame_input(Some(&world)),
        &mut random,
    )?;
    assert_eq!(frame.placements.len(), 3);
    assert_eq!(frame.sources.len(), 1);
    for step in 0..3 {
        if moving && step == 2 {
            world.update_transform(90, WorldTransform::new(Vec3::new(35., -17., 4.), 0.7))?;
            objects.synchronize(Some(&world))?;
            terrain.synchronize_game_object_movement(
                Some(&world),
                &objects,
                MovementBspCacheMode::Enabled,
            )?;
            frame.update_game_object_states(
                objects.frame_input(Some(&world)),
                300.,
                &mut random,
            )?;
        }
        let root_transform = frame
            .placements
            .iter()
            .find(|placement| {
                doodad_scene::owner_key(placement.owner).is_some_and(|(_, index)| index == 0)
            })
            .ok_or("exterior doodad")?
            .transform
            * Mat4::from_translation(-POSITIONS[0]);
        let (eye, target) = if step == 1 {
            (Vec3::new(1., 1., 1.), Vec3::new(-30., 1., 1.))
        } else {
            (Vec3::new(-30., 0., 1.), Vec3::new(0., 0., 1.))
        };
        let camera = WorldCamera::stock(
            root_transform.transform_point3(eye),
            root_transform.transform_point3(target),
            Vec3::Z,
            100.,
        )
        .frame(1.)?;
        let colors = if step == 1 {
            [Vec3::new(16., 96., 176.), Vec3::new(240., 64., 32.)]
        } else {
            [Vec3::new(32., 64., 96.), Vec3::new(192., 128., 32.)]
        }
        .map(|color| color / 255.);
        let fog = Vec4::new(0., 1., 0., 1.);
        let base = M2SceneUniform::new(
            camera.projection(),
            camera.view(),
            camera.camera().position(),
            Vec3::ONE,
            Vec3::ZERO,
            Vec3::Z,
            fog,
            colors[1],
            [M2LocalLightState::disabled(); 4],
        );
        let environment = solarity_systems::WorldEntityLightEnvironment::new(
            Vec3::ONE,
            Vec3::ZERO,
            -Vec3::Z,
            -Vec3::Z,
        );
        let visible = frame.prepare_visible_draws_with_unit_effects(
            &renderer,
            WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
            camera,
            M2TransparentPass::One,
            colors[1],
            (step + 1) as f32 * 100.,
            M2CameraEffectScale::EXTERNAL_CAMERA,
            &mut random,
            Some(objects.frame_input(Some(&world))),
            None,
            Some((
                base,
                solarity_rendering::M2DirectionalLight::new(-Vec3::Z, Vec3::ONE, Vec3::ZERO),
            )),
            Some((&mut terrain, environment, colors[0], &liquid_types)),
            None,
            None,
        )?;
        assert_eq!(visible.draws.len(), 2, "moving {moving}, step {step}");
        assert_eq!(
            visible.scene_points.points().len(),
            if publishes_light { 3 } else { 0 },
            "the hidden WMO doodad still publishes its light"
        );
        let expected_colors = [colors[0], colors[usize::from(step == 1)]];
        for (draw, expected) in visible.draws.iter().zip(expected_colors) {
            let scene = visible.instance_scenes[draw.scene_index().ok_or("doodad scene")? as usize]
                .to_bytes();
            let actual = Vec3::new(
                read_float(&scene, 144),
                read_float(&scene, 148),
                read_float(&scene, 152),
            );
            assert!(
                actual.abs_diff_eq(expected, 1.0e-7),
                "particle scene inherits model fog: moving {moving}, step {step}, actual {actual:?}, expected {expected:?}"
            );
        }
        let scene = WorldFrameScene::new(
            TerrainSceneUniform::new(
                camera.projection(),
                camera.view(),
                Vec3::ONE,
                Vec3::ZERO,
                Vec3::Z,
            ),
            WorldModelSceneUniform::new(
                camera.projection(),
                camera.view(),
                camera.camera().position(),
                Vec3::ONE,
                Vec3::ZERO,
                Vec3::Z,
                fog,
            ),
            base,
        )
        .with_m2_instance_scenes(visible.instance_scenes);
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
        let capture = renderer.take_captured_frame()?.ok_or("capture")?;
        for (index, color) in expected_colors.into_iter().enumerate() {
            let point = root_transform.transform_point3(POSITIONS[index]);
            let clip = camera.view_projection() * point.extend(1.);
            let x = ((clip.x / clip.w * 0.5 + 0.5) * 128.) as usize;
            let y = ((0.5 - clip.y / clip.w * 0.5) * 128.) as usize;
            let pixel = &capture.rgba8()[(y * 128 + x) * 4..][..3];
            for (actual, expected) in pixel.iter().zip((color * 255.).to_array()) {
                assert!(
                    (f32::from(*actual) - expected).abs() <= 1.,
                    "moving {moving}, step {step}, doodad {index}, pixel {pixel:?}"
                );
            }
        }
        for (index, placement) in frame.placements.iter().enumerate() {
            let (_, doodad) = doodad_scene::owner_key(placement.owner).ok_or("owner")?;
            assert_eq!(
                frame.doodad_scene.fog_bank(index),
                match doodad {
                    0 => Some(false),
                    1 => Some(step == 1),
                    _ => None,
                }
            );
            assert_eq!(
                placement.last_effect_time_ms,
                if doodad == 2 { 0 } else { (step + 1) * 100 }
            );
        }
    }
    if moving {
        world.remove_object(90)?;
        objects.synchronize(Some(&world))?;
        terrain.synchronize_game_object_movement(
            Some(&world),
            &objects,
            MovementBspCacheMode::Enabled,
        )?;
        frame.synchronize_game_objects(
            &mut renderer,
            objects.frame_input(Some(&world)),
            &mut random,
        )?;
        assert!(frame.placements.is_empty());
        assert!(frame.sources.is_empty());
    }
    renderer.shutdown()?;
    Ok(())
}

fn read_float(bytes: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn add_point_light(model: &mut Vec<u8>) {
    let record = model.len();
    model.resize(record + 156, 0);
    model[record..record + 2].copy_from_slice(&1_u16.to_le_bytes());
    track(model, record + 0x10, &[0], &floats(&[0., 0., 0.]), 12, 0);
    track(model, record + 0x24, &[0], &floats(&[1.]), 4, 0);
    track(model, record + 0x38, &[0], &floats(&[1., 0.5, 0.25]), 12, 0);
    track(model, record + 0x4c, &[0], &floats(&[1.]), 4, 0);
    for offset in [0x60, 0x74] {
        model[record + offset + 2..record + offset + 4].copy_from_slice(&u16::MAX.to_le_bytes());
    }
    track(model, record + 0x88, &[0], &[1], 1, 0);
    array(model, 0x108, 1, record);
}

type RoomFiles = (Vec<u8>, Vec<u8>, Vec<u8>);

fn rooms(floor: Vec<u8>) -> Result<RoomFiles, Box<dyn Error>> {
    let (mut root, _, _, _) = fixture_files();
    let bounds: Vec<u8> = [-16_f32, -16., -16., 16., 16., 16.]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    let mut header = find_chunk(&root, b"DHOM")?.to_vec();
    put(&mut header, 4, 2);
    put(&mut header, 8, 1);
    put(&mut header, 20, 3);
    header[36..60].copy_from_slice(&bounds);
    replace_chunk(&mut root, b"DHOM", &header)?;
    let mut info = vec![0; 64];
    for index in 0..2 {
        let record = &mut info[index * 32..][..32];
        put(record, 0, if index == 0 { 8 } else { 0 });
        record[4..28].copy_from_slice(&bounds);
        put(record, 28, u32::MAX);
    }
    replace_chunk(&mut root, b"IGOM", &info)?;
    let mut doodads = Vec::new();
    for position in POSITIONS {
        let mut record = [0; 40];
        for (index, value) in position.to_array().into_iter().enumerate() {
            put(&mut record, 4 + index * 4, value.to_bits());
        }
        put(&mut record, 28, 1_f32.to_bits());
        put(&mut record, 32, 1_f32.to_bits());
        put(&mut record, 36, u32::MAX);
        doodads.extend(record);
    }
    replace_chunk(&mut root, b"DDOM", &doodads)?;
    let mut set = find_chunk(&root, b"SDOM")?.to_vec();
    put(&mut set, 24, 3);
    replace_chunk(&mut root, b"SDOM", &set)?;
    chunk(
        &mut root,
        b"VPOM",
        &floats(&[-3., -1., -4., -3., -1., 4., -3., 1., 4., -3., 1., -4.]),
    );
    let mut portal = [0; 20];
    portal[2..4].copy_from_slice(&4_u16.to_le_bytes());
    put(&mut portal, 4, (-1_f32).to_bits());
    put(&mut portal, 16, (-3_f32).to_bits());
    chunk(&mut root, b"TPOM", &portal);
    chunk(
        &mut root,
        b"RPOM",
        &[0_u16, 1, 1, 0, 0, 0, u16::MAX, 0]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    let mut inside = find_chunk(&floor, b"PGOM")?.to_vec();
    inside[12..36].copy_from_slice(&bounds);
    inside[36..38].copy_from_slice(&1_u16.to_le_bytes());
    inside[38..40].copy_from_slice(&1_u16.to_le_bytes());
    let mut children = inside[68..].to_vec();
    replace_chunk(&mut children, b"RDOM", &[1, 0, 2, 0])?;
    inside.truncate(68);
    inside.extend(children);
    let mut outside = inside[..68].to_vec();
    put(&mut outside, 8, 8);
    outside[36..38].fill(0);
    chunk(&mut outside, b"RDOM", &[0, 0]);
    let groups = [outside, inside].map(|header| {
        let mut bytes = Vec::new();
        chunk(&mut bytes, b"REVM", &17_u32.to_le_bytes());
        chunk(&mut bytes, b"PGOM", &header);
        bytes
    });
    let [outside, inside] = groups;
    Ok((root, outside, inside))
}

fn find_chunk<'a>(bytes: &'a [u8], tag: &[u8; 4]) -> Result<&'a [u8], Box<dyn Error>> {
    let mut at = 0;
    while at + 8 <= bytes.len() {
        let length = u32::from_le_bytes(bytes[at + 4..at + 8].try_into()?) as usize;
        if bytes[at..at + 4] == *tag {
            return Ok(&bytes[at + 8..at + 8 + length]);
        }
        at += 8 + length;
    }
    Err("missing fixture chunk".into())
}
fn replace_chunk(bytes: &mut Vec<u8>, tag: &[u8; 4], payload: &[u8]) -> Result<(), Box<dyn Error>> {
    let mut at = 0;
    while at + 8 <= bytes.len() {
        let length = u32::from_le_bytes(bytes[at + 4..at + 8].try_into()?) as usize;
        if bytes[at..at + 4] == *tag {
            bytes.splice(at + 8..at + 8 + length, payload.iter().copied());
            put(bytes, at + 4, payload.len() as u32);
            return Ok(());
        }
        at += 8 + length;
    }
    Err("missing fixture chunk".into())
}
fn put(bytes: &mut [u8], at: usize, word: u32) {
    bytes[at..at + 4].copy_from_slice(&word.to_le_bytes());
}
fn chunk(bytes: &mut Vec<u8>, tag: &[u8; 4], payload: &[u8]) {
    bytes.extend(tag);
    bytes.extend((payload.len() as u32).to_le_bytes());
    bytes.extend(payload);
}
fn display() -> Vec<u8> {
    let path = b"\0World\\Light.wmo\0";
    let mut bytes = b"WDBC".to_vec();
    bytes.extend(
        [1_u32, 19, 76, path.len() as u32]
            .into_iter()
            .flat_map(u32::to_le_bytes),
    );
    let mut record = [0; 76];
    put(&mut record, 0, 42);
    put(&mut record, 4, 1);
    bytes.extend(record);
    bytes.extend(path);
    bytes
}
