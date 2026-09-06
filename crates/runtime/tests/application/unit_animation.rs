//! Archive-decoded primary timers exercise posture ownership and completion.

use super::*;
use crate::test_support::{ClientFixture, game_object_models as models};
use glam::Vec3;
use solarity_asset::{ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, Locale};
use solarity_ecs::{ActiveWorld, WorldBootstrap, WorldMapId};
use std::error::Error;

const POSES: &[u16] = &[
    0, 1, 4, 6, 91, 96, 97, 98, 99, 100, 101, 102, 103, 104, 114, 115, 116, 127, 131, 132, 201,
    202, 224, 300, 301, 302, 304, 466, 468, 472,
];

fn input(stand: u8) -> UnitAnimationInput {
    UnitAnimationInput {
        stand,
        locomotion: UnitLocomotionAnimation::STAND,
        tier: UnitAnimationTier::Ground,
        movement_flags: 0,
        mounted: false,
    }
}

fn owner(ids: &[u16], stand: u8) -> Result<UnitAnimationBehavior, Box<dyn Error>> {
    owner_with_input(ids, input(stand))
}

fn owner_with_input(
    ids: &[u16],
    initial: UnitAnimationInput,
) -> Result<UnitAnimationBehavior, Box<dyn Error>> {
    let mut bytes = models::model_with_animations(ids)?;
    let offset = u32::from_le_bytes(bytes[0x20..0x24].try_into()?) as usize;
    for (index, id) in ids.iter().enumerate() {
        let sequence = offset + index * 64;
        let flags: u32 = if matches!(
            id,
            1 | 6
                | 96
                | 98
                | 99
                | 101
                | 114
                | 116
                | 127
                | 131
                | 132
                | 201
                | 224
                | 300
                | 302
                | 466
                | 468
                | 472
        ) {
            0x21
        } else {
            0x20
        };
        bytes[sequence + 12..sequence + 16].copy_from_slice(&flags.to_le_bytes());
        bytes[sequence + 28..sequence + 32].copy_from_slice(&400_u32.to_le_bytes());
        let variation = ids[..index]
            .iter()
            .filter(|previous| *previous == id)
            .count() as u16;
        let metadata = variation
            + match id {
                466 => 7,
                468 => 11,
                472 => 17,
                _ => 0,
            };
        bytes[sequence + 2..sequence + 4].copy_from_slice(&metadata.to_le_bytes());
        if ids.get(index + 1) == Some(id) {
            bytes[sequence + 16..sequence + 20].copy_from_slice(&0_u32.to_le_bytes());
            bytes[sequence + 60..sequence + 62]
                .copy_from_slice(&((index + 1) as u16).to_le_bytes());
        }
    }
    let skin = models::skin()?;
    let mut dbc = b"WDBC".to_vec();
    for value in [POSES.len() as u32, 8, 32, 1] {
        dbc.extend_from_slice(&value.to_le_bytes());
    }
    for id in POSES {
        let fallback = match id {
            6 => 1,
            132 => 131,
            466 => 1,
            472 => 6,
            _ => 0,
        };
        let (behavior, tier) = if *id == 304 {
            (6, 3)
        } else if (300..=302).contains(id) {
            (u32::from(*id) - 204, 3)
        } else {
            (u32::from(*id), 0)
        };
        let flags = if matches!(id, 6 | 132) { 0x20 } else { 0 };
        for value in [u32::from(*id), 0, 0, 0, flags, fallback, behavior, tier] {
            dbc.extend_from_slice(&value.to_le_bytes());
        }
    }
    dbc.push(0);
    let fixture = ClientFixture::with_common_files(&[
        ("Solarity\\Postures.m2", &bytes),
        ("Solarity\\Postures00.skin", &skin),
        ("DBFilesClient\\AnimationData.dbc", &dbc),
    ])?;
    let archive =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(archive)?;
    let world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.0,
    ));
    Ok(UnitAnimationBehavior::new(
        world.object_identity(7).ok_or("local lifetime")?,
        Arc::new(DecodedM2Model::load(
            &mut store,
            &AssetPath::new("Solarity\\Postures.m2")?,
        )?),
        Arc::new(AnimationDataCatalog::load(&mut store)?),
        initial,
    ))
}

