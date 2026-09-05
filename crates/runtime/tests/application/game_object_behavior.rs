//! Live CPU model callbacks and replicated state share one retained owner.

use super::*;
use crate::test_support::{ClientFixture, game_object_models as models};
use glam::Vec3;
use solarity_asset::{ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, Locale};
use solarity_ecs::{ObjectKind, WorldBootstrap, WorldMapId};
use solarity_systems::project_object_fields;
use std::error::Error;

fn world(state: u8, progress: u16) -> Result<ActiveWorld, Box<dyn Error>> {
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.0,
    ));
    world.create_object(9, ObjectKind::GameObject, None, [])?;
    fields(
        &mut world,
        &[
            (8, 42),
            (14, u32::from(progress) << 16),
            (17, u32::from(state)),
        ],
    )?;
    Ok(world)
}

fn fields(world: &mut ActiveWorld, fields: &[(u16, u32)]) -> Result<(), Box<dyn Error>> {
    world.update_fields(9, fields.iter().copied())?;
    project_object_fields(world, 9, fields.iter().copied())?;
    Ok(())
}

fn owner(
    world: &ActiveWorld,
    boneless: bool,
) -> Result<(GameObjectBehavior, Arc<DecodedM2Model>), Box<dyn Error>> {
    let ids = [145, 146, 147, 148, 149, 150, 151, 152];
    let mut bytes = models::model_with_animations(&ids)?;
    let offset = u32::from_le_bytes(bytes[0x20..0x24].try_into()?) as usize;
    for index in 0..ids.len() {
        bytes[offset + index * 64 + 12..offset + index * 64 + 16]
            .copy_from_slice(&0x21_u32.to_le_bytes());
    }
    if boneless {
        bytes[0x2C..0x34].fill(0);
    }
    let skin = models::skin()?;
    let fixture = ClientFixture::with_common_files(&[
        ("World\\GameObject.m2", &bytes),
        ("World\\GameObject00.skin", &skin),
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let model = Arc::new(DecodedM2Model::load(
        &mut store,
        &AssetPath::new("World\\GameObject.m2")?,
    )?);
    let owner = GameObjectBehavior::new(
        world.object_identity(9).ok_or("identity")?,
        world.game_object_presentation(9).ok_or("fields")?,
        Arc::new(AnimationDataCatalog::load(&mut store)?),
    );
    Ok((owner, model))
}

#[test]
fn live_transition_completes_without_gpu_placement_and_preserves_shared_timer()
-> Result<(), Box<dyn Error>> {
    let mut world = world(1, u16::MAX)?;
    let (owner, model) = owner(&world, false)?;
    let mut random = CrtRand::new();
    owner.attach_model(&world, 42, &model, 0, &mut random)?;
    let playback = owner.playback().ok_or("playback")?;
    assert_eq!(owner.state(), Some(GameObjectAnimationState::Closed));
    fields(&mut world, &[(17, 0)])?;
    owner.notify(&world, GameObjectNotification::State, 100, &mut random)?;
    assert_eq!(owner.state(), Some(GameObjectAnimationState::Opening));
    assert_eq!(playback.borrow().animation_id, 148);
    owner.advance_scene(&world, 1_100.0, 1_100.0, &mut random)?;
    assert_eq!(owner.state(), Some(GameObjectAnimationState::Opening));
    owner.advance_scene(&world, 1_101.0, 1_101.0, &mut random)?;
    assert_eq!(owner.state(), Some(GameObjectAnimationState::Opened));
    assert_eq!(playback.borrow().animation_id, 149);
    let sample = owner.take_scene_sample().ok_or("scene sample")?;
    assert_eq!(sample.advance.expired_variations.len(), 1);
    assert_eq!(sample.advance.clock.animation_time_ms(), 0.0);
    assert!(owner.take_scene_sample().is_none());
    let timer = playback.borrow().script_timer;
    let rng = random;
    owner.attach_model(&world, 42, &model, 1_101, &mut random)?;
    assert!(Rc::ptr_eq(
        &playback,
        &owner.playback().ok_or("shared playback")?
    ));
    assert_eq!(playback.borrow().script_timer, timer);
    assert_eq!(random, rng);
    fields(&mut world, &[(17, 1)])?;
    owner.notify(&world, GameObjectNotification::State, 1_101, &mut random)?;
    assert!(!owner.state().ok_or("state")?.door_collision_eligible());
    owner.advance_scene(&world, 2_102.0, 2_102.0, &mut random)?;
    assert!(owner.state().ok_or("state")?.door_collision_eligible());
    owner.advance_scene(&world, 3_102.0, 3_102.0, &mut random)?;
    assert_eq!(
        playback
            .borrow()
            .script_timer
            .ok_or("stable timer")?
            .start_time_ms(),
        3_102
    );
    Ok(())
}

#[test]
fn live_pause_and_reversal_keep_fractional_pose() -> Result<(), Box<dyn Error>> {
    let mut world = world(0, 32_767)?;
    let (owner, model) = owner(&world, false)?;
    let mut random = CrtRand::new();
    owner.attach_model(&world, 42, &model, 2_000, &mut random)?;
    assert_eq!(
        world
            .game_object_presentation(9)
            .ok_or("fields")?
            .sequence_progress(),
        None
    );
    owner.advance_scene(&world, 2_001.0, 2_001.0, &mut random)?;
    assert_eq!(
        owner
            .take_scene_sample()
            .ok_or("sample")?
            .advance
            .clock
            .animation_time_ms(),
        500.0
    );
    fields(&mut world, &[(9, 0x80)])?;
    owner.notify(
        &world,
        GameObjectNotification::Flags { previous: 0 },
        2_001,
        &mut random,
    )?;
    let rng = random;
    owner.advance_scene(&world, 3_001.0, 3_001.0, &mut random)?;
    assert_eq!(
        owner
            .take_scene_sample()
            .ok_or("sample")?
            .advance
            .clock
            .animation_time_ms(),
        500.0
    );
    assert_eq!(random, rng);
    fields(&mut world, &[(9, 0), (17, 1)])?;
    owner.notify(
        &world,
        GameObjectNotification::Flags { previous: 0x80 },
        3_001,
        &mut random,
    )?;
    owner.notify(&world, GameObjectNotification::State, 3_001, &mut random)?;
    assert_eq!(owner.state(), Some(GameObjectAnimationState::Closing));
    owner.advance_scene(&world, 3_002.0, 3_002.0, &mut random)?;
    assert_eq!(
        owner
            .take_scene_sample()
            .ok_or("sample")?
            .advance
            .clock
            .animation_time_ms(),
        500.0
    );
    owner.advance_scene(&world, 3_502.0, 3_502.0, &mut random)?;
    assert_eq!(owner.state(), Some(GameObjectAnimationState::Closed));
    Ok(())
}

#[test]
fn boneless_transition_consumes_progress_without_a_sequence_draw() -> Result<(), Box<dyn Error>> {
    let world = world(0, 32_767)?;
    let (owner, model) = owner(&world, true)?;
    let mut random = CrtRand::new();
    let rng = random;
    owner.attach_model(&world, 42, &model, 0, &mut random)?;
    assert_eq!(
        world
            .game_object_presentation(9)
            .ok_or("fields")?
            .sequence_progress(),
        None
    );
    assert!(
        owner
            .playback()
            .ok_or("playback")?
            .borrow()
            .script_timer
            .is_none()
    );
    assert_eq!(random, rng);
    Ok(())
}

#[test]
fn progress_handler_consumption_changes_the_following_state_decision() -> Result<(), Box<dyn Error>>
{
    let mut world = world(0, 0)?;
    let (owner, model) = owner(&world, false)?;
    let mut random = CrtRand::new();
    owner.attach_model(&world, 42, &model, 0, &mut random)?;
    fields(&mut world, &[(14, 0x7FFF_0001), (17, 2)])?;
    owner.notify(&world, GameObjectNotification::Progress, 100, &mut random)?;
    owner.notify(&world, GameObjectNotification::State, 100, &mut random)?;
    assert_eq!(owner.state(), Some(GameObjectAnimationState::Destroyed));
    assert_eq!(
        owner.playback().ok_or("playback")?.borrow().animation_id,
        151
    );
    assert_eq!(
        world
            .game_object_presentation(9)
            .ok_or("fields")?
            .sequence_progress(),
        None
    );
    Ok(())
}

#[test]
fn display_replacement_retires_the_old_timer_before_state_callbacks() -> Result<(), Box<dyn Error>>
{
    let mut world = world(1, u16::MAX)?;
    let (owner, model) = owner(&world, false)?;
    let mut random = CrtRand::new();
    owner.attach_model(&world, 42, &model, 0, &mut random)?;
    let old = owner.playback().ok_or("old playback")?;
    let old_timer = old.borrow().script_timer;
    let rng = random;
    fields(&mut world, &[(8, 43), (17, 0)])?;
    owner.notify(&world, GameObjectNotification::State, 100, &mut random)?;
    assert!(owner.playback().is_none());
    assert_eq!(owner.state(), Some(GameObjectAnimationState::Opening));
    assert_eq!(old.borrow().script_timer, old_timer);
    assert_eq!(random, rng);
    owner.attach_model(&world, 43, &model, 100, &mut random)?;
    let new = owner.playback().ok_or("new playback")?;
    assert!(!Rc::ptr_eq(&old, &new));
    assert_eq!(new.borrow().animation_id, 148);
    Ok(())
}
