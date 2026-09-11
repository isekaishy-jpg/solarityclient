//! Deterministic Model playback tests using archive-decoded sequence records.

#[path = "model_bone_playback.rs"]
mod bones;

use std::error::Error;
use std::io::Cursor;

use solarity_asset::{
    AnimationDataCatalog, ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedM2Model,
    Locale,
};
use solarity_rendering::{M2BonePose, triggered_m2_event_indices};
use wow_m2::header::M2Header;
use wow_m2::skin::OldSkinHeader;
use wow_m2::{M2Model, M2Version, OldSkin};

use super::M2Playback;
use crate::random::CrtRand;
use crate::test_support::ClientFixture;

#[test]
fn attachment_clock_queries_preserve_overdue_callbacks_events_and_variation_randomness()
-> Result<(), Box<dyn Error>> {
    let (model, catalog) = playback_model()?;
    let mut random = CrtRand::new();
    let mut queried = M2Playback::default_sequence(&model, &catalog, 0, &mut random)?;
    queried.apply_model_sequence(&model, &catalog, 0, 0, 0, &mut random)?;
    queried.clock(&model, 250., &mut random)?;
    queried.event_window(250.);
    let mut control = queried.clone();
    let mut control_random = random;
    let sequence = queried.sequence;
    for now in [350, 1001, 2200, 2200] {
        let clock = queried.sample_clock(now);
        assert_eq!(clock.sequence(), sequence);
        assert_eq!(clock.global_time_ms(), now as f32);
        assert_eq!(
            clock.animation_time_ms(),
            control.script_timer.ok_or("timer")?.animation_time_ms(now) as f32
        );
    }
    let mut calls = 0;
    let mut control_calls = 0;
    let actual = queried.clock_with_completion(
        &model,
        2200.,
        &mut random,
        Some(&mut |_, _| {
            calls += 1;
            Ok(())
        }),
    )?;
    let expected = control.clock_with_completion(
        &model,
        2200.,
        &mut control_random,
        Some(&mut |_, _| {
            control_calls += 1;
            Ok(())
        }),
    )?;
    assert!(calls > 0);
    assert_eq!(calls, control_calls);
    assert_eq!(actual.clock, expected.clock);
    assert_eq!(actual.expired_variations.len(), 2);
    for (actual, expected) in actual
        .expired_variations
        .iter()
        .zip(&expected.expired_variations)
    {
        assert_eq!(actual.clock, expected.clock);
        assert_eq!(
            triggered_m2_event_indices(model.animations(), actual.event_window),
            triggered_m2_event_indices(model.animations(), expected.event_window),
        );
    }
    assert_eq!(queried.event_window(2200.), control.event_window(2200.));
    assert_eq!(random, control_random);
    queried.set_paused(true, 2300);
    let frozen = queried.sample_clock(2300);
    let later = queried.sample_clock(3500);
    assert_eq!(frozen.animation_time_ms(), later.animation_time_ms());
    assert_eq!(later.global_time_ms() - frozen.global_time_ms(), 1200.);
    Ok(())
}

#[test]
fn model_global_origin_survives_primary_seek_pause_and_scene_wrap() -> Result<(), Box<dyn Error>> {
    let (model, catalog) = default_sequence_model(&[0, 7], 0, 0, 1)?;
    for (created, now, expected) in [(5000_u32, 5101_u32, 101_u32), (0xfffffff0, 16, 32)] {
        let mut random = CrtRand::new();
        let mut playback = M2Playback::default_sequence(&model, &catalog, created, &mut random)?;
        assert_eq!(playback.global_tick(now), expected);
        let initial = playback.clock(&model, now as f32, &mut random)?.clock;
        assert_eq!(initial.global_time_ms(), expected as f32);
        playback.apply_model_sequence(&model, &catalog, 7, 25, now, &mut random)?;
        assert_eq!(
            playback
                .clock(&model, now as f32, &mut random)?
                .clock
                .global_time_ms(),
            expected as f32
        );
        playback.set_paused(true, now);
        let later = now.wrapping_add(1000);
        let paused = playback.clock(&model, later as f32, &mut random)?.clock;
        assert_eq!(paused.global_time_ms(), (expected + 1000) as f32);
        let mut fresh = M2Playback::default_sequence(&model, &catalog, later, &mut random)?;
        assert_eq!(
            fresh
                .clock(&model, later as f32, &mut random)?
                .clock
                .global_time_ms(),
            0.0
        );
    }
    Ok(())
}

#[test]
fn unstarted_model_does_not_dispatch_sequence_zero_events() -> Result<(), Box<dyn Error>> {
    let (model, _) = playback_model()?;
    let mut playback = M2Playback::unstarted(0, 0);
    let mut random = CrtRand::new();
    let original_random = random;
    playback.clock(&model, 20_000.0, &mut random)?;
    assert!(
        triggered_m2_event_indices(model.animations(), playback.event_window(20_000.0)).is_empty()
    );
    assert_eq!(random, original_random);
    Ok(())
}

