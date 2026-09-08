//! Archive-decoded primary timers exercise posture ownership and completion.

use super::*;
use crate::test_support::{ClientFixture, game_object_models as models};
use glam::Vec3;
use solarity_asset::{ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, Locale};
use solarity_ecs::{ActiveWorld, WorldBootstrap, WorldMapId};
use std::error::Error;

const POSES: &[u16] = &[
    0, 1, 4, 5, 6, 11, 12, 13, 37, 38, 39, 40, 91, 96, 97, 98, 99, 100, 101, 102, 103, 104, 114,
    115, 116, 127, 131, 132, 187, 201, 202, 224, 300, 301, 302, 304, 466, 468, 472,
];

#[test]
#[ignore = "requires SOLARITY_STOCK_DATA_ROOT with locally owned build-12340 archives"]
fn stock_drowning_kit_and_health_death_complete_and_return_to_current_movement()
-> Result<(), Box<dyn Error>> {
    let root = std::env::var_os("SOLARITY_STOCK_DATA_ROOT").ok_or("stock data root")?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(root)?,
        Locale::EnUs,
    )?)?;
    let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
    let kits = solarity_asset::EnvironmentalDamageCatalog::load(&mut store)?;
    let animation = u16::try_from(kits.visual_kit(1).ok_or("drowning kit")?.animation())?;
    let world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.0,
    ));
    let identity = world.object_identity(7).ok_or("identity")?;
    for race in [
        "Human", "Orc", "Dwarf", "NightElf", "Scourge", "Tauren", "Gnome", "Troll", "BloodElf",
        "Draenei",
    ] {
        for gender in ["Male", "Female"] {
            let path = AssetPath::new(format!("Character/{race}/{gender}/{race}{gender}.m2"))?;
            let model = Arc::new(DecodedM2Model::load(&mut store, &path)?);
            for flags in [0, 0x200000] {
                let owner = UnitAnimationBehavior::new(
                    identity,
                    Arc::clone(&model),
                    Arc::clone(&animations),
                    input(0).with_movement(movement(flags, None)),
                    0,
                );
                let mut random = CrtRand::new();
                owner.advance_scene(100.0, &mut random)?;
                let ordinary = owner.behavior(&owner.playback.borrow());
                owner.request_visual_kit_animation(animation);
                owner.advance_scene(101.0, &mut random)?;
                assert_eq!(
                    owner.behavior(&owner.playback.borrow()),
                    9,
                    "{path}, flags={flags}"
                );
                let end = owner
                    .playback
                    .borrow()
                    .script_timer
                    .ok_or("wound timer")?
                    .end_time_ms();
                owner.advance_scene(end as f32 + 1.0, &mut random)?;
                assert_eq!(
                    owner.behavior(&owner.playback.borrow()),
                    ordinary,
                    "{path}, flags={flags}"
                );
                let mut dying = input(0).with_movement(movement(flags, None));
                dying.alive = false;
                owner.set_input(dying);
                let mut now = end as f32 + 100.0;
                owner.advance_scene(now, &mut random)?;
                let entry = owner.behavior(&owner.playback.borrow());
                assert!(matches!(entry, 1 | 131 | 466), "{path}: {entry}");
                if flags != 0 {
                    assert_eq!(entry, 131, "{path}");
                }
                owner.request_visual_kit_animation(animation);
                owner.advance_scene(now + 1.0, &mut random)?;
                assert_eq!(owner.behavior(&owner.playback.borrow()), entry, "{path}");
                for _ in 0..3 {
                    let end = owner
                        .playback
                        .borrow()
                        .script_timer
                        .ok_or("death timer")?
                        .end_time_ms();
                    now = end as f32 + 1.0;
                    owner.advance_scene(now, &mut random)?;
                    if matches!(owner.behavior(&owner.playback.borrow()), 6 | 132 | 472)
                        || owner.playback.borrow().script_mode == M2ModelAnimationMode::HoldEnd
                    {
                        break;
                    }
                }
                let playback = owner.playback.borrow();
                assert!(
                    matches!(owner.behavior(&playback), 6 | 132 | 472)
                        || (matches!(owner.behavior(&playback), 1 | 131)
                            && playback.script_mode == M2ModelAnimationMode::HoldEnd),
                    "{path}, flags={flags}: behavior={}, mode={:?}",
                    owner.behavior(&playback),
                    playback.script_mode
                );
                drop(playback);
                owner.set_input(input(0).with_movement(movement(flags, None)));
                owner.advance_scene(now + 100.0, &mut random)?;
                assert_eq!(owner.behavior(&owner.playback.borrow()), ordinary, "{path}");
            }
        }
    }
    Ok(())
}

