//! Registered units and attached effects use owner fog through portal transitions.

use super::*;

#[test]
fn registered_owner_fog_reaches_meshes_particles_and_attachments() -> Result<(), Box<dyn Error>> {
    for moving in [false, true] {
        verify(moving)?;
    }
    Ok(())
}

#[allow(unsafe_code)] // The hidden test window transfers surface ownership to Vulkan.
fn verify(moving: bool) -> Result<(), Box<dyn Error>> {
    let (_, floor, wdt, map) = fixture_files();
    let (root, outside, inside) = rooms(floor)?;
    let model_bytes = model_with_attached_particles()?;
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
    let liquids = solarity_asset::LiquidTypeCatalog::load(&mut store)?;
    let displays = GameObjectDisplayCatalog::load(&mut store)?;
    let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
    let model = Arc::new(DecodedM2Model::load(
        &mut store,
        &AssetPath::new("Receiver.m2")?,
    )?);
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
        .window("Solarity registered model fog", 128, 128)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: The live window outlives the renderer's sole ownership of its surface.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (128, 128), 0) }?;
    let mut random = CrtRand::new();
    objects
        .frame_input(Some(&world))
        .advance_scene(0., &mut random)?;
    objects.synchronize_animations(Some(&world), &mut random)?;
    let empty = ResidentM2Scene::default();
    let resident = if moving {
        &empty
    } else {
        terrain.resident_m2_scene().map_or(&empty, Arc::as_ref)
    };
    let mut frame = M2Frame::prepare(
        &mut renderer,
        resident,
        Arc::clone(&animations),
        &mut random,
        Arc::new(M2ParticleTwinkleTable::new(1)),
    )?;
    frame.synchronize_game_objects(
        &mut renderer,
        objects.frame_input(Some(&world)),
        &mut random,
    )?;
    assert_eq!(frame.sources.len(), 1);
    let initial_transform = frame.placements[0].transform * Mat4::from_translation(-POSITIONS[0]);
    frame.placements.clear();
    // Mounts publish the saddle pose before their attached rider.
    for owner in [
        M2GpuPlacementOwner::CreatureMount { guid: 1 },
        M2GpuPlacementOwner::CreatureBody { guid: 1 },
        M2GpuPlacementOwner::CreatureBody { guid: 2 },
    ] {
        let playback = M2Playback::default_sequence(&model, &animations, 0, &mut random)?;
        frame.placements.push(m2_gpu_placement(
            0,
            Mat4::IDENTITY,
            owner,
            &model,
            Some(M2PlaybackStorage::Local(playback)),
            None,
            0,
        )?);
    }
    frame.placement_topology_dirty = true;
    let mut root_transform = initial_transform;
    for step in 0..5 {
        if moving && step == 3 {
            let translation = Vec3::new(35., -17., 4.);
            world.update_transform(90, WorldTransform::new(translation, 0.7))?;
            objects.synchronize(Some(&world))?;
            terrain.synchronize_game_object_movement(
                Some(&world),
                &objects,
                MovementBspCacheMode::Enabled,
            )?;
            root_transform = Mat4::from_translation(translation)
                * Mat4::from_rotation_z(0.7)
                * initial_transform;
        }
        let root_position = if matches!(step, 2 | 3) {
            Vec3::new(-8., 0.5, 0.)
        } else {
            Vec3::new(0.5, 0.5, 0.)
        };
        let positions = [
            root_position,
            root_position + Vec3::new(-8., 0., 3.),
            Vec3::new(-8., -4.5, 0.),
        ];
        for (index, placement) in frame.placements.iter_mut().enumerate() {
            let position = if index == 2 {
                positions[2]
            } else {
                root_position
            };
            let transform = root_transform * Mat4::from_translation(position);
            placement.local_transform = transform;
            placement.transform = transform;
            placement.scene_registration = Some(UnitSceneRegistration::new(&model, transform)?);
        }
        let (eye, target) = if matches!(step, 0 | 3) {
            (Vec3::new(-30., 0., 1.), Vec3::new(1., 0., 1.))
        } else {
            (Vec3::new(3., 1., 0.5), Vec3::new(-30., 1., 0.5))
        };
        let camera = WorldCamera::stock(
            root_transform.transform_point3(eye),
            root_transform.transform_point3(target),
            Vec3::Z,
            100.,
        )
        .frame(1.)?;
        let colors = [
            Vec3::new(32., 80. + step as f32 * 8., 176.),
            Vec3::new(224., 64., 32. + step as f32 * 8.),
        ]
        .map(|value| value / 255.);
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
        // Retain live particle history across each camera/owner transition.
        for tick in 0..2 {
            let visible = frame.prepare_visible_draws_with_unit_effects(
                &renderer,
                WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
                camera,
                M2TransparentPass::One,
                colors[1],
                (step * 2 + tick + 1) as f32 * 200.,
                M2CameraEffectScale::EXTERNAL_CAMERA,
                &mut random,
                None,
                None,
                Some((
                    base,
                    M2DirectionalLight::new(-Vec3::Z, Vec3::ONE, Vec3::ZERO),
                )),
                Some((&mut terrain, environment, colors[0], &liquids)),
                None,
                None,
            )?;
            if tick == 0 {
                continue;
            }
            assert_eq!(visible.draws.len(), 3, "moving={moving}, step={step}");
            assert_eq!(visible.particle_draws.len(), 3);
            assert!(!visible.particle_vertices.is_empty(), "live particle cards");
            let expected = [
                colors[usize::from(step != 0)],
                colors[usize::from(step != 0)],
                colors[0],
            ];
            let mut scenes = std::collections::HashMap::new();
            for draw in visible.draws {
                let bytes = draw.material().to_bytes();
                let position = root_transform.inverse().transform_point3(Vec3::new(
                    read_float(&bytes, 48),
                    read_float(&bytes, 52),
                    read_float(&bytes, 56),
                ));
                let index = positions
                    .iter()
                    .position(|point| point.abs_diff_eq(position, 0.001))
                    .ok_or("registered owner position")?;
                let scene_index = draw.scene_index().ok_or("owner scene")?;
                let uniform = visible.instance_scenes[scene_index as usize].to_bytes();
                let actual = Vec3::new(
                    read_float(&uniform, 144),
                    read_float(&uniform, 148),
                    read_float(&uniform, 152),
                );
                assert!(
                    actual.abs_diff_eq(expected[index], 1e-7),
                    "moving={moving}, step={step}, owner={index}: {actual:?} != {:?}",
                    expected[index]
                );
                scenes.insert(scene_index, expected[index]);
            }
            for particle in visible.particle_draws {
                assert!(
                    scenes.contains_key(&particle.scene_index().ok_or("particle owner scene")?)
                );
                assert!(particle.index_count() > 0);
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
                &[],
                &[],
            )?;
            let capture = renderer.take_captured_frame()?.ok_or("capture")?;
            for (index, position) in positions.into_iter().enumerate() {
                let clip =
                    camera.view_projection() * root_transform.transform_point3(position).extend(1.);
                let x = ((clip.x / clip.w * 0.5 + 0.5) * 128.) as usize;
                let y = ((0.5 - clip.y / clip.w * 0.5) * 128.) as usize;
                let pixel = &capture.rgba8()[(y * 128 + x) * 4..][..3];
                assert!(
                    pixel
                        .iter()
                        .zip((expected[index] * 255.).to_array())
                        .all(|(&actual, expected)| (f32::from(actual) - expected).abs() <= 1.),
                    "moving={moving}, step={step}, owner={index}, pixel={pixel:?}"
                );
            }
        }
    }
    renderer.shutdown()?;
    Ok(())
}