/// Original 834540 -> 832AB0 probes, including a zero-weight variation zero.
#[test]
fn default_sequence_matches_native_selection_fallbacks_and_random_draws()
-> Result<(), Box<dyn Error>> {
    use solarity_asset::M2ModelAnimationMode::{Forward, HoldEnd, HoldStart, Reverse};
    for (ids, fallback, flags, bone_count, selected, mode) in [
        (&[0, 0, 7][..], 0, 0, 1, Some(1), Forward),
        (&[7, 147][..], 0, 0, 1, Some(1), Forward),
        (&[7][..], 0, 0, 1, Some(0), Forward),
        (&[7][..], 7, 0x10, 1, Some(0), Reverse),
        (&[7][..], 7, 0x20, 1, Some(0), HoldEnd),
        (&[7][..], 7, 0x30, 1, Some(0), HoldStart),
        (&[0][..], 0, 0, 0, None, Forward),
        (&[][..], 0, 0, 1, None, Forward),
    ] {
        let (model, catalog) = default_sequence_model(ids, fallback, flags, bone_count)?;
        let mut random = CrtRand::new();
        let mut playback = M2Playback::default_sequence(&model, &catalog, 20_000, &mut random)?;
        let mut expected_random = CrtRand::new();
        if let Some(selected) = selected {
            let _variation = expected_random.next_u15();
            let _cycles = expected_random.next_u15();
            assert_eq!(playback.sequence, selected, "{ids:?}");
            assert_eq!(playback.animation_id, ids[selected]);
            assert_eq!(playback.script_mode, mode);
            let timer = playback
                .script_timer
                .ok_or("missing native default timer")?;
            assert_eq!(timer.start_time_ms(), 20_001);
            assert_eq!(timer.cycle_count(), 2);
            assert_eq!(
                timer.end_time_ms(),
                if matches!(mode, HoldStart | HoldEnd) {
                    20_001
                } else {
                    22_001
                }
            );
            assert!(playback.script_blend.is_none());
            let advance = playback.clock(&model, 20_101.0, &mut random)?;
            assert_eq!(
                advance.clock.animation_time_ms(),
                match mode {
                    Forward => 100.0,
                    Reverse => 900.0,
                    // These authored clips loop: an exact endpoint wraps to zero.
                    HoldEnd | HoldStart => 0.0,
                }
            );
        } else {
            assert!(playback.script_timer.is_none());
        }
        assert_eq!(random, expected_random, "{ids:?}, bones={bone_count}");
    }
    Ok(())
}

#[test]
fn unit_effect_load_callback_matches_original_timers_and_random_draws() -> Result<(), Box<dyn Error>>
{
    use solarity_rendering::M2SequenceStartPhase::{BeforeSceneUpdate, DuringSceneUpdate};
    for (phase, fixture) in [
        (
            BeforeSceneUpdate,
            include_str!("../fixtures/unit_effect_load_before.txt"),
        ),
        (
            DuringSceneUpdate,
            include_str!("../fixtures/unit_effect_load_during.txt"),
        ),
    ] {
        let mut count = 0;
        for line in fixture.lines().filter(|line| !line.starts_with('#')) {
            let (input, expected) = line.split_once(" -> ").ok_or("native record")?;
            let (ids, input) = input.split_once(']').ok_or("native animation ids")?;
            let ids = ids
                .trim_start_matches('[')
                .split(',')
                .map(|id| id.trim().parse::<u16>())
                .collect::<Result<Vec<_>, _>>()?;
            let input = input
                .split_whitespace()
                .map(str::parse::<u32>)
                .collect::<Result<Vec<_>, _>>()?;
            let (model, catalog) = default_sequence_model(&ids, input[0], input[1], input[2])?;
            let expected = expected.split(';').collect::<Vec<_>>();
            let rolls = expected[1].trim().parse::<usize>()?;
            let words = expected[2]
                .trim()
                .trim_matches(['(', ')'])
                .split(',')
                .map(|word| word.trim().parse::<u32>())
                .collect::<Result<Vec<_>, _>>()?;
            let mut random = CrtRand::new();
            let playback = M2Playback::unit_effect_default_sequence(
                &model,
                &catalog,
                20_000,
                20_000,
                phase,
                &mut random,
            )?;
            let mut expected_random = CrtRand::new();
            for _ in 0..rolls {
                let _ = expected_random.next_u15();
            }
            assert_eq!(random, expected_random, "{line}");
            if words[0] & 0xffff == 0xffff {
                assert!(playback.script_timer.is_none());
            } else {
                let timer = playback.script_timer.ok_or("effect primary timer")?;
                assert_eq!(playback.sequence as u32, words[0] & 0xffff);
                assert_eq!(
                    [
                        timer.start_time_ms(),
                        timer.end_time_ms(),
                        timer.speed().to_bits(),
                        timer.cycle_count()
                    ],
                    [words[1], words[2], words[3], words[6]],
                    "{line}"
                );
                assert_eq!(
                    playback
                        .script_blend
                        .map_or(u32::from(u16::MAX), |blend| blend.sequence() as u32),
                    words[8],
                    "{line}"
                );
            }
            count += 1;
        }
        assert_eq!(count, 8);
    }
    Ok(())
}