#[test]
fn ordinary_transitions_complete_on_the_cpu_and_retain_the_shared_timer()
-> Result<(), Box<dyn Error>> {
    for (stand, down, hold, up) in [(1, 96, 97, 98), (3, 99, 100, 101), (8, 114, 115, 116)] {
        let owner = owner(POSES, 0)?;
        let mut random = CrtRand::new();
        owner.advance_scene(100.0, 100.0, &mut random)?;
        let playback = owner.playback();
        owner.set_input(input(stand));
        owner.advance_scene(200.0, 200.0, &mut random)?;
        assert_eq!(playback.borrow().animation_id, down);
        owner.take_scene_sample();
        let mut expected = random;
        let _ = expected.next_u15();
        let _ = expected.next_u15();
        owner.advance_scene(1500.0, 1500.0, &mut random)?;
        assert_eq!(playback.borrow().animation_id, hold);
        assert_eq!(
            random, expected,
            "only incoming variation and cycle may roll"
        );
        let sample = owner.take_scene_sample().ok_or("scene sample")?;
        assert_eq!(sample.advance.expired_variations.len(), 1);
        assert_eq!(sample.advance.clock.animation_time_ms(), 0.0);
        assert_eq!(
            playback
                .borrow()
                .script_timer
                .ok_or("timer")?
                .start_time_ms(),
            1500
        );
        assert!(playback.borrow().script_blend.is_some());
        // GPU consumers can disappear and reconnect without constructing a timer.
        drop(playback);
        owner.advance_scene(1700.0, 1700.0, &mut random)?;
        let playback = owner.playback();
        assert_eq!(
            playback
                .borrow()
                .script_timer
                .ok_or("retained timer")?
                .start_time_ms(),
            1500
        );
        assert_eq!(random, expected);
        owner.set_input(input(0));
        owner.advance_scene(1800.0, 1800.0, &mut random)?;
        assert_eq!(playback.borrow().animation_id, up);
        owner.advance_scene(3000.0, 3000.0, &mut random)?;
        assert_eq!(playback.borrow().animation_id, 0);
    }
    Ok(())
}

#[test]
fn interrupted_sit_uses_latest_posture_and_identical_input_does_not_restart()
-> Result<(), Box<dyn Error>> {
    let owner = owner(POSES, 1)?;
    let mut random = CrtRand::new();
    owner.advance_scene(100.0, 100.0, &mut random)?;
    let playback = owner.playback();
    let expected = random;
    owner.set_input(input(1));
    owner.advance_scene(250.0, 250.0, &mut random)?;
    assert_eq!(playback.borrow().animation_id, 96);
    assert_eq!(random, expected);
    owner.set_input(input(0));
    owner.advance_scene(300.0, 300.0, &mut random)?;
    assert_eq!(playback.borrow().animation_id, 98);
    owner.advance_scene(1500.0, 1500.0, &mut random)?;
    assert_eq!(playback.borrow().animation_id, 0);
    Ok(())
}

#[test]
fn chair_loops_preserve_random_state_and_locomotion_overrides_posture() -> Result<(), Box<dyn Error>>
{
    for (stand, animation) in [(4, 102), (5, 103), (6, 104)] {
        let owner = owner(POSES, stand)?;
        let mut random = CrtRand::new();
        owner.advance_scene(100.0, 100.0, &mut random)?;
        let expected = random;
        owner.advance_scene(3500.0, 3500.0, &mut random)?;
        assert_eq!(owner.playback.borrow().animation_id, animation);
        assert_eq!(random, expected);
        let mut moving = input(stand);
        moving.locomotion = UnitLocomotionAnimation::new(4);
        owner.set_input(moving);
        owner.advance_scene(3600.0, 3600.0, &mut random)?;
        assert_eq!(owner.playback.borrow().animation_id, 4);
        moving.mounted = true;
        owner.set_input(moving);
        owner.advance_scene(3700.0, 3700.0, &mut random)?;
        assert_eq!(owner.playback.borrow().animation_id, 91);
    }
    Ok(())
}

#[test]
fn unavailable_pose_uses_actual_fallback_behavior_and_failed_request_remains_pending()
-> Result<(), Box<dyn Error>> {
    let owner = owner(&[0], 1)?;
    let mut random = CrtRand::new();
    owner.advance_scene(100.0, 100.0, &mut random)?;
    assert_eq!(owner.playback.borrow().animation_id, 0);
    assert_eq!(owner.behavior(&owner.playback.borrow()), 0);
    let expected = random;
    owner.advance_scene(2500.0, 2500.0, &mut random)?;
    assert_eq!(owner.playback.borrow().animation_id, 0);
    assert_eq!(random, expected);
    let failed = self::owner(&[4], 1)?;
    assert!(failed.synchronize(100, &mut random).is_err());
    assert_eq!(failed.pending.borrow().len(), 1);
    assert_eq!(failed.processed_stand.get(), 0);
    Ok(())
}

#[test]
fn death_entry_completes_through_native_chain_and_preserves_corpse_variation()
-> Result<(), Box<dyn Error>> {
    for corpse_variants in [1, 2] {
        let mut ids = vec![0, 466, 466, 468, 468, 472];
        if corpse_variants == 2 {
            ids.push(472);
        }
        let owner = owner(&ids, 7)?;
        let mut random = CrtRand::new();
        owner.advance_scene(100.0, 100.0, &mut random)?;
        assert_eq!(owner.playback.borrow().animation_id, 466);
        assert_eq!(
            owner.model.animations().sequences()[owner.playback.borrow().sequence]
                .variation_index(),
            8
        );
        // Input changes while dying cannot replace death with locomotion.
        let mut moving = input(7);
        moving.locomotion = UnitLocomotionAnimation::new(4);
        owner.set_input(moving);
        owner.advance_scene(200.0, 200.0, &mut random)?;
        assert_eq!(owner.playback.borrow().animation_id, 466);
        owner.advance_scene(1500.0, 1500.0, &mut random)?;
        assert_eq!(owner.playback.borrow().animation_id, 468);
        let mut expected = random;
        if corpse_variants == 1 {
            let _variation = expected.next_u15();
        }
        let _cycle = expected.next_u15();
        owner.advance_scene(2600.0, 2600.0, &mut random)?;
        assert_eq!(owner.playback.borrow().animation_id, 472);
        assert_eq!(
            owner.model.animations().sequences()[owner.playback.borrow().sequence]
                .variation_index(),
            corpse_variants + 16
        );
        assert_eq!(random, expected);
        owner.advance_scene(4000.0, 4000.0, &mut random)?;
        owner.advance_scene(8000.0, 8000.0, &mut random)?;
        assert_eq!(owner.playback.borrow().animation_id, 472);
        assert_eq!(random, expected, "a completed corpse retains its timer");
    }
    Ok(())
}

