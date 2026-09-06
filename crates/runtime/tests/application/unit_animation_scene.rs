//! Scene culling and GPU replacement cannot restart a unit's primary timer.

use super::super::{M2PlaybackStorage, m2_gpu_placement};
use super::*;
use crate::application::unit_animation::{UnitAnimationBehavior, UnitAnimationInput};
use glam::Mat4;
use solarity_ecs::UnitAnimationTier;
use solarity_rendering::{M2CameraEffectScale, WorldCamera, WorldFrustum, WorldScreenWindow};
use solarity_systems::UnitLocomotionAnimation;
use std::rc::Rc;

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