#[test]
#[ignore = "requires SOLARITY_STOCK_DATA_ROOT with the user's 3.3.5a archives"]
fn stock_repeated_jumps_sample_authored_variations() -> Result<(), Box<dyn Error>> {
    let root = std::env::var_os("SOLARITY_STOCK_DATA_ROOT").ok_or("stock data root")?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(root)?,
        Locale::EnUs,
    )?)?;
    let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
    let world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.,
    ));
    let identity = world.object_identity(7).ok_or("local identity")?;
    for race in [
        "Human", "Orc", "Dwarf", "NightElf", "Scourge", "Tauren", "Gnome", "Troll", "BloodElf",
        "Draenei",
    ] {
        for gender in ["Male", "Female"] {
            let path = AssetPath::new(format!("Character\\{race}\\{gender}\\{race}{gender}.m2"))?;
            let model = Arc::new(DecodedM2Model::load(&mut store, &path)?);
            let owner =
                UnitAnimationBehavior::new(identity, model, Arc::clone(&animations), input(0), 0);
            let mut random = CrtRand::new();
            let mut time = 100.;
            let mut seen = BTreeMap::<u16, BTreeMap<usize, usize>>::new();
            for _ in 0..128 {
                owner.set_input(input(0));
                owner.advance_scene(time, &mut random)?;
                notify(
                    &owner,
                    movement(0x1000, Some(-7.95555)),
                    UnitMovementAnimationEventKind::Jump,
                );
                time += 100.;
                owner.advance_scene(time, &mut random)?;
                for expected in [37, 38] {
                    let playback = owner.playback.borrow();
                    assert_eq!(owner.behavior(&playback), expected, "{path}");
                    *seen
                        .entry(expected)
                        .or_default()
                        .entry(playback.sequence)
                        .or_default() += 1;
                    time = playback.script_timer.ok_or("jump timer")?.end_time_ms() as f32 + 1.;
                    drop(playback);
                    owner.advance_scene(time, &mut random)?;
                }
                notify(
                    &owner,
                    movement(0, None),
                    UnitMovementAnimationEventKind::Land {
                        previous_flags: 0x1000,
                        forced: true,
                        slow: true,
                    },
                );
                time += 100.;
                owner.advance_scene(time, &mut random)?;
                time = owner
                    .playback
                    .borrow()
                    .script_timer
                    .ok_or("land timer")?
                    .end_time_ms() as f32
                    + 1.;
                owner.advance_scene(time, &mut random)?;
                time += 100.;
            }
            let metadata = owner
                .model
                .animations()
                .sequences()
                .iter()
                .enumerate()
                .filter(|(_, sequence)| matches!(sequence.animation_id(), 37 | 38))
                .map(|(index, sequence)| {
                    (
                        index,
                        sequence.animation_id(),
                        sequence.variation_index(),
                        sequence.frequency(),
                        sequence.variation_next(),
                    )
                })
                .collect::<Vec<_>>();
            let events = owner
                .model
                .animations()
                .events()
                .iter()
                .map(|event| {
                    (
                        String::from_utf8_lossy(&event.identifier()).into_owned(),
                        event.data(),
                    )
                })
                .collect::<Vec<_>>();
            println!("{path}: authored={metadata:?} sampled={seen:?} events={events:?}");
        }
    }
    Ok(())
}

fn input(stand: u8) -> UnitAnimationInput {
    UnitAnimationInput::new(stand, UnitAnimationTier::Ground, false, None)
}

#[test]
fn mouse_twist_retains_random_state_then_release_selects_procedural_turn()
-> Result<(), Box<dyn Error>> {
    let owner = owner(POSES, 0)?;
    let mut random = CrtRand::new();
    owner.advance_scene(1000., &mut random)?;
    let initial_random = random;
    let mut facing = input(0);
    facing.controlled = true;
    facing.mouse_turning = true;
    facing.facing = 0.5;
    owner.set_input(facing);
    owner.advance_scene(1001., &mut random)?;
    let twist = owner.body_pose();
    assert_eq!(twist.placement_rotation, body_rotation(-0.5));
    assert_eq!(twist.bone_transforms(), &[(4, body_rotation(0.5))]);
    assert_eq!(owner.playback.borrow().animation_id, 0);
    owner.advance_scene(1016., &mut random)?;
    assert_eq!(
        random, initial_random,
        "mouse twist does not select a variation"
    );
    facing.mouse_turning = false;
    owner.set_input(facing);
    owner.advance_scene(1017., &mut random)?;
    assert_eq!(
        owner.playback.borrow().animation_id,
        0,
        "release resets the facing tick"
    );
    owner.advance_scene(1018., &mut random)?;
    assert_eq!(owner.playback.borrow().animation_id, 11);
    assert_eq!(owner.body_pose().procedural_turn, 0x800);
    let caught_up = owner.body_pose();
    let turn_random = random;
    owner.advance_scene(1018., &mut random)?;
    assert_eq!(
        owner.body_pose().placement_rotation,
        caught_up.placement_rotation,
        "the same scene cannot advance body smoothing twice"
    );
    assert_eq!(random, turn_random);
    owner.advance_scene(1118., &mut random)?;
    assert!(owner.body_pose().bone_transforms().is_empty());
    owner.advance_scene(1119., &mut random)?;
    assert_eq!(
        owner.playback.borrow().animation_id,
        11,
        "native turn admission retains the current clip after procedural flags clear"
    );
    assert_eq!(owner.body_pose().placement_rotation, Mat4::IDENTITY);
    let end = owner
        .playback
        .borrow()
        .script_timer
        .ok_or("turn timer")?
        .end_time_ms() as f32
        + 1.;
    owner.advance_scene(end, &mut random)?;
    assert_eq!(owner.playback.borrow().animation_id, 0);
    Ok(())
}