#[test]
fn mount_requests_preserve_weighted_primaries_and_replace_default_fallback_modes()
-> Result<(), Box<dyn Error>> {
    use solarity_asset::M2ModelAnimationMode::Forward;
    let (model, catalog) = default_sequence_model(&[0, 0, 7], 0, 0, 1)?;
    let mut random = CrtRand::new();
    let mut playback = M2Playback::default_sequence(&model, &catalog, 20_000, &mut random)?;
    assert_eq!(playback.sequence, 1, "variation zero has no weight");
    let initial = playback.script_timer.ok_or("mount timer")?;
    let unchanged_random = random;
    for now in [20_000., 20_001., 20_101.] {
        playback.select_mount_animation(&model, 0, (1., 0), now, &mut random)?;
        assert_eq!(playback.sequence, 1);
        assert_eq!(playback.script_timer, Some(initial));
        assert_eq!(playback.previous_event_scene_time_ms, 20_000);
        assert_eq!(random, unchanged_random);
    }
    playback.select_mount_animation(&model, 7, (1., 0), 20_200., &mut random)?;
    assert_eq!(playback.sequence, 2);
    assert_eq!(
        playback.script_timer.ok_or("new timer")?.start_time_ms(),
        20_201
    );
    assert_eq!(playback.sample_clock(20_301).animation_time_ms(), 100.);
    let mut expected_random = unchanged_random;
    let _variation = expected_random.next_u15();
    let _cycles = expected_random.next_u15();
    assert_eq!(random, expected_random);

    // Constructor fallback operations belong to Stand's DBC traversal. A
    // resolved Unit_C locomotion clip is an explicit forward request, even
    // when it happens to have the same identifier as that constructor fallback.
    for flags in [0x10, 0x20, 0x30] {
        let (model, catalog) = default_sequence_model(&[7], 7, flags, 1)?;
        let mut playback = M2Playback::default_sequence(&model, &catalog, 30_000, &mut random)?;
        assert_ne!(playback.script_mode, Forward);
        playback.select_mount_animation(&model, 7, (1., 0), 30_100., &mut random)?;
        assert_eq!(playback.script_mode, Forward);
        assert_eq!(
            playback
                .script_timer
                .ok_or("forward mount timer")?
                .start_time_ms(),
            30_101
        );
        assert_eq!(playback.sample_clock(30_301).animation_time_ms(), 200.);
    }
    Ok(())
}

#[test]
fn mount_rate_requests_match_native_submission_and_random_consumption() -> Result<(), Box<dyn Error>>
{
    use solarity_asset::M2ModelAnimationMode::Forward;
    use solarity_rendering::{M2ModelSequenceTimer, M2SequenceStartPhase::BeforeSceneUpdate};
    let (model, _) = default_sequence_model(&[4, 5], 0, 0, 1)?;
    let mut checked = 0;
    for line in include_str!("../fixtures/unit_mount_request_native.txt").lines() {
        if line.starts_with('#') {
            continue;
        }
        let words = line.split_whitespace().collect::<Vec<_>>();
        // The renderer has already admitted a resident mount and resolved a
        // valid animation. The fixture also records the native absent/disabled
        // guards, which belong to upstream unit policy.
        if words[0] != "1" || words[1] != "1" || words[3] == "4294967295" {
            continue;
        }
        let old_id = words[2].parse()?;
        let new_id = words[3].parse()?;
        let old_speed = f32::from_bits(u32::from_str_radix(words[4], 16)?);
        let new_speed = f32::from_bits(u32::from_str_radix(words[5], 16)?);
        let offset = words[7].parse()?;
        let submitted = words[8] == "1";
        let mut random = CrtRand::new();
        let mut playback = M2Playback::unstarted(0, 20_000);
        playback.apply_resolved_model_sequence_variation(
            &model,
            old_id,
            None,
            Forward,
            old_speed,
            0,
            20_000,
            BeforeSceneUpdate,
            true,
            &mut random,
        )?;
        let previous = playback.script_timer;
        let mut expected_random = random;
        playback.select_mount_animation(
            &model,
            new_id,
            (new_speed, offset),
            20_500.,
            &mut random,
        )?;
        if submitted {
            let _variation = expected_random.next_u15();
            let cycles = expected_random.next_u15();
            let sequence = &model.animations().sequences()[playback.sequence];
            let timer = M2ModelSequenceTimer::with_speed(
                sequence,
                Forward,
                new_speed,
                20_500,
                offset,
                cycles,
                BeforeSceneUpdate,
            );
            assert_eq!(playback.script_timer, Some(timer), "{line}");
            assert_eq!(playback.animation_id, new_id, "{line}");
        } else {
            assert_eq!(playback.script_timer, previous, "{line}");
        }
        assert_eq!(random, expected_random, "{line}");
        checked += 1;
    }
    assert_eq!(checked, 40);
    Ok(())
}