fn model_with_attached_particles() -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = game_object_models::model_with_animations(&[0])?;
    let material = u32::from_le_bytes(bytes[0x74..0x78].try_into()?) as usize;
    bytes[material..material + 2].copy_from_slice(&5_u16.to_le_bytes()); // unlit, two-sided, fogged
    let bone = bytes.len();
    bytes.resize(bone + 88, 0);
    put(&mut bytes, bone, u32::MAX);
    bytes[bone + 8..bone + 10].copy_from_slice(&u16::MAX.to_le_bytes());
    for offset in [18, 38, 58] {
        bytes[bone + offset..bone + offset + 2].copy_from_slice(&u16::MAX.to_le_bytes());
    }
    array(&mut bytes, 0x2c, 1, bone);
    let attachment = bytes.len();
    bytes.resize(attachment + 40, 0);
    put(&mut bytes, attachment + 8, (-8_f32).to_bits());
    put(&mut bytes, attachment + 16, 3_f32.to_bits());
    track(&mut bytes, attachment + 20, &[0], &[1], 1, 0);
    array(&mut bytes, 0xf0, 1, attachment);
    let lookup = bytes.len();
    bytes.extend(0_u16.to_le_bytes());
    array(&mut bytes, 0xf8, 1, lookup);
    crate::test_support::unit_models::append_effects(&mut bytes, 1);
    put(&mut bytes, 0x120, 0); // This fixture exercises the fogged particle path.
    let particle = u32::from_le_bytes(bytes[0x12c..0x130].try_into()?) as usize;
    put(&mut bytes, particle + 4, 1); // Unlit, fogged cards.
    for offset in [0x164, 0x168, 0x16c] {
        put(&mut bytes, particle + offset, 1_f32.to_bits());
    }
    for (offset, values) in [
        (0x104, floats(&[1., 1., 1.])),
        (0x114, i16::MAX.to_le_bytes().to_vec()),
        (0x124, floats(&[0.25, 0.25])),
    ] {
        let time = bytes.len();
        bytes.extend(0_u16.to_le_bytes());
        let value = bytes.len();
        bytes.extend(values);
        array(&mut bytes, particle + offset, 1, time);
        array(&mut bytes, particle + offset + 8, 1, value);
    }
    Ok(bytes)
}