fn owner(ids: &[u16], stand: u8) -> Result<UnitAnimationBehavior, Box<dyn Error>> {
    owner_with_input(ids, input(stand))
}

fn owner_with_input(
    ids: &[u16],
    initial: UnitAnimationInput,
) -> Result<UnitAnimationBehavior, Box<dyn Error>> {
    owner_with_sequence_metadata(ids, initial, |_, _, _| {})
}

fn owner_with_sequence_metadata(
    ids: &[u16],
    initial: UnitAnimationInput,
    configure: impl Fn(usize, u16, &mut [u8]),
) -> Result<UnitAnimationBehavior, Box<dyn Error>> {
    let mut bytes = models::model_with_animations(ids)?;
    let key_bones = bytes.len() as u32;
    for index in [-1_i16, -1, -1, -1, 0] {
        bytes.extend_from_slice(&index.to_le_bytes());
    }
    bytes[0x34..0x38].copy_from_slice(&5_u32.to_le_bytes());
    bytes[0x38..0x3c].copy_from_slice(&key_bones.to_le_bytes());
    let offset = u32::from_le_bytes(bytes[0x20..0x24].try_into()?) as usize;
    for (index, id) in ids.iter().enumerate() {
        let sequence = offset + index * 64;
        let flags: u32 = if matches!(
            id,
            1 | 6
                | 37
                | 39
                | 187
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
        configure(index, *id, &mut bytes[sequence..sequence + 64]);
    }
    let skin = models::skin()?;
    let mut dbc = b"WDBC".to_vec();
    let mut definitions = POSES.iter().chain(ids).copied().collect::<Vec<_>>();
    definitions.sort_unstable();
    definitions.dedup();
    for value in [definitions.len() as u32, 8, 32, 1] {
        dbc.extend_from_slice(&value.to_le_bytes());
    }
    for id in &definitions {
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
        0,
    ))
}

#[test]
fn movement_sequence_timing_matches_original_executable() -> Result<(), Box<dyn Error>> {
    let mut cases = 0;
    for line in
        include_str!("../../../systems/tests/fixtures/unit-movement-speed-native.txt").lines()
    {
        let words: Vec<_> = line.split_whitespace().collect();
        if words.first() != Some(&"timing") {
            continue;
        }
        let hex = |index: usize| u32::from_str_radix(words[index], 16);
        let (rate, offset) = unit_sequence_timing(
            words[1].parse()?,
            hex(2)?,
            f32::from_bits(hex(3)?),
            f32::from_bits(hex(4)?),
            words[5].parse()?,
            Some((f32::from_bits(hex(6)?), words[7].parse()?, hex(8)?)),
        );
        assert_eq!(
            (rate.to_bits(), offset as u32),
            (hex(9)?, hex(10)?),
            "{line}"
        );
        cases += 1;
    }
    assert_eq!(cases, 756);
    Ok(())
}

#[test]
fn speed_changes_keep_variation_and_random_state_then_carry_fresh_stride_into_walk()
-> Result<(), Box<dyn Error>> {
    let owner = owner_with_sequence_metadata(
        &[0, 4, 5, 5],
        input(0).with_movement(movement(1, None)),
        |index, id, bytes| {
            let (speed, duration): (f32, u32) = match (id, index) {
                (4, _) => (2.5, 1200),
                (5, 2) => (7., 1000),
                (5, 3) => (99., 800),
                _ => (0., 1000),
            };
            bytes[4..8].copy_from_slice(&duration.to_le_bytes());
            bytes[8..12].copy_from_slice(&speed.to_le_bytes());
        },
    )?;
    let mut random = CrtRand::new();
    owner.synchronize(100, &mut random)?;
    let retained_random = random;
    {
        let playback = owner.playback.borrow();
        assert_eq!(playback.sequence, 3);
        assert_eq!(playback.script_timer.ok_or("timer")?.speed(), 1.);
    }
    let mut faster = owner.input.get();
    faster.movement_speed = 10.5;
    owner.set_input(faster);
    owner.synchronize(301, &mut random)?;
    let outgoing = {
        let playback = owner.playback.borrow();
        assert_eq!(playback.sequence, 3);
        let timer = playback.script_timer.ok_or("timer")?;
        assert_eq!(timer.speed(), 1.5);
        assert_eq!(timer.start_time_ms(), 168);
        assert_eq!(timer.unwrapped_time(301), 199);
        timer
    };
    assert_eq!(random, retained_random);
    owner.set_input(input(0).with_movement(movement(0x101, None)));
    owner.synchronize(401, &mut random)?;
    let playback = owner.playback.borrow();
    assert_eq!(playback.animation_id, 4);
    let timer = playback.script_timer.ok_or("walk timer")?;
    assert_eq!(timer.speed(), 1.);
    // Fresh outgoing phase is 349; the selected variation's 800ms duration
    // maps this to 523ms in Walk. The native setup adds its one-tick delay.
    assert_eq!(timer.unwrapped_time(401), 522);
    assert_eq!(
        playback.script_blend.ok_or("walk blend")?,
        solarity_rendering::M2ModelSequenceBlend::new(3, outgoing, 401, 400)
    );
    let mut expected = retained_random;
    let _variation = expected.next_u15();
    let _cycles = expected.next_u15();
    assert_eq!(random, expected);
    Ok(())
}

