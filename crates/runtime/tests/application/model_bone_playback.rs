//! Full native model scan, variation RNG, and cross-slot callback mutation.

use super::{CrtRand, M2Playback, playback_model};
use crate::application::model_playback::M2ExpiredVariation;
use crate::application::terrain_frame::RuntimeTerrainFrameError;

#[test]
fn model_pause_moves_body_and_upper_timers_together_without_stopping_global_tracks()
-> Result<(), Box<dyn Error>> {
    let (model, _) = playback_model()?;
    let mut playback = M2Playback::unstarted(0, 0);
    let mut random = CrtRand::new();
    playback.apply_resolved_model_sequence_variation(
        &model,
        0,
        None,
        M2ModelAnimationMode::Forward,
        1.,
        0,
        0,
        M2SequenceStartPhase::DuringSceneUpdate,
        true,
        &mut random,
    )?;
    playback.apply_bone_sequence(
        &model,
        4,
        7,
        None,
        M2ModelAnimationMode::Forward,
        1.,
        0,
        0,
        M2SequenceStartPhase::DuringSceneUpdate,
        &mut random,
    )?;
    playback.clock(&model, 200., &mut random)?;
    let expected_random = random;
    playback.set_paused(true, 250);
    for now in [500_u32, 900] {
        let advance = playback.clock(&model, now as f32, &mut random)?;
        assert_eq!(advance.clock.animation_time_ms(), 250.);
        assert_eq!(advance.clock.global_time_ms(), now as f32);
        assert!(advance.expired_variations.is_empty());
        let upper = playback.bone_sequence_clocks(&model, advance.clock, now);
        assert_eq!(upper[0].1.animation_time_ms(), 250.);
        assert_eq!(upper[0].1.global_time_ms(), now as f32);
    }
    playback.set_paused(false, 900);
    let advance = playback.clock(&model, 950., &mut random)?;
    assert_eq!(advance.clock.animation_time_ms(), 300.);
    assert_eq!(
        playback.bone_sequence_clocks(&model, advance.clock, 950)[0]
            .1
            .animation_time_ms(),
        300.
    );
    assert_eq!(random, expected_random);
    Ok(())
}
use solarity_asset::{DecodedM2Model, M2ModelAnimationMode};
use solarity_rendering::M2SequenceStartPhase;
use std::{
    cell::{Cell, RefCell},
    error::Error,
};

