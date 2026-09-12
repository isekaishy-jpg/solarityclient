//! Registered WMO liquid reaches model pass order and actual clipped GPU pixels.

use super::*;
use solarity_rendering::{TerrainSceneUniform, WorldFrameScene, WorldModelSceneUniform};

#[test]
#[allow(unsafe_code)] // The hidden test surface transfers to the renderer.
fn registered_model_liquid_splits_translucent_meshes_without_double_blending()
-> Result<(), Box<dyn Error>> {
    let _lock = SDL_TEST_LOCK.lock().map_err(|_| "SDL lock poisoned")?;
    let (mut root, mut group, wdt, map) = fixture_files();
    put(&mut root, 20, 1);
    chunk(&mut root, b"XTOM", &[0]);
    chunk(&mut root, b"TMOM", &[0; 64]);
    // Preserve the real floor/BSP while adding a one-cell liquid at local Z=1.
    let header = 20;
    put(&mut group, header + 8, 0x1801);
    put(&mut group, header + 52, 14);
    let mut grid = Vec::new();
    for word in [2u32, 2, 1, 1, 0, 0, 0] {
        grid.extend(word.to_le_bytes());
    }
    grid.extend(0u16.to_le_bytes());
    for _ in 0..4 {
        grid.extend(0u32.to_le_bytes());
        grid.extend(1f32.to_le_bytes());
    }
    grid.push(0);
    chunk(&mut group, b"QILM", &grid);
    let size = group.len() - header;
    put(&mut group, 16, size as u32);
    let mut model_bytes = game_object_models::model_with_animations(&[0])?;
    let material = u32::from_le_bytes(model_bytes[0x74..0x78].try_into()?) as usize;
    model_bytes[material..material + 2].copy_from_slice(&7u16.to_le_bytes()); // unlit, unfogged, two-sided
    model_bytes[material + 2..material + 4].copy_from_slice(&2u16.to_le_bytes()); // alpha blend
    add_effect_routes(&mut model_bytes)?;
    add_rider_attachment(&mut model_bytes);
    let mut files = crate::test_support::liquid_models::files(0, 0, 4, 14, 0);
    files.extend([
        ("DBFilesClient\\Map.dbc".to_owned(), map),
        ("World\\Maps\\Light\\Light.wdt".to_owned(), wdt),
        ("World\\Light.wmo".to_owned(), root),
        ("World\\Light_000.wmo".to_owned(), group),
        ("Receiver.m2".to_owned(), model_bytes),
        ("Receiver00.skin".to_owned(), game_object_models::skin()?),
    ]);
    let fixture = ClientFixture::with_common_files(
        &files
            .iter()
            .map(|(path, data)| (path.as_str(), data.as_slice()))
            .collect::<Vec<_>>(),
    )?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let maps = MapCatalog::load(&mut store)?;
    let liquids = solarity_asset::LiquidTypeCatalog::load(&mut store)?;
    let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
    let model = Arc::new(DecodedM2Model::load(
        &mut store,
        &AssetPath::new("Receiver.m2")?,
    )?);
    let mut terrain = RuntimeTerrainCoordinator::new(AssetStoreHandle::new(store), maps);
    let world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(571),
        1,
        "Light",
        Vec3::ZERO,
        0.,
    ));
    terrain.synchronize(Some(&world))?;
    let root_transform = terrain.resident_m2_scene().ok_or("scene")?.placements()[0].transform();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Model liquid clipping", 128, 128)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: The window outlives the renderer, which solely owns the surface.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (128, 128), 0) }?;
    assert!(
        renderer.m2_liquid_clipping_enabled(),
        "GPU proof requires clip-distance support"
    );
    let mut random = CrtRand::new();
    let mut frame = M2Frame::prepare(
        &mut renderer,
        &ResidentM2Scene::default(),
        Arc::clone(&animations),
        &mut random,
        Arc::new(M2ParticleTwinkleTable::new(1)),
    )?;
    frame.sources.push(Some(prepare_gpu_source(
        &mut renderer,
        &model,
        &[M2ResolvedTexture::StockWhite],
        None,
        M2LocalLightCount::Four,
        M2ModelOrientation::Authored,
    )?));
    let playback = M2Playback::default_sequence(&model, &animations, 0, &mut random)?;
    frame.placements.push(m2_gpu_placement(
        0,
        Mat4::IDENTITY,
        M2GpuPlacementOwner::CreatureBody { guid: 1 },
        &model,
        Some(M2PlaybackStorage::Local(playback)),
        None,
        0,
    )?);
    frame.placements[0].color = [255, 255, 255, 128];
    frame.placement_topology_dirty = true;
    for (step, (z, copies, alpha)) in [
        (4., 1, 128),
        (1., 2, 128),
        (0., 2, 128),
        (-2., 1, 128),
        (-2., 1, 255),
    ]
    .into_iter()
    .enumerate()
    {
        let transform = root_transform * Mat4::from_translation(Vec3::new(1., 1., z));
        frame.placements[0].local_transform = transform;
        frame.placements[0].transform = transform;
        frame.placements[0].color[3] = alpha;
        frame.placements[0].scene_registration =
            Some(UnitSceneRegistration::new(&model, transform)?);
        let position = transform.w_axis.truncate();
        let camera = WorldCamera::stock(
            root_transform.transform_point3(Vec3::new(8., 1., z)),
            position,
            Vec3::Z,
            100.,
        )
        .frame(1.)?;
        let base = M2SceneUniform::new(
            camera.projection(),
            camera.view(),
            camera.camera().position(),
            Vec3::ONE,
            Vec3::ZERO,
            Vec3::Z,
            Vec4::ZERO,
            Vec3::ZERO,
            [M2LocalLightState::disabled(); 4],
        );
        let environment = solarity_systems::WorldEntityLightEnvironment::new(
            Vec3::ONE,
            Vec3::ZERO,
            -Vec3::Z,
            -Vec3::Z,
        );
        let camera_below = z < 1.;
        let first_pass = if camera_below {
            M2TransparentPass::One
        } else {
            M2TransparentPass::Two
        };
        let visible = frame.prepare_visible_draws_with_unit_effects(
            &renderer,
            WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
            camera,
            first_pass,
            Vec3::ZERO,
            step as f32 * 100.,
            M2CameraEffectScale::EXTERNAL_CAMERA,
            &mut random,
            None,
            None,
            Some((
                base,
                M2DirectionalLight::new(-Vec3::Z, Vec3::ONE, Vec3::ZERO),
            )),
            Some((&mut terrain, environment, Vec3::ZERO, &liquids)),
            None,
            None,
        )?;
        assert_eq!(visible.draws.len(), copies, "local Z={z}");
        let plane = |draw: &solarity_rendering::M2PreparedDraw| {
            let bytes = draw.material().to_bytes();
            Vec4::from_array(std::array::from_fn(|i| {
                super::super::scene_lighting_tests::word(&bytes, 304 + i * 4)
            }))
        };
        if copies == 2 {
            assert!(plane(&visible.draws[0]).abs_diff_eq(-plane(&visible.draws[1]), 1e-5));
            let solarity_rendering::M2LiquidState::Surface(expected) =
                solarity_rendering::M2LiquidState::at_world_height(
                    root_transform.transform_point3(Vec3::Z).z,
                    camera.view(),
                )
            else {
                return Err("surface".into());
            };
            assert!(
                plane(&visible.draws[0])
                    .abs_diff_eq(if camera_below { expected } else { -expected }, 0.001)
            );
            assert!(visible.draws[0].scene_order() < visible.water_scene_order);
            assert!(visible.draws[1].scene_order() >= visible.water_scene_order);
        } else {
            assert_eq!(plane(&visible.draws[0]), Vec4::W);
        }
        if step > 0 {
            let mut particles: Vec<_> = visible.particle_draws.iter().collect();
            particles.sort_by_key(|draw| draw.effect_order());
            assert_eq!(particles.len(), 3);
            for (index, particle) in particles.iter().enumerate() {
                let before = if index == 2 && alpha == 255 {
                    true
                } else {
                    let pass = if index == 1 || z < 0. {
                        M2TransparentPass::Two
                    } else {
                        M2TransparentPass::One
                    };
                    pass == first_pass
                };
                assert_eq!(
                    particle.scene_order() < visible.water_scene_order,
                    before,
                    "particle {index}, z={z}, alpha={alpha}"
                );
            }
            let mut ribbons: Vec<_> = visible.ribbon_draws.iter().collect();
            ribbons.sort_by_key(|draw| draw.first_vertex());
            assert_eq!(ribbons.len(), 4);
            for (index, passes) in ribbons.as_chunks::<2>().0.iter().enumerate() {
                let ribbon = passes[0];
                assert!(ribbon.first_material_pass());
                assert!(!passes[1].first_material_pass());
                assert_eq!(passes[1].scene_order(), ribbon.scene_order());
                assert_eq!(passes[1].effect_order(), ribbon.effect_order());
                assert_eq!(passes[1].first_vertex(), ribbon.first_vertex());
                assert_eq!(passes[1].vertex_count(), ribbon.vertex_count());
                assert_eq!(
                    passes.map(|pass| pass.blend_order()),
                    if index == 0 { [2, 0] } else { [0, 2] },
                    "all material passes retain authored order"
                );
                let vertices = &visible.ribbon_vertices[ribbon.first_vertex() as usize..]
                    [..ribbon.vertex_count() as usize];
                assert_eq!(
                    vertices.first().ok_or("old ribbon edge")?.color_bgra()[3],
                    128,
                    "retained edges keep their original owner opacity"
                );
                assert_eq!(
                    vertices.last().ok_or("live ribbon edge")?.color_bgra()[3],
                    alpha,
                    "new edges receive the current owner opacity"
                );
                let before = if index == 1 && alpha == 255 {
                    true
                } else {
                    (if z < 0. {
                        M2TransparentPass::Two
                    } else {
                        M2TransparentPass::One
                    }) == first_pass
                };
                assert_eq!(
                    ribbon.scene_order() < visible.water_scene_order,
                    before,
                    "ribbon {index}, z={z}, alpha={alpha}"
                );
            }
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
                Vec4::ZERO,
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
            &[],
            &[],
            &[],
            &[],
            &[],
        )?;
        let capture = renderer.take_captured_frame()?.ok_or("capture")?;
        for height in [-0.5, 0.5] {
            let clip = camera.view_projection()
                * transform
                    .transform_point3(Vec3::new(0., 0., height))
                    .extend(1.);
            let x = ((clip.x / clip.w * 0.5 + 0.5) * 128.) as usize;
            let y = ((0.5 - clip.y / clip.w * 0.5) * 128.) as usize;
            let pixel = &capture.rgba8()[(y * 128 + x) * 4..][..3];
            assert!(
                pixel.iter().all(|&value| value.abs_diff(alpha) <= 1),
                "Z={z}, sample={height}: {pixel:?}; both clips must blend exactly once"
            );
        }
    }
    // 82F0F0 shares the root liquid query with attachments, then classifies
    // each child's own sphere only when the root query reported a surface.
    frame.placements.clear();
    for owner in [
        M2GpuPlacementOwner::CreatureMount { guid: 1 },
        M2GpuPlacementOwner::CreatureBody { guid: 1 },
    ] {
        let playback = M2Playback::default_sequence(&model, &animations, 0, &mut random)?;
        let mut placement = m2_gpu_placement(
            0,
            Mat4::IDENTITY,
            owner,
            &model,
            Some(M2PlaybackStorage::Local(playback)),
            None,
            0,
        )?;
        placement.color = [255, 255, 255, 128];
        frame.placements.push(placement);
    }
    frame.placement_topology_dirty = true;
    for (step, z) in [1., -2.].into_iter().enumerate() {
        let transform = root_transform * Mat4::from_translation(Vec3::new(1., 1., z));
        for placement in &mut frame.placements {
            placement.local_transform = transform;
            placement.transform = transform;
            placement.scene_registration = None;
        }
        let camera = WorldCamera::stock(
            root_transform.transform_point3(Vec3::new(13., 1., 1.)),
            root_transform.transform_point3(Vec3::new(1., 1., 1.)),
            Vec3::Z,
            100.,
        )
        .frame(1.)?;
        let visible = frame.prepare_visible_draws_with_unit_effects(
            &renderer,
            WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
            camera,
            M2TransparentPass::Two,
            Vec3::ZERO,
            500. + step as f32 * 100.,
            M2CameraEffectScale::EXTERNAL_CAMERA,
            &mut random,
            None,
            None,
            None,
            Some((
                &mut terrain,
                solarity_systems::WorldEntityLightEnvironment::new(
                    Vec3::ONE,
                    Vec3::ZERO,
                    -Vec3::Z,
                    -Vec3::Z,
                ),
                Vec3::ZERO,
                &liquids,
            )),
            None,
            None,
        )?;
        assert_eq!(visible.draws.len(), if z == 1. { 3 } else { 2 });
        let rider_z = transform.transform_point3(Vec3::Z * 4.).z;
        let riders: Vec<_> = visible
            .draws
            .iter()
            .filter(|draw| {
                let bytes = draw.material().to_bytes();
                (super::super::scene_lighting_tests::word(&bytes, 56) - rider_z).abs() < 0.001
            })
            .collect();
        assert_eq!(
            riders.len(),
            1,
            "attachment is posed four units above mount"
        );
        assert_eq!(
            riders[0].scene_order() < visible.water_scene_order,
            z < 0.,
            "rider inherits a fully submerged root even while its own geometry is above water",
        );
    }
    renderer.shutdown()?;
    Ok(())
}