fn movement(flags: u32, vertical: Option<f32>) -> WorldMovementState {
    WorldMovementState::new(
        u64::from(flags),
        solarity_ecs::WorldMovementSpeeds::new([2.5, 7., 4.5, 4.72, 2.5, 7., 4.5, 3., 3.]),
        solarity_ecs::WorldMovementContext {
            falling: vertical.map(|vertical_speed| solarity_ecs::WorldMovementFall {
                vertical_speed,
                direction_sin: 0.,
                direction_cos: 1.,
                horizontal_speed: 0.,
            }),
            ..Default::default()
        },
    )
}

fn notify(
    owner: &UnitAnimationBehavior,
    movement: WorldMovementState,
    kind: UnitMovementAnimationEventKind,
) {
    owner.notify_movement(UnitMovementAnimationEvent {
        identity: owner.identity,
        movement,
        stand: 0,
        kind,
    });
}

#[test]
fn jump_retains_takeoff_then_loops_and_lands_with_shared_random_and_blend()
-> Result<(), Box<dyn Error>> {
    let owner = owner(POSES, 0)?;
    let mut random = CrtRand::new();
    owner.advance_scene(100., &mut random)?;
    notify(
        &owner,
        movement(0x1000, Some(-7.95555)),
        UnitMovementAnimationEventKind::Jump,
    );
    owner.advance_scene(200., &mut random)?;
    assert_eq!(owner.playback.borrow().animation_id, 37);
    let mut expected = random;
    owner.set_input(input(0).with_movement(movement(0x1001, Some(-7.95555))));
    owner.advance_scene(300., &mut random)?;
    assert_eq!(owner.playback.borrow().animation_id, 37);
    assert_eq!(random, expected);
    owner.advance_scene(1500., &mut random)?;
    assert_eq!(owner.playback.borrow().animation_id, 38);
    let _variation = expected.next_u15();
    let _cycle = expected.next_u15();
    assert_eq!(random, expected);
    notify(
        &owner,
        movement(0, None),
        UnitMovementAnimationEventKind::Land {
            previous_flags: 0x3000,
            forced: false,
            slow: true,
        },
    );
    owner.set_input(input(0).with_movement(movement(0, None)));
    owner.advance_scene(1700., &mut random)?;
    assert_eq!(owner.playback.borrow().animation_id, 39);
    assert!(owner.playback.borrow().script_blend.is_some());
    expected = random;
    owner.advance_scene(1800., &mut random)?;
    assert_eq!(owner.playback.borrow().animation_id, 39);
    assert_eq!(random, expected);
    owner.advance_scene(3000., &mut random)?;
    assert_eq!(owner.playback.borrow().animation_id, 0);
    Ok(())
}

#[test]
fn running_landing_completes_and_new_movement_can_interrupt_stationary_landing()
-> Result<(), Box<dyn Error>> {
    let owner = owner(POSES, 0)?;
    let mut random = CrtRand::new();
    owner.advance_scene(100., &mut random)?;
    notify(
        &owner,
        movement(1, None),
        UnitMovementAnimationEventKind::Land {
            previous_flags: 0x3001,
            forced: false,
            slow: false,
        },
    );
    owner.advance_scene(200., &mut random)?;
    assert_eq!(owner.playback.borrow().animation_id, 187);
    owner.advance_scene(1500., &mut random)?;
    assert_eq!(owner.playback.borrow().animation_id, 5);
    notify(
        &owner,
        movement(0, None),
        UnitMovementAnimationEventKind::Land {
            previous_flags: 0x3000,
            forced: false,
            slow: true,
        },
    );
    owner.advance_scene(1600., &mut random)?;
    assert_eq!(owner.playback.borrow().animation_id, 39);
    owner.set_input(input(0).with_movement(movement(1, None)));
    owner.advance_scene(1700., &mut random)?;
    assert_eq!(owner.playback.borrow().animation_id, 5);
    Ok(())
}