fn default_sequence_model(
    ids: &[u16],
    fallback: u32,
    flags: u32,
    bone_count: u32,
) -> Result<(DecodedM2Model, AnimationDataCatalog), Box<dyn Error>> {
    use crate::test_support::game_object_models as models;
    let mut bytes = models::model_with_animations(ids)?;
    let sequences = u32::from_le_bytes(bytes[0x20..0x24].try_into()?) as usize;
    for index in 0..ids.len() {
        let record = sequences + index * 64;
        bytes[record + 2..record + 4].copy_from_slice(&(index as u16).to_le_bytes());
        bytes[record + 24..record + 28].copy_from_slice(&3_u32.to_le_bytes());
        if ids.starts_with(&[0, 0]) && index == 0 {
            bytes[record + 16..record + 20].copy_from_slice(&0_u32.to_le_bytes());
            bytes[record + 60..record + 62].copy_from_slice(&1_u16.to_le_bytes());
        }
    }
    bytes[0x2c..0x30].copy_from_slice(&bone_count.to_le_bytes());
    let mut dbc = b"WDBC".to_vec();
    for value in [1_u32, 8, 32, 1, 0, 0, 0, 0, flags, fallback, 0, 0] {
        dbc.extend_from_slice(&value.to_le_bytes());
    }
    dbc.push(0);
    let fixture = ClientFixture::with_common_files(&[
        ("World\\GameObject.m2", &bytes),
        ("World\\GameObject00.skin", &models::skin()?),
        ("DBFilesClient\\AnimationData.dbc", &dbc),
    ])?;
    let archive =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(archive)?;
    Ok((
        DecodedM2Model::load(&mut store, &AssetPath::new("World\\GameObject.m2")?)?,
        AnimationDataCatalog::load(&mut store)?,
    ))
}

#[test]
fn explicit_variation_retains_its_ordinal_across_loop_boundaries() -> Result<(), Box<dyn Error>> {
    let (model, _) = playback_model()?;
    let mut random = CrtRand::new();
    let mut expected = random;
    let _cycle = expected.next_u15();
    let mut playback = M2Playback::unstarted(0, 0);
    playback.apply_resolved_model_sequence_variation(
        &model,
        0,
        Some(1),
        solarity_asset::M2ModelAnimationMode::Forward,
        1.0,
        0,
        100,
        solarity_rendering::M2SequenceStartPhase::BeforeSceneUpdate,
        true,
        &mut random,
    )?;
    assert_eq!(playback.sequence, 1);
    assert_eq!(random, expected);
    playback.clock(&model, 5000.0, &mut random)?;
    assert_eq!(playback.sequence, 1);
    assert_eq!(
        random, expected,
        "832AB0's explicit variation disables automatic rolls"
    );
    Ok(())
}

/// A user completion replaces the timer before the automatic variation branch.
#[test]
fn primary_completion_runs_before_variation_and_starts_at_current_scene_tick()
-> Result<(), Box<dyn Error>> {
    let (model, catalog) = playback_model()?;
    let mut random = CrtRand::new();
    let mut playback = M2Playback::unstarted(0, 0);
    playback.apply_model_sequence(&model, &catalog, 7, 0, 0, &mut random)?;
    let mut expected_random = random;
    let _variation = expected_random.next_u15();
    let _cycle = expected_random.next_u15();
    let mut completions = 0;
    let mut completed = |playback: &mut M2Playback, random: &mut CrtRand| {
        completions += 1;
        assert!(playback.script_finished);
        playback.apply_resolved_model_sequence(
            &model,
            0,
            solarity_asset::M2ModelAnimationMode::Forward,
            0,
            playback.scene_time_ms,
            solarity_rendering::M2SequenceStartPhase::DuringSceneUpdate,
            true,
            random,
        )?;
        Ok(())
    };
    let advance =
        playback.clock_with_completion(&model, 2_500.0, &mut random, Some(&mut completed))?;
    assert_eq!(completions, 1);
    assert_eq!(advance.expired_variations.len(), 1);
    assert_eq!(advance.expired_variations[0].clock.sequence(), 2);
    assert_eq!(advance.clock.animation_time_ms(), 0.0);
    assert_eq!(
        playback
            .script_timer
            .ok_or("missing replacement")?
            .start_time_ms(),
        2_500
    );
    assert_eq!(
        random, expected_random,
        "replacement must suppress automatic reroll"
    );
    assert!(playback.script_blend.is_some());
    Ok(())
}