#[test]
fn swimming_and_fallback_death_use_the_actual_primary_completion() -> Result<(), Box<dyn Error>> {
    for (ids, swimming, start, finish) in [
        (&[0, 1, 6][..], false, 1, 6),
        (&[0, 131, 132][..], true, 131, 132),
    ] {
        let mut death = input(7);
        death.movement_flags = if swimming { 0x200000 } else { 0 };
        let owner = owner_with_input(ids, death)?;
        let mut random = CrtRand::new();
        owner.advance_scene(100.0, 100.0, &mut random)?;
        assert_eq!(owner.playback.borrow().animation_id, start);
        let mut expected = random;
        let _cycle = expected.next_u15();
        owner.advance_scene(1500.0, 1500.0, &mut random)?;
        assert_eq!(owner.playback.borrow().animation_id, finish);
        assert_eq!(random, expected);
    }
    Ok(())
}

#[test]
fn submerged_entry_finishes_before_hold_and_exit_prefers_standup() -> Result<(), Box<dyn Error>> {
    for (ids, exit) in [(POSES, 127), (&[0, 201, 202, 224][..], 224)] {
        let owner = owner(ids, 9)?;
        let mut random = CrtRand::new();
        owner.advance_scene(100.0, 100.0, &mut random)?;
        assert_eq!(owner.playback.borrow().animation_id, 201);
        owner.advance_scene(1500.0, 1500.0, &mut random)?;
        assert_eq!(owner.playback.borrow().animation_id, 202);
        owner.set_input(input(0));
        owner.advance_scene(1600.0, 1600.0, &mut random)?;
        assert_eq!(owner.playback.borrow().animation_id, exit);
        owner.advance_scene(2800.0, 2800.0, &mut random)?;
        assert_eq!(owner.playback.borrow().animation_id, 0);
    }
    Ok(())
}

#[test]
fn tiered_postures_complete_by_behavior_and_resolve_successor_in_the_same_tier()
-> Result<(), Box<dyn Error>> {
    let mut pose = input(1);
    pose.tier = UnitAnimationTier::Fly;
    let owner = owner_with_input(&[0, 300, 301, 302], pose)?;
    let mut random = CrtRand::new();
    owner.advance_scene(100.0, 100.0, &mut random)?;
    assert_eq!(owner.playback.borrow().animation_id, 300);
    owner.advance_scene(1500.0, 1500.0, &mut random)?;
    assert_eq!(owner.playback.borrow().animation_id, 301);
    pose.stand = 0;
    owner.set_input(pose);
    owner.advance_scene(1600.0, 1600.0, &mut random)?;
    assert_eq!(owner.playback.borrow().animation_id, 302);
    owner.advance_scene(2800.0, 2800.0, &mut random)?;
    assert_eq!(owner.playback.borrow().animation_id, 0);
    Ok(())
}

#[test]
fn direct_corpse_callbacks_bypass_unit_tiers_and_hold_a_missing_corpse_endpoint()
-> Result<(), Box<dyn Error>> {
    for (ids, swimming, expected_id, mode) in [
        (&[0, 1, 6, 304][..], false, 6, M2ModelAnimationMode::Forward),
        (&[0, 1, 304][..], false, 1, M2ModelAnimationMode::HoldEnd),
        (&[0, 131][..], true, 131, M2ModelAnimationMode::HoldEnd),
    ] {
        let mut death = input(7);
        death.tier = UnitAnimationTier::Fly;
        death.movement_flags = if swimming { 0x200000 } else { 0 };
        let owner = owner_with_input(ids, death)?;
        let mut random = CrtRand::new();
        owner.advance_scene(100.0, 100.0, &mut random)?;
        let mut expected_random = random;
        let _cycle = expected_random.next_u15();
        owner.advance_scene(1500.0, 1500.0, &mut random)?;
        assert_eq!(owner.playback.borrow().animation_id, expected_id);
        assert_eq!(owner.playback.borrow().script_mode, mode);
        assert_eq!(random, expected_random);
        owner.advance_scene(5000.0, 5000.0, &mut random)?;
        assert_eq!(random, expected_random);
    }
    Ok(())
}