#[test]
fn turn_directions_and_zero_launch_falls_use_distinct_requests() -> Result<(), Box<dyn Error>> {
    let owner = owner(POSES, 0)?;
    let mut random = CrtRand::new();
    for (index, flags, vertical, expected) in [
        (1, 0x10, None, 11),
        (2, 0x20, None, 12),
        (3, 0x11, None, 5),
        (4, 0x1000, Some(0.), 0),
        (5, 0x3000, Some(0.), 40),
    ] {
        owner.set_input(input(0).with_movement(movement(flags, vertical)));
        owner.advance_scene(index as f32 * 100., &mut random)?;
        assert_eq!(owner.playback.borrow().animation_id, expected);
    }
    Ok(())
}

#[test]
fn jump_and_landing_in_one_scene_preserve_both_ordered_requests() -> Result<(), Box<dyn Error>> {
    let owner = owner(POSES, 0)?;
    let mut random = CrtRand::new();
    owner.advance_scene(100., &mut random)?;
    let mut expected = random;
    notify(
        &owner,
        movement(0x1000, Some(-7.95555)),
        UnitMovementAnimationEventKind::Jump,
    );
    notify(
        &owner,
        movement(0, None),
        UnitMovementAnimationEventKind::Land {
            previous_flags: 0x1000,
            forced: true,
            slow: true,
        },
    );
    owner.advance_scene(200., &mut random)?;
    assert_eq!(owner.playback.borrow().animation_id, 39);
    for _ in 0..4 {
        let _ = expected.next_u15();
    }
    assert_eq!(
        random, expected,
        "both primary selections consume their own variation and cycle draws"
    );
    Ok(())
}

