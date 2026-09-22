//! Full native model scan, variation RNG, and cross-slot callback mutation.

use super::{CrtRand, M2Playback, playback_model};
use crate::application::model_playback::M2BoneEvent;
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
    let queue_capacity = solarity_rendering::m2_callback_queue_capacity(
        model.animations(),
        model.animations().bones().len(),
    )?;
    let budget = solarity_cpu::CpuStorageBudget::new(solarity_cpu::CpuStoragePlan::new(
        model.animations().bones().len() * size_of::<(u16, solarity_rendering::M2AnimationClock)>()
            + queue_capacity * size_of::<solarity_rendering::M2QueuedCallback>(),
        0,
        0,
    ));
    let mut scratch = crate::application::model_playback::M2CallbackScratch::default();
    scratch.prepare(&budget, model.animations().bones().len(), queue_capacity)?;
    let address = scratch.clocks.values().as_ptr();
    let queue_address = scratch.queue.as_ptr();
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
                         _: &M2BoneEvent<'_>,
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
            Some(crate::application::model_playback::M2CallbackStorage {
                budget: &budget,
                scratch: &mut scratch,
                queue_capacity,
            }),
        )?;
        assert_eq!(scratch.clocks.values().as_ptr(), address);
        assert_eq!(scratch.queue.as_ptr(), queue_address);
        assert!(scratch.queue.is_empty());
        assert_eq!(playback.callback_queue.capacity(), 0);
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

#[test]
fn admitted_event_snapshot_preserves_refused_timers_and_matches_owned_dispatch()
-> Result<(), Box<dyn Error>> {
    use crate::application::model_playback::M2CallbackScratch;
    use solarity_cpu::{CpuStorageBudget, CpuStorageClass as Class, CpuStoragePlan};
    use solarity_rendering::M2AnimationClock;
    let (model, catalog) = playback_model()?;
    let mut random = CrtRand::new();
    let mut playback = M2Playback::unstarted(0, 0);
    playback.apply_model_sequence(&model, &catalog, 0, 0, 0, &mut random)?;
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
    let queue_capacity = solarity_rendering::m2_callback_queue_capacity(
        model.animations(),
        model.animations().bones().len(),
    )?;
    let bytes = size_of::<(u16, M2AnimationClock)>()
        + queue_capacity * size_of::<solarity_rendering::M2QueuedCallback>();
    let refused = CpuStorageBudget::new(CpuStoragePlan::new(bytes - 1, 0, 0));
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(bytes, 0, 0));
    let mut scratch = M2CallbackScratch::default();
    let original_clock = playback.sample_clock(0);
    let original_cursor = playback.previous_event_scene_time_ms;
    let original_random = random;
    let mut called = false;
    let mut complete = |_: &mut M2Playback, _: i32, _: u16, _: u32, _: &mut CrtRand| {
        called = true;
        Ok(())
    };
    assert!(matches!(
        playback.clock_with_bone_callbacks(
            &model,
            2_000,
            &mut random,
            Some(&mut complete),
            None,
            Some(crate::application::model_playback::M2CallbackStorage {
                budget: &refused,
                scratch: &mut scratch,
                queue_capacity,
            })
        ),
        Err(RuntimeTerrainFrameError::Cpu(
            solarity_cpu::CpuError::StorageAtCapacity { .. }
        ))
    ));
    assert!(!called);
    assert_eq!(playback.scene_time_ms, 0);
    assert_eq!(playback.sample_clock(0), original_clock);
    assert_eq!(playback.previous_event_scene_time_ms, original_cursor);
    assert_eq!(random, original_random);
    assert!(scratch.clocks.values().is_empty());
    assert_eq!(scratch.queue.capacity(), 0);
    assert_eq!(refused.snapshot().used(Class::Frame), 0);

    scratch.prepare(&budget, 1, queue_capacity)?;
    let address = scratch.clocks.values().as_ptr();
    let queue_address = scratch.queue.as_ptr();
    let mut reference = playback.clone();
    let mut reference_random = random;
    let expected = reference.clock_with_bone_callbacks(
        &model,
        2_000,
        &mut reference_random,
        None,
        None,
        None,
    )?;
    assert!(!expected.expired_variations.is_empty());
    let mut received = Vec::new();
    let mut event =
        |_: &mut M2Playback, _: usize, _: u32, event: &M2BoneEvent<'_>, _: &mut CrtRand| {
            assert_eq!(event.bone_sequences.as_ptr(), address);
            received.push((
                event.clock,
                event.event_window,
                event.bone_sequences.to_vec(),
            ));
            Ok(())
        };
    let advance = playback.clock_with_bone_callbacks(
        &model,
        2_000,
        &mut random,
        None,
        Some(&mut event),
        Some(crate::application::model_playback::M2CallbackStorage {
            budget: &budget,
            scratch: &mut scratch,
            queue_capacity,
        }),
    )?;
    assert!(advance.expired_variations.is_empty());
    assert_eq!(advance.clock, expected.clock);
    assert_eq!(random, reference_random);
    assert_eq!(received.len(), expected.expired_variations.len());
    for (actual, expected) in received.iter().zip(&expected.expired_variations) {
        assert_eq!(actual.0, expected.clock);
        assert_eq!(actual.1, expected.event_window);
        assert_eq!(actual.2, expected.bone_sequences);
    }
    assert_eq!(scratch.clocks.values().as_ptr(), address);
    assert_eq!(scratch.queue.as_ptr(), queue_address);
    assert!(scratch.queue.is_empty());
    assert_eq!(playback.callback_queue.capacity(), 0);
    assert_eq!(budget.snapshot().used(Class::Frame), bytes);
    // Independently retained events survive reuse of the immediate callback bank.
    scratch.clocks.capture(
        &budget,
        [(4, M2AnimationClock::new(0, 10., 10.))].into_iter(),
    )?;
    for (actual, expected) in received.iter().zip(&expected.expired_variations) {
        assert_eq!(actual.2, expected.bone_sequences);
    }
    drop(scratch);
    assert_eq!(budget.snapshot().used(Class::Frame), 0);
    Ok(())
}