/// Retaining a terminal timer marks it finished; pause blocks both events and completion.
#[test]
fn primary_completion_is_once_and_pause_moves_the_retained_clock() -> Result<(), Box<dyn Error>> {
    let (model, catalog) = playback_model()?;
    let mut random = CrtRand::new();
    let mut playback = M2Playback::unstarted(0, 0);
    playback.apply_model_sequence(&model, &catalog, 7, 0, 0, &mut random)?;
    playback.clock(&model, 250.0, &mut random)?;
    playback.event_window(250.0);
    playback.set_paused(true, 250);
    let expected_random = random;
    let mut completions = 0;
    let mut completed = |_: &mut M2Playback, _: &mut CrtRand| {
        completions += 1;
        Ok(())
    };
    let advance =
        playback.clock_with_completion(&model, 1_250.0, &mut random, Some(&mut completed))?;
    assert_eq!(advance.clock.animation_time_ms(), 249.0);
    assert!(advance.expired_variations.is_empty());
    assert!(
        triggered_m2_event_indices(model.animations(), playback.event_window(1_250.0),).is_empty()
    );
    playback.set_paused(false, 1_250);
    let advance =
        playback.clock_with_completion(&model, 3_000.0, &mut random, Some(&mut completed))?;
    assert_eq!(advance.expired_variations.len(), 1);
    playback.event_window(3_000.0);
    let advance =
        playback.clock_with_completion(&model, 4_000.0, &mut random, Some(&mut completed))?;
    assert!(advance.expired_variations.is_empty());
    assert_eq!(completions, 1);
    assert_eq!(random, expected_random);
    Ok(())
}

/// Two looping variations plus a terminal sequence with reverse/hold fallback rows.
fn playback_model() -> Result<(DecodedM2Model, AnimationDataCatalog), Box<dyn Error>> {
    playback_model_with_blend(400)
}

fn playback_model_with_blend(
    blend_ms: u32,
) -> Result<(DecodedM2Model, AnimationDataCatalog), Box<dyn Error>> {
    let mut source = M2Model {
        header: M2Header::new(M2Version::WotLK),
        name: Some("Playback".to_owned()),
        ..M2Model::default()
    };
    source.header.num_skin_profiles = Some(1);
    let mut cursor = Cursor::new(Vec::new());
    source.write(&mut cursor)?;
    let mut bytes = cursor.into_inner();
    let name = bytes.len() as u32;
    bytes.extend_from_slice(b"Playback\0");
    bytes[8..12].copy_from_slice(&9_u32.to_le_bytes());
    bytes[12..16].copy_from_slice(&name.to_le_bytes());
    let sequences = bytes.len() as u32;
    for (id, variation, duration, flags, next, cycles) in [
        (0_u16, 0_u16, 1_000_u32, 0x20_u32, 1_u16, 2_u32),
        (0, 1, 1_000, 0x20, u16::MAX, 2),
        (7, 0, 600, 0x21, u16::MAX, 1),
    ] {
        let mut sequence = [0_u8; 64];
        sequence[0..2].copy_from_slice(&id.to_le_bytes());
        sequence[2..4].copy_from_slice(&variation.to_le_bytes());
        sequence[4..8].copy_from_slice(&duration.to_le_bytes());
        sequence[12..16].copy_from_slice(&flags.to_le_bytes());
        sequence[16..20].copy_from_slice(&16_384_u32.to_le_bytes());
        sequence[20..24].copy_from_slice(&cycles.to_le_bytes());
        sequence[24..28].copy_from_slice(&cycles.to_le_bytes());
        sequence[28..32].copy_from_slice(&blend_ms.to_le_bytes());
        sequence[60..62].copy_from_slice(&next.to_le_bytes());
        bytes.extend_from_slice(&sequence);
    }
    bytes[0x1c..0x20].copy_from_slice(&3_u32.to_le_bytes());
    bytes[0x20..0x24].copy_from_slice(&sequences.to_le_bytes());
    let bones = bytes.len() as u32;
    let mut bone = [0_u8; 88];
    bone[0..4].copy_from_slice(&(-1_i32).to_le_bytes());
    for offset in [8, 18, 38, 58] {
        bone[offset..offset + 2].copy_from_slice(&u16::MAX.to_le_bytes());
    }
    bytes.extend_from_slice(&bone);
    for (key, parent) in [(4_i32, 0_u16), (26, 1)] {
        bone[0..4].copy_from_slice(&key.to_le_bytes());
        bone[8..10].copy_from_slice(&parent.to_le_bytes());
        bytes.extend_from_slice(&bone);
    }
    bytes[0x2c..0x30].copy_from_slice(&3_u32.to_le_bytes());
    bytes[0x30..0x34].copy_from_slice(&bones.to_le_bytes());
    let lookup = bytes.len() as u32;
    for key in 0..27 {
        bytes.extend_from_slice(
            &(match key {
                4 => 1_u16,
                26 => 2,
                _ => u16::MAX,
            })
            .to_le_bytes(),
        );
    }
    bytes[0x34..0x38].copy_from_slice(&27_u32.to_le_bytes());
    bytes[0x38..0x3c].copy_from_slice(&lookup.to_le_bytes());
    append_playback_translation(&mut bytes, bones as usize + 16);
    append_playback_sound_event(&mut bytes);
    let skin = OldSkin {
        header: OldSkinHeader::new(),
        indices: Vec::new(),
        triangles: Vec::new(),
        bone_indices: Vec::new(),
        submeshes: Vec::new(),
        batches: Vec::new(),
    };
    let mut skin_bytes = Cursor::new(Vec::new());
    skin.write(&mut skin_bytes)?;
    let mut dbc = b"WDBC".to_vec();
    for value in [2_u32, 8, 32, 1] {
        dbc.extend_from_slice(&value.to_le_bytes());
    }
    for (id, flags) in [(5_u32, 0x20_u32), (6, 0x10)] {
        for value in [id, 0, 0, 0, flags, 7, id, 0] {
            dbc.extend_from_slice(&value.to_le_bytes());
        }
    }
    dbc.push(0);
    let fixture = ClientFixture::with_common_files(&[
        ("Solarity\\Playback.m2", &bytes),
        ("Solarity\\Playback00.skin", skin_bytes.get_ref()),
        ("DBFilesClient\\AnimationData.dbc", &dbc),
    ])?;
    let archive =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(archive)?;
    Ok((
        DecodedM2Model::load(&mut store, &AssetPath::new("Solarity\\Playback.m2")?)?,
        AnimationDataCatalog::load(&mut store)?,
    ))
}