#[test]
#[ignore = "requires SOLARITY_STOCK_DATA_ROOT with the user's 3.3.5a archives"]
fn stock_character_movement_sequences_complete() -> Result<(), Box<dyn Error>> {
    let root = std::env::var_os("SOLARITY_STOCK_DATA_ROOT").ok_or("stock data root")?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(root)?,
        Locale::EnUs,
    )?)?;
    let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
    let world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.,
    ));
    let identity = world.object_identity(7).ok_or("local identity")?;
    let mut count = 0;
    for race in [
        "Human", "Orc", "Dwarf", "NightElf", "Scourge", "Tauren", "Gnome", "Troll", "BloodElf",
        "Draenei",
    ] {
        for gender in ["Male", "Female"] {
            let path = AssetPath::new(format!("Character\\{race}\\{gender}\\{race}{gender}.m2"))?;
            let model = Arc::new(DecodedM2Model::load(&mut store, &path)?);
            let owner =
                UnitAnimationBehavior::new(identity, model, Arc::clone(&animations), input(0), 0);
            let mut random = CrtRand::new();
            let mut time = 100.;
            owner.advance_scene(time, &mut random)?;
            notify(
                &owner,
                movement(0x1000, Some(-7.95555)),
                UnitMovementAnimationEventKind::Jump,
            );
            time += 100.;
            owner.advance_scene(time, &mut random)?;
            assert_eq!(
                owner.behavior(&owner.playback.borrow()),
                37,
                "{path} takeoff"
            );
            time += owner.model.animations().sequences()[owner.playback.borrow().sequence]
                .duration_ms() as f32
                + 1.;
            owner.advance_scene(time, &mut random)?;
            assert_eq!(
                owner.behavior(&owner.playback.borrow()),
                38,
                "{path} jump loop"
            );
            notify(
                &owner,
                movement(0, None),
                UnitMovementAnimationEventKind::Land {
                    previous_flags: 0x1000,
                    forced: true,
                    slow: true,
                },
            );
            time += 100.;
            owner.advance_scene(time, &mut random)?;
            assert_eq!(
                owner.behavior(&owner.playback.borrow()),
                39,
                "{path} landing"
            );
            time += owner.model.animations().sequences()[owner.playback.borrow().sequence]
                .duration_ms() as f32
                + 1.;
            owner.advance_scene(time, &mut random)?;
            assert_eq!(
                owner.behavior(&owner.playback.borrow()),
                0,
                "{path} stand after landing"
            );
            for (flags, expected) in [(0x10, 11), (0x20, 12)] {
                owner.set_input(input(0).with_movement(movement(flags, None)));
                time += 100.;
                owner.advance_scene(time, &mut random)?;
                assert_eq!(
                    owner.behavior(&owner.playback.borrow()),
                    expected,
                    "{path} turn"
                );
            }
            notify(
                &owner,
                movement(1, None),
                UnitMovementAnimationEventKind::Land {
                    previous_flags: 0x3001,
                    forced: false,
                    slow: false,
                },
            );
            time += 100.;
            owner.advance_scene(time, &mut random)?;
            assert_eq!(
                owner.behavior(&owner.playback.borrow()),
                187,
                "{path} running landing"
            );
            time = owner
                .playback
                .borrow()
                .script_timer
                .ok_or("landing timer")?
                .end_time_ms() as f32
                + 1.;
            owner.advance_scene(time, &mut random)?;
            assert_eq!(
                owner.behavior(&owner.playback.borrow()),
                5,
                "{path} run after landing"
            );
            for (flags, run_speed, requested) in [
                (0x101, 7., 4),
                (1, 4., 4),
                (1, 7., 5),
                (1, 10.5, 5),
                (1, 11., 143),
            ] {
                let mut speeds = movement(flags, None).speeds().values();
                speeds[1] = run_speed;
                let next = WorldMovementState::new(
                    u64::from(flags),
                    solarity_ecs::WorldMovementSpeeds::new(speeds),
                    Default::default(),
                );
                let previous_random = random;
                let previous_animation = owner.playback.borrow().animation_id;
                owner.set_input(input(0).with_movement(next));
                time += 100.;
                owner.synchronize(time as u32, &mut random)?;
                {
                    let playback = owner.playback.borrow();
                    let resolved = resolve_unit_model_animation(
                        &animations,
                        UnitLocomotionAnimation::new(requested),
                        UnitAnimationTier::Ground,
                        |id| {
                            owner
                                .model
                                .animations()
                                .available_variation_count(id)
                                .is_some()
                        },
                    )
                    .ok_or("resolved locomotion")?;
                    assert_eq!(
                        playback.animation_id,
                        resolved.animation_id(),
                        "{path} speed {run_speed}"
                    );
                    let index = owner
                        .model
                        .animations()
                        .model_sequence_for_variation(playback.animation_id, 0)
                        .ok_or("locomotion metadata")?;
                    let authored = owner.model.animations().sequences()[index].movement_speed();
                    if matches!(playback.animation_id, 4 | 5 | 143) && authored != 0. {
                        let expected_rate = (f64::from(resolve_unit_movement_speed(next))
                            / f64::from(authored).abs())
                            as f32;
                        assert_eq!(
                            playback.script_timer.ok_or("locomotion timer")?.speed(),
                            expected_rate,
                            "{path}"
                        );
                    }
                    if playback.animation_id == previous_animation {
                        assert_eq!(
                            random, previous_random,
                            "{path} speed change rerolled variation"
                        );
                    }
                }
                owner.advance_scene(time, &mut random)?;
                let sample = owner.take_scene_sample().ok_or("locomotion scene sample")?;
                let pose = solarity_rendering::M2BonePose::compose_with_model_view(
                    owner.model.animations(),
                    sample.advance.clock,
                    glam::Mat4::IDENTITY,
                )?;
                assert!(
                    pose.transforms()
                        .iter()
                        .all(|transform| transform.is_finite()),
                    "{path} speed {run_speed}"
                );
            }
            count += 1;
            assert!(
                owner.model.animations().key_bone(4).is_some(),
                "{path} spine"
            );
            assert!(
                owner.model.animations().key_bone(6).is_some(),
                "{path} head"
            );
            let mut pose = solarity_rendering::M2BonePose::default();
            for flags in [1, 5, 4, 6, 2, 10, 8, 9] {
                let mut moving = input(0).with_movement(movement(flags, None));
                moving.controlled = true;
                moving.facing = 0.7;
                owner.set_input(moving);
                for _ in 0..24 {
                    time += 1000. / 1200.;
                    owner.advance_scene(time, &mut random)?;
                    let sample = owner
                        .take_scene_sample()
                        .ok_or("directional scene sample")?;
                    let body = owner.body_pose();
                    pose.recompose_with_overrides(
                        owner.model.animations(),
                        sample.advance.clock,
                        body.placement_rotation,
                        solarity_rendering::M2BonePoseOverrides {
                            bone_transforms: body.bone_transforms(),
                            ..Default::default()
                        },
                    )?;
                    assert!(
                        pose.transforms().iter().all(|matrix| matrix.is_finite()),
                        "{path} direction {flags}"
                    );
                    for attachment in owner.model.attachments() {
                        let transform = pose.attachment_transform(
                            owner.model.animations(),
                            attachment,
                            sample.advance.clock,
                            body.placement_rotation,
                        )?;
                        assert!(
                            transform.is_none_or(|matrix| matrix.is_finite()),
                            "{path} attachment"
                        );
                    }
                }
            }
        }
    }
    println!(
        "Validated jump, land, turns, speed changes, eight-direction poses and attachments on {count} installed character models"
    );
    Ok(())
}

#[test]
fn ordinary_transitions_complete_on_the_cpu_and_retain_the_shared_timer()
-> Result<(), Box<dyn Error>> {
    for (stand, down, hold, up) in [(1, 96, 97, 98), (3, 99, 100, 101), (8, 114, 115, 116)] {
        let owner = owner(POSES, 0)?;
        let mut random = CrtRand::new();
        owner.advance_scene(100.0, &mut random)?;
        let playback = owner.playback();
        owner.set_input(input(stand));
        owner.advance_scene(200.0, &mut random)?;
        assert_eq!(playback.borrow().animation_id, down);
        owner.take_scene_sample();
        let mut expected = random;
        let _ = expected.next_u15();
        let _ = expected.next_u15();
        owner.advance_scene(1500.0, &mut random)?;
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
        owner.advance_scene(1700.0, &mut random)?;
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
        owner.advance_scene(1800.0, &mut random)?;
        assert_eq!(playback.borrow().animation_id, up);
        owner.advance_scene(3000.0, &mut random)?;
        assert_eq!(playback.borrow().animation_id, 0);
    }
    Ok(())
}