#[test]
fn shared_model_callbacks_variations_and_mutations_match_original_executable()
-> Result<(), Box<dyn Error>> {
    let (model, _) = playback_model()?;
    let mut cases = 0;
    for row in include_str!("../fixtures/native_model_bone_playback.txt").lines() {
        if row.is_empty() || row.starts_with('#') {
            continue;
        }
        let columns = row.split('|').collect::<Vec<_>>();
        let words = |text: &str| {
            text.split_whitespace()
                .map(|v| u32::from_str_radix(v, 16))
                .collect::<Result<Vec<_>, _>>()
        };
        let inputs = words(columns[0])?;
        let [
            body,
            upper,
            order,
            speed0,
            speed1,
            offset,
            previous,
            now,
            phase,
            action,
        ] = inputs.as_slice()
        else {
            return Err("native inputs".into());
        };
        let expected_calls = columns[1]
            .split_whitespace()
            .map(|record| {
                record
                    .split(':')
                    .map(|v| u32::from_str_radix(v, 16))
                    .collect::<Result<Vec<_>, _>>()
            })
            .collect::<Result<Vec<_>, _>>()?;
        let rolls = words(columns[2])?[0];
        let expected_timers = words(columns[3])?;
        let mut random = CrtRand::new();
        let mut playback = M2Playback::unstarted(0, 0);
        let mut requests = [(None, *body, *speed0), (Some(4), *upper, *speed1)];
        if *order != 0 {
            requests.reverse();
        }
        let phase = if *phase == 0 {
            M2SequenceStartPhase::BeforeSceneUpdate
        } else {
            M2SequenceStartPhase::DuringSceneUpdate
        };
        for (key, animation, speed) in requests {
            if let Some(key) = key {
                playback.apply_bone_sequence(
                    &model,
                    key,
                    animation as u16,
                    None,
                    M2ModelAnimationMode::Forward,
                    f32::from_bits(speed),
                    *offset as i32,
                    0,
                    phase,
                    &mut random,
                )?;
            } else {
                playback.apply_resolved_model_sequence_variation(
                    &model,
                    animation as u16,
                    None,
                    M2ModelAnimationMode::Forward,
                    f32::from_bits(speed),
                    *offset as i32,
                    0,
                    phase,
                    true,
                    &mut random,
                )?;
            }
        }
        playback.previous_event_scene_time_ms = *previous;
        let calls = RefCell::new(Vec::new());
        let acted = Cell::new(false);
        let mut complete = |playback: &mut M2Playback,
                            key: i32,
                            animation: u16,
                            boundary: u32,
                            random: &mut CrtRand| {
            calls.borrow_mut().push(vec![
                0,
                key as u32,
                u32::from(animation),
                now.wrapping_sub(boundary),
            ]);
            if !acted.get()
                && ((matches!(*action, 1 | 6) && key == -1) || (*action == 2 && key == 4))
            {
                acted.set(true);
                mutate(playback, &model, *action, random)?;
            }
            Ok(())
        };
        let mut event = |playback: &mut M2Playback,
                         index: usize,
                         boundary: u32,
                         _: &M2ExpiredVariation,
                         random: &mut CrtRand| {
            calls
                .borrow_mut()
                .push(vec![1, u32::MAX, index as u32, now.wrapping_sub(boundary)]);
            if *action == 4 {
                let _ = random.next_u15();
            }
            if !acted.get() && matches!(*action, 3 | 5) {
                acted.set(true);
                mutate(playback, &model, *action, random)?;
            }
            Ok(())
        };
        let advance = playback.clock_with_bone_callbacks(
            &model,
            *now,
            &mut random,
            Some(&mut complete),
            Some(&mut event),
        )?;
        assert!(
            advance.expired_variations.is_empty(),
            "synchronously dispatched events must not replay later"
        );
        assert_eq!(calls.into_inner(), expected_calls, "{row}");
        let mut expected_random = CrtRand::new();
        for _ in 0..rolls {
            let _ = expected_random.next_u15();
        }
        assert_eq!(random, expected_random, "{row}");
        for (slot, expected) in [&playback, playback.bone_playback(4).ok_or("upper slot")?]
            .into_iter()
            .zip(expected_timers.as_chunks::<8>().0)
        {
            if let Some(timer) = slot.script_timer {
                assert_eq!(
                    [
                        slot.sequence as u32,
                        u32::from(slot.script_finished),
                        timer.start_time_ms(),
                        timer.end_time_ms(),
                        timer.speed().to_bits(),
                        timer.cycle_count()
                    ],
                    expected[..6],
                    "{row}"
                );
            } else {
                assert_eq!(expected[0], u32::from(u16::MAX), "{row}");
            }
            assert_eq!(
                slot.script_blend
                    .map_or(u32::from(u16::MAX), |blend| blend.sequence() as u32),
                expected[6],
                "{row}"
            );
            assert_eq!(
                slot.script_blend.map_or(0, |blend| blend.end_time_ms()),
                expected[7],
                "{row}"
            );
        }
        cases += 1;
    }
    assert_eq!(cases, 288);
    Ok(())
}

fn mutate(
    playback: &mut M2Playback,
    model: &DecodedM2Model,
    action: u32,
    random: &mut CrtRand,
) -> Result<(), RuntimeTerrainFrameError> {
    let now = playback.scene_time_ms;
    if action == 5 {
        playback.clear_bone_sequence(model, 4, true, now);
    } else if action == 1 {
        playback.apply_bone_sequence(
            model,
            4,
            7,
            None,
            M2ModelAnimationMode::Forward,
            1.,
            0,
            now,
            M2SequenceStartPhase::DuringSceneUpdate,
            random,
        )?;
    } else {
        playback.apply_resolved_model_sequence_variation(
            model,
            if action == 6 { 0 } else { 7 },
            None,
            M2ModelAnimationMode::Forward,
            1.,
            0,
            now,
            M2SequenceStartPhase::DuringSceneUpdate,
            true,
            random,
        )?;
    }
    Ok(())
}