/// Sequence-specific root positions expose whether an automatic blend survives.
fn append_playback_translation(bytes: &mut Vec<u8>, track: usize) {
    let timestamps = bytes.len() as u32;
    for time in [0_u32, 1_000] {
        bytes.extend_from_slice(&time.to_le_bytes());
    }
    let mut values = Vec::new();
    for base in [0.0_f32, 100.0, 500.0] {
        values.push(bytes.len() as u32);
        for value in [base, 0.0, 0.0, base + 100.0, 0.0, 0.0] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    let time_refs = bytes.len() as u32;
    for _ in 0..3 {
        bytes.extend_from_slice(&2_u32.to_le_bytes());
        bytes.extend_from_slice(&timestamps.to_le_bytes());
    }
    let value_refs = bytes.len() as u32;
    for offset in values {
        bytes.extend_from_slice(&2_u32.to_le_bytes());
        bytes.extend_from_slice(&offset.to_le_bytes());
    }
    bytes[track..track + 2].copy_from_slice(&1_u16.to_le_bytes());
    for (offset, value) in [(4, 3), (8, time_refs), (12, 3), (16, value_refs)] {
        bytes[track + offset..track + offset + 4].copy_from_slice(&value.to_le_bytes());
    }
}

/// A sound callback at 100 ms exposes hidden-time catch-up after login reappears.
fn append_playback_sound_event(bytes: &mut Vec<u8>) {
    let timestamp = bytes.len() as u32;
    bytes.extend_from_slice(&100_u32.to_le_bytes());
    let channels = bytes.len() as u32;
    for _ in 0..3 {
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&timestamp.to_le_bytes());
    }
    let event = bytes.len() as u32;
    let mut record = [0_u8; 36];
    record[..4].copy_from_slice(b"$SND");
    record[26..28].copy_from_slice(&u16::MAX.to_le_bytes());
    record[28..32].copy_from_slice(&3_u32.to_le_bytes());
    record[32..36].copy_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&record);
    bytes[0x100..0x104].copy_from_slice(&1_u32.to_le_bytes());
    bytes[0x104..0x108].copy_from_slice(&event.to_le_bytes());
}

/// 0x00826B00 anchors a new sequence without aging it through earlier world time.
#[test]
fn newly_streamed_world_model_starts_at_its_admission_time() -> Result<(), Box<dyn Error>> {
    let (model, catalog) = playback_model()?;
    let mut random = CrtRand::new();
    let mut playback = M2Playback::default_sequence(&model, &catalog, 60_000, &mut random)?;
    let mut expected_random = random;
    let advance = playback.clock(&model, 60_001.0, &mut random)?;
    assert_eq!(advance.clock.animation_time_ms(), 0.0);
    assert!(advance.expired_variations.is_empty());
    assert!(
        triggered_m2_event_indices(model.animations(), playback.event_window(60_001.0)).is_empty()
    );
    let advance = playback.clock(&model, 60_101.0, &mut random)?;
    assert_eq!(advance.clock.animation_time_ms(), 100.0);
    assert_eq!(
        triggered_m2_event_indices(model.animations(), playback.event_window(60_101.0)),
        vec![0]
    );
    assert_eq!(random.next_u15(), expected_random.next_u15());
    Ok(())
}