#[test]
fn interrupted_sit_uses_latest_posture_and_identical_input_does_not_restart()
-> Result<(), Box<dyn Error>> {
    let owner = owner(POSES, 1)?;
    let mut random = CrtRand::new();
    owner.advance_scene(100.0, &mut random)?;
    let playback = owner.playback();
    let expected = random;
    owner.set_input(input(1));
    owner.advance_scene(250.0, &mut random)?;
    assert_eq!(playback.borrow().animation_id, 96);
    assert_eq!(random, expected);
    owner.set_input(input(0));
    owner.advance_scene(300.0, &mut random)?;
    assert_eq!(playback.borrow().animation_id, 98);
    owner.advance_scene(1500.0, &mut random)?;
    assert_eq!(playback.borrow().animation_id, 0);
    Ok(())
}

#[test]
fn chair_loops_preserve_random_state_and_locomotion_overrides_posture() -> Result<(), Box<dyn Error>>
{
    for (stand, animation) in [(4, 102), (5, 103), (6, 104)] {
        let owner = owner(POSES, stand)?;
        let mut random = CrtRand::new();
        owner.advance_scene(100.0, &mut random)?;
        let expected = random;
        owner.advance_scene(3500.0, &mut random)?;
        assert_eq!(owner.playback.borrow().animation_id, animation);
        assert_eq!(random, expected);
        let mut moving = input(stand);
        moving.locomotion = UnitLocomotionAnimation::new(4);
        owner.set_input(moving);
        owner.advance_scene(3600.0, &mut random)?;
        assert_eq!(owner.playback.borrow().animation_id, 4);
        moving.mounted = true;
        owner.set_input(moving);
        owner.advance_scene(3700.0, &mut random)?;
        assert_eq!(owner.playback.borrow().animation_id, 91);
    }
    Ok(())
}

#[test]
fn unavailable_pose_uses_actual_fallback_behavior_and_failed_request_remains_pending()
-> Result<(), Box<dyn Error>> {
    let owner = owner(&[0], 1)?;
    let mut random = CrtRand::new();
    owner.advance_scene(100.0, &mut random)?;
    assert_eq!(owner.playback.borrow().animation_id, 0);
    assert_eq!(owner.behavior(&owner.playback.borrow()), 0);
    let expected = random;
    owner.advance_scene(2500.0, &mut random)?;
    assert_eq!(owner.playback.borrow().animation_id, 0);
    assert_eq!(random, expected);
    let failed = self::owner(&[4], 1)?;
    assert!(failed.synchronize(100, &mut random).is_err());
    assert_eq!(failed.pending.borrow().len(), 1);
    assert_eq!(failed.processed_stand.get(), 0);
    Ok(())
}

#[test]
fn replicated_death_predicate_and_entry_match_original_executable() -> Result<(), Box<dyn Error>> {
    let ids = [
        0, 1, 6, 8, 9, 37, 131, 132, 133, 465, 466, 467, 468, 469, 472, 473,
    ];
    let owner = owner(&ids, 0)?;
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.0,
    ));
    world.create_object(7, solarity_ecs::ObjectKind::Player, None, [])?;
    let mut cases = 0;
    for line in include_str!("../fixtures/unit_death_native.txt").lines() {
        let words = line.split_whitespace().collect::<Vec<_>>();
        match words.first().copied() {
            Some("dead") => {
                let health = u32::from_str_radix(words[1], 16)?;
                let secondary = u32::from_str_radix(words[2], 16)?;
                solarity_systems::project_object_fields(
                    &mut world,
                    7,
                    [(24, health), (60, secondary)],
                )?;
                let input = input(words[3].parse()?).with_orientation(&world, 7, false, false);
                assert_eq!(input.dead(), words[4] == "1", "{line}");
            }
            Some("entry") => {
                let mut playback = owner.playback.borrow_mut();
                playback.animation_id = words[1].parse()?;
                let mut input = input(0);
                input.alive = false;
                input.movement_flags = u32::from_str_radix(words[2], 16)?;
                let request = owner
                    .transition_request(input, &playback)
                    .map_or(-1, i32::from);
                assert_eq!(request, words[3].parse::<i32>()?, "{line}");
            }
            _ => continue,
        }
        cases += 1;
    }
    assert_eq!(cases, 152);
    Ok(())
}