fn put(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn add_rider_attachment(bytes: &mut Vec<u8>) {
    let record = bytes.len();
    bytes.resize(record + 40, 0);
    put(bytes, record + 16, 4f32.to_bits());
    super::super::scene_lighting_tests::track(bytes, record + 20, &[0], &[1], 1, 0);
    put(bytes, 0xf0, 1);
    put(bytes, 0xf4, record as u32);
    let lookup = bytes.len();
    bytes.extend(0u16.to_le_bytes());
    put(bytes, 0xf8, 1);
    put(bytes, 0xfc, lookup as u32);
}

/// Three emitters exercise ordinary, forced-below, and opaque particle queues;
/// Two ribbons each have opposite opaque/alpha pass orders. The first material
/// classifies the whole emitter, even when later passes use a different blend.
fn add_effect_routes(bytes: &mut Vec<u8>) -> Result<(), Box<dyn Error>> {
    crate::test_support::unit_models::append_effects(bytes, 1);
    let original = u32::from_le_bytes(bytes[0x12c..0x130].try_into()?) as usize;
    let particle = bytes[original..original + 476].to_vec();
    let particles = bytes.len();
    for _ in 0..3 {
        bytes.extend_from_slice(&particle);
    }
    put(bytes, particles + 476 + 4, 0x2000);
    bytes[particles + 2 * 476 + 40] = 0;
    put(bytes, 0x128, 3);
    put(bytes, 0x12c, particles as u32);
    let materials = bytes.len();
    bytes.extend([7, 0, 2, 0, 7, 0, 0, 0]);
    put(bytes, 0x70, 2);
    put(bytes, 0x74, materials as u32);
    let original = u32::from_le_bytes(bytes[0x124..0x128].try_into()?) as usize;
    let ribbon = bytes[original..original + 176].to_vec();
    let ribbons = bytes.len();
    for _ in 0..2 {
        bytes.extend_from_slice(&ribbon);
    }
    for (index, materials) in [[0u16, 1], [1u16, 0]].into_iter().enumerate() {
        let material_indices = bytes.len();
        for material in materials {
            bytes.extend(material.to_le_bytes());
        }
        let texture_indices = bytes.len();
        bytes.extend([0u8; 4]);
        let emitter = ribbons + index * 176;
        put(bytes, emitter + 20, 2);
        put(bytes, emitter + 24, texture_indices as u32);
        put(bytes, emitter + 28, 2);
        put(bytes, emitter + 32, material_indices as u32);
    }
    put(bytes, 0x120, 2);
    put(bytes, 0x124, ribbons as u32);
    Ok(())
}
fn chunk(bytes: &mut Vec<u8>, magic: &[u8; 4], data: &[u8]) {
    bytes.extend(magic);
    bytes.extend((data.len() as u32).to_le_bytes());
    bytes.extend(data);
}