/// AccountLogin_OnShow and 0x00826B00 restart against the current scene tick.
#[test]
fn model_show_restarts_after_hidden_time_without_replaying_sound_or_variations()
-> Result<(), Box<dyn Error>> {
    let (model, catalog) = playback_model()?;
    let mut random = CrtRand::new();
    let mut playback = M2Playback::default_sequence(&model, &catalog, 0, &mut random)?;
    playback.apply_model_sequence(&model, &catalog, 0, 0, 0, &mut random)?;
    playback.clock(&model, 250.0, &mut random)?;
    playback.event_window(250.0);
    // No model update occurs during the minute spent on another Glue screen.
    playback.apply_model_sequence(&model, &catalog, 0, 0, 60_000, &mut random)?;
    let mut expected_random = random;
    let advance = playback.clock(&model, 60_001.0, &mut random)?;
    assert_eq!(advance.clock.animation_time_ms(), 0.0);
    assert!(advance.expired_variations.is_empty());
    assert!(
        triggered_m2_event_indices(model.animations(), playback.event_window(60_001.0),).is_empty()
    );
    playback.clock(&model, 60_101.0, &mut random)?;
    assert_eq!(
        triggered_m2_event_indices(model.animations(), playback.event_window(60_101.0),),
        vec![0]
    );
    assert_eq!(random.next_u15(), expected_random.next_u15());
    Ok(())
}

/// Equal requests consume separate variation/cycle rolls and establish fresh timers.
#[test]
fn model_requests_restart_seek_and_preserve_the_native_random_stream() -> Result<(), Box<dyn Error>>
{
    let (model, catalog) = playback_model()?;
    let mut random = CrtRand::new();
    let mut playback = M2Playback::default_sequence(&model, &catalog, 0, &mut random)?;
    playback.apply_model_sequence(&model, &catalog, 0, 0, 0, &mut random)?;
    let first = playback.clock(&model, 250.0, &mut random)?.clock;
    assert_eq!((first.sequence(), first.animation_time_ms()), (0, 249.0));
    playback.event_window(250.0);
    playback.apply_model_sequence(&model, &catalog, 0, 0, 250, &mut random)?;
    assert_eq!(
        playback
            .clock(&model, 251.0, &mut random)?
            .clock
            .animation_time_ms(),
        0.0
    );
    playback.apply_model_sequence(&model, &catalog, 0, 450, 251, &mut random)?;
    let seek = playback.clock(&model, 252.0, &mut random)?.clock;
    assert_eq!((seek.sequence(), seek.animation_time_ms()), (0, 450.0));
    let mut next = random;
    assert_eq!(next.next_u15(), 26_962);
    playback.apply_model_sequence(&model, &catalog, u32::MAX, 0, 252, &mut random)?;
    assert_eq!(
        playback
            .clock(&model, 253.0, &mut random)?
            .clock
            .animation_time_ms(),
        451.0
    );
    assert_eq!(random.next_u15(), 26_962);
    playback.apply_model_sequence(&model, &catalog, 6, 200, 253, &mut random)?;
    let reverse = playback.clock(&model, 254.0, &mut random)?.clock;
    assert_eq!(
        (reverse.sequence(), reverse.animation_time_ms()),
        (2, 400.0)
    );
    assert_eq!(
        playback
            .clock(&model, 900.0, &mut random)?
            .clock
            .animation_time_ms(),
        0.0
    );
    playback.apply_model_sequence(&model, &catalog, 5, 123, 900, &mut random)?;
    assert_eq!(
        playback
            .clock(&model, 901.0, &mut random)?
            .clock
            .animation_time_ms(),
        600.0
    );
    Ok(())
}

/// 0x00832260/0x00831FC0 retain overdue time and each intervening variation roll.
#[test]
fn model_variation_callbacks_preserve_long_frame_remainder_and_event_tails()
-> Result<(), Box<dyn Error>> {
    let (model, catalog) = playback_model()?;
    let mut random = CrtRand::new();
    let mut playback = M2Playback::default_sequence(&model, &catalog, 0, &mut random)?;
    playback.apply_model_sequence(&model, &catalog, 0, 0, 0, &mut random)?;
    playback.clock(&model, 250.0, &mut random)?;
    playback.event_window(250.0);
    let advance = playback.clock(&model, 2_200.0, &mut random)?;
    assert_eq!(advance.expired_variations.len(), 2);
    assert_eq!(
        (advance.clock.sequence(), advance.clock.animation_time_ms()),
        (0, 201.0)
    );
    assert_eq!(random.next_u15(), 26_962);
    // Both callbacks run at scene tick 2200. The first retains sequence 0's
    // original timer with full weight, so the second must not overwrite it.
    let pose = M2BonePose::compose(model.animations(), advance.clock)?;
    assert!((pose.transforms()[0].w_axis.x - 19.9).abs() < 0.0001);
    Ok(())
}