#[test]
fn replicated_health_drives_death_without_changing_stand_and_prediction_cannot_kill()
-> Result<(), Box<dyn Error>> {
    for (flags, entry, corpse) in [(0, 1, 6), (0x200000, 131, 132)] {
        let mut world = ActiveWorld::enter(WorldBootstrap::new(
            WorldMapId::new(0),
            7,
            "Local",
            Vec3::ZERO,
            0.0,
        ));
        world.create_object(7, solarity_ecs::ObjectKind::Player, None, [])?;
        solarity_systems::project_object_fields(&mut world, 7, [(24, 100), (32, 100)])?;
        let current = || input(0).with_movement(movement(flags, None));
        let owner = owner_with_input(
            &[0, 1, 6, 9, 131, 132],
            current().with_orientation(&world, 7, true, false),
        )?;
        let mut random = CrtRand::new();
        owner.advance_scene(100.0, &mut random)?;
        let player = world.local_player();
        world
            .storage_mut()
            .add_component(player, (solarity_ecs::UnitHealthPrediction::new(-100),));
        owner.set_input(current().with_orientation(&world, 7, true, false));
        owner.advance_scene(200.0, &mut random)?;
        assert!(owner.processed_alive.get());
        for health in [0, u32::MAX] {
            solarity_systems::project_object_fields(&mut world, 7, [(24, health)])?;
            owner.set_input(current().with_orientation(&world, 7, true, false));
            owner.advance_scene(300.0, &mut random)?;
            assert_eq!(owner.playback.borrow().animation_id, entry);
            assert_eq!(owner.input.get().stand, 0);
        }
        let retained = random;
        owner.request_visual_kit_animation(9);
        let mut changed = current().with_orientation(&world, 7, true, false);
        changed.stand = 9;
        owner.set_input(changed);
        owner.advance_scene(400.0, &mut random)?;
        assert_eq!(owner.playback.borrow().animation_id, entry);
        assert_eq!(random, retained);
        owner.advance_scene(1500.0, &mut random)?;
        assert_eq!(owner.playback.borrow().animation_id, corpse);
        solarity_systems::project_object_fields(&mut world, 7, [(24, 100)])?;
        owner.set_input(current().with_orientation(&world, 7, true, false));
        owner.advance_scene(1600.0, &mut random)?;
        assert!(owner.processed_alive.get());
        assert!(!matches!(
            owner.behavior(&owner.playback.borrow()),
            1 | 6 | 131 | 132
        ));
    }
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
        owner.advance_scene(100.0, &mut random)?;
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
        owner.advance_scene(200.0, &mut random)?;
        assert_eq!(owner.playback.borrow().animation_id, 466);
        owner.advance_scene(1500.0, &mut random)?;
        assert_eq!(owner.playback.borrow().animation_id, 468);
        let mut expected = random;
        if corpse_variants == 1 {
            let _variation = expected.next_u15();
        }
        let _cycle = expected.next_u15();
        owner.advance_scene(2600.0, &mut random)?;
        assert_eq!(owner.playback.borrow().animation_id, 472);
        assert_eq!(
            owner.model.animations().sequences()[owner.playback.borrow().sequence]
                .variation_index(),
            corpse_variants + 16
        );
        assert_eq!(random, expected);
        owner.advance_scene(4000.0, &mut random)?;
        owner.advance_scene(8000.0, &mut random)?;
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
        owner.advance_scene(100.0, &mut random)?;
        assert_eq!(owner.playback.borrow().animation_id, start);
        let mut expected = random;
        let _cycle = expected.next_u15();
        owner.advance_scene(1500.0, &mut random)?;
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
        owner.advance_scene(100.0, &mut random)?;
        assert_eq!(owner.playback.borrow().animation_id, 201);
        owner.advance_scene(1500.0, &mut random)?;
        assert_eq!(owner.playback.borrow().animation_id, 202);
        owner.set_input(input(0));
        owner.advance_scene(1600.0, &mut random)?;
        assert_eq!(owner.playback.borrow().animation_id, exit);
        owner.advance_scene(2800.0, &mut random)?;
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
    owner.advance_scene(100.0, &mut random)?;
    assert_eq!(owner.playback.borrow().animation_id, 300);
    owner.advance_scene(1500.0, &mut random)?;
    assert_eq!(owner.playback.borrow().animation_id, 301);
    pose.stand = 0;
    owner.set_input(pose);
    owner.advance_scene(1600.0, &mut random)?;
    assert_eq!(owner.playback.borrow().animation_id, 302);
    owner.advance_scene(2800.0, &mut random)?;
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
        owner.advance_scene(100.0, &mut random)?;
        let mut expected_random = random;
        let _cycle = expected_random.next_u15();
        owner.advance_scene(1500.0, &mut random)?;
        assert_eq!(owner.playback.borrow().animation_id, expected_id);
        assert_eq!(owner.playback.borrow().script_mode, mode);
        assert_eq!(random, expected_random);
        owner.advance_scene(5000.0, &mut random)?;
        assert_eq!(random, expected_random);
    }
    Ok(())
}