#[test]
fn accelerated_variations_preserve_native_remainder_blend_and_event_deadlines()
-> Result<(), Box<dyn Error>> {
    let (model, _) = playback_model()?;
    let mut playback = M2Playback::unstarted(0, 0);
    let mut random = CrtRand::new();
    playback.apply_resolved_model_sequence_variation(
        &model,
        0,
        None,
        solarity_asset::M2ModelAnimationMode::Forward,
        2.0,
        0,
        0,
        solarity_rendering::M2SequenceStartPhase::BeforeSceneUpdate,
        true,
        &mut random,
    )?;
    for (tick, expected_events) in [(50., vec![]), (51., vec![0])] {
        playback.clock(&model, tick, &mut random)?;
        assert_eq!(
            triggered_m2_event_indices(model.animations(), playback.event_window(tick)),
            expected_events
        );
    }
    let mut expected_random = random;
    let _variation = expected_random.next_u15();
    let _cycles = expected_random.next_u15();
    let advance = playback.clock(&model, 551., &mut random)?;
    assert_eq!(advance.expired_variations.len(), 1);
    assert_eq!(
        advance.expired_variations[0].clock.animation_time_ms(),
        998.
    );
    assert_eq!(advance.clock.animation_time_ms(), 26.);
    assert_eq!(
        playback
            .script_timer
            .ok_or("accelerated timer")?
            .start_time_ms(),
        538
    );
    assert_eq!(
        playback.script_timer.ok_or("accelerated timer")?.speed(),
        2.
    );
    assert_eq!(random, expected_random);
    let pose = M2BonePose::compose(model.animations(), advance.clock)?;
    assert!((pose.transforms()[0].w_axis.x - 10.).abs() < 0.0001);
    for (tick, expected_events) in [(551., vec![]), (587., vec![]), (588., vec![0])] {
        playback.clock(&model, tick, &mut random)?;
        assert_eq!(
            triggered_m2_event_indices(model.animations(), playback.event_window(tick)),
            expected_events
        );
    }
    Ok(())
}

/// The incoming duration drives the fade, and a later Lua request clears it.
#[test]
fn model_variations_blend_then_explicit_sequence_calls_replace_the_timer()
-> Result<(), Box<dyn Error>> {
    let (model, catalog) = playback_model()?;
    let mut random = CrtRand::new();
    let mut playback = M2Playback::default_sequence(&model, &catalog, 0, &mut random)?;
    playback.apply_model_sequence(&model, &catalog, 0, 0, 0, &mut random)?;
    for tick in [250.0, 1_000.0, 1_400.0] {
        playback.clock(&model, tick, &mut random)?;
        playback.event_window(tick);
    }
    let start = playback.clock(&model, 1_999.0, &mut random)?.clock;
    assert_eq!(start.sequence(), 0);
    let pose = M2BonePose::compose(model.animations(), start)?;
    assert!((pose.transforms()[0].w_axis.x - 199.9).abs() < 0.0001);
    playback.event_window(1_999.0);
    let halfway = playback.clock(&model, 2_199.0, &mut random)?.clock;
    let pose = M2BonePose::compose(model.animations(), halfway)?;
    assert!((pose.transforms()[0].w_axis.x - 69.95).abs() < 0.0001);
    playback.event_window(2_199.0);
    playback.apply_model_sequence(&model, &catalog, 0, 500, 2_199, &mut random)?;
    let explicit = playback.clock(&model, 2_200.0, &mut random)?.clock;
    assert_eq!(explicit.sequence(), 1);
    let pose = M2BonePose::compose(model.animations(), explicit)?;
    assert!((pose.transforms()[0].w_axis.x - 150.0).abs() < 0.0001);
    Ok(())
}

/// 0x00826C40 preserves a secondary only while its weight is strictly above 0.5.
#[test]
fn model_variation_blend_replaces_the_secondary_at_exactly_half_weight()
-> Result<(), Box<dyn Error>> {
    for (duration, expected_x) in [(1_998, 199.9_f32), (1_999, 99.8)] {
        let (model, catalog) = playback_model_with_blend(duration)?;
        let mut random = CrtRand::new();
        let mut playback = M2Playback::default_sequence(&model, &catalog, 0, &mut random)?;
        playback.apply_model_sequence(&model, &catalog, 0, 0, 0, &mut random)?;
        for tick in [250.0, 1_000.0] {
            playback.clock(&model, tick, &mut random)?;
            playback.event_window(tick);
        }
        let clock = playback.clock(&model, 1_999.0, &mut random)?.clock;
        let pose = M2BonePose::compose(model.animations(), clock)?;
        // The newly installed blend has full weight; a retained one still has
        // just over half, so its output includes the incoming sequence at zero.
        let expected_x = if duration == 1_999 {
            let fraction = 1_000.0_f32 / 1_999.0;
            expected_x * (3.0 - 2.0 * fraction) * fraction * fraction
        } else {
            expected_x
        };
        assert!((pose.transforms()[0].w_axis.x - expected_x).abs() < 0.0001);
    }
    Ok(())
}
