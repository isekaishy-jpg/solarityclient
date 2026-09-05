//! Sequence timer and seek-event contracts recovered from the pinned executable.

use std::error::Error;

use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedM2Model, Locale,
    M2ModelAnimationMode,
};
use solarity_rendering::{
    M2EventTimeWindow, M2ModelSequenceTimer, M2SequenceStartPhase, triggered_m2_event_indices,
};

use crate::support::{Fixture, FixtureFile};

use super::{
    append_render_event_track, append_render_events, m2_array_offset, render_m2_bytes,
    render_skin_bytes,
};

/// Loads the same decoded sequence through both looping and terminal flag paths.
fn model(non_looping: bool) -> Result<DecodedM2Model, Box<dyn Error>> {
    let mut bytes = render_m2_bytes("Timer", 1)?;
    // The fixture writer does not include the terminator for this name length.
    let name_offset = bytes.len() as u32;
    bytes.extend_from_slice(b"Timer\0");
    bytes[8..12].copy_from_slice(&6_u32.to_le_bytes());
    bytes[12..16].copy_from_slice(&name_offset.to_le_bytes());
    append_render_events(&mut bytes)?;
    if non_looping {
        let events = m2_array_offset(&bytes, 0x100)?;
        append_render_event_track(&mut bytes, events + 24, &[0, 125, 750, 1_250], None)?;
    }
    let sequence = m2_array_offset(&bytes, 0x1c)?;
    bytes[sequence + 12..sequence + 16]
        .copy_from_slice(&(0x20_u32 | u32::from(non_looping)).to_le_bytes());
    let skin = render_skin_bytes()?;
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Creature\\Solarity\\Timer.m2",
            bytes: &bytes,
        },
        FixtureFile {
            path: "Creature\\Solarity\\Timer00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    Ok(DecodedM2Model::load(
        &mut store,
        &AssetPath::new("Creature\\Solarity\\Timer.m2")?,
    )?)
}

/// 0x00826B00 retains signed seeks; 0x0082F0F0 wraps or holds according to flag 1.
#[test]
fn sequence_timer_samples_native_seek_reverse_hold_and_tick_wrap() -> Result<(), Box<dyn Error>> {
    for non_looping in [false, true] {
        let model = model(non_looping)?;
        let sequence = &model.animations().sequences()[0];
        let timer = M2ModelSequenceTimer::new(
            sequence,
            M2ModelAnimationMode::Forward,
            1_000,
            250,
            0,
            M2SequenceStartPhase::BeforeSceneUpdate,
        );
        assert_eq!(timer.start_time_ms(), 751);
        assert_eq!(timer.end_time_ms(), 1_751);
        assert_eq!(timer.cycle_count(), 1);
        assert_eq!(timer.animation_time_ms(1_000), 249);
        assert_eq!(timer.animation_time_ms(1_001), 250);
        assert_eq!(
            timer.next_loop_boundary_ms(1_000, 1_900),
            if non_looping { None } else { Some(1_750) }
        );
        let restarted =
            timer.restart_variation(sequence, M2ModelAnimationMode::Forward, 1_900, 1_750, 0);
        assert_eq!(restarted.start_time_ms(), 1_750);
        assert_eq!(restarted.animation_time_ms(1_900), 150);
        assert_eq!(
            timer.animation_time_ms(1_751),
            if non_looping { 1_000 } else { 0 }
        );
        assert_eq!(
            timer.animation_time_ms(2_001),
            if non_looping { 1_000 } else { 250 }
        );
        let delayed = M2ModelSequenceTimer::new(
            sequence,
            M2ModelAnimationMode::Forward,
            1_000,
            -250,
            0,
            M2SequenceStartPhase::BeforeSceneUpdate,
        );
        assert_eq!(delayed.start_time_ms(), 1_251);
        assert_eq!(
            delayed.animation_time_ms(1_000),
            if non_looping { 0 } else { 45 }
        ); // (-251 as u32) % 1000
        let reverse = M2ModelSequenceTimer::new(
            sequence,
            M2ModelAnimationMode::Reverse,
            1_000,
            250,
            0,
            M2SequenceStartPhase::BeforeSceneUpdate,
        );
        assert_eq!(reverse.animation_time_ms(1_001), 750);
        assert_eq!(reverse.animation_time_ms(1_751), 0);
        assert_eq!(
            reverse.animation_time_ms(2_001),
            if non_looping { 0 } else { 46 }
        ); // (-250 as u32) % 1000
        for (mode, end) in [
            (M2ModelAnimationMode::HoldStart, false),
            (M2ModelAnimationMode::HoldEnd, true),
        ] {
            let held = M2ModelSequenceTimer::new(
                sequence,
                mode,
                1_000,
                500,
                0,
                M2SequenceStartPhase::BeforeSceneUpdate,
            );
            assert_eq!(held.start_time_ms(), 1_001);
            assert_eq!(held.end_time_ms(), 1_001);
            assert_eq!(
                held.animation_time_ms(9_000),
                if non_looping && end { 1_000 } else { 0 }
            );
        }
        let wrapped = M2ModelSequenceTimer::new(
            sequence,
            M2ModelAnimationMode::Forward,
            u32::MAX - 100,
            0,
            0,
            M2SequenceStartPhase::DuringSceneUpdate,
        );
        assert_eq!(wrapped.animation_time_ms(149), 250);
    }
    Ok(())
}

/// 0x00830FB0 dispatches crossed occurrences, not the skipped animation prefix.
#[test]
fn sequence_seek_events_use_scene_interval_and_preserve_occurrence_order()
-> Result<(), Box<dyn Error>> {
    let model = model(false)?;
    let animations = model.animations();
    let sequence = &animations.sequences()[0];
    let events = |mode, offset, previous, current| {
        let timer = M2ModelSequenceTimer::new(
            sequence,
            mode,
            1_000,
            offset,
            0,
            M2SequenceStartPhase::BeforeSceneUpdate,
        );
        triggered_m2_event_indices(
            animations,
            M2EventTimeWindow::new(0, 0.0, 0.0, false, false)
                .with_scene_timer(timer, previous, current),
        )
    };
    assert!(events(M2ModelAnimationMode::Forward, 500, 1_000, 1_010).is_empty());
    assert_eq!(
        events(M2ModelAnimationMode::Forward, 125, 1_000, 1_001),
        [0]
    );
    assert!(events(M2ModelAnimationMode::Forward, 125, 1_001, 1_001).is_empty());
    assert_eq!(
        events(M2ModelAnimationMode::Forward, 0, 1_000, 1_751),
        [0, 1, 0, 0]
    );
    assert_eq!(
        events(M2ModelAnimationMode::Forward, 0, 1_750, 2_126),
        [0, 0, 1, 0]
    );
    assert_eq!(
        events(M2ModelAnimationMode::Reverse, 0, 1_000, 2_001),
        [0, 0, 1, 0]
    );
    assert!(events(M2ModelAnimationMode::HoldStart, 125, 1_000, 9_000).is_empty());
    assert!(events(M2ModelAnimationMode::HoldEnd, 125, 1_000, 9_000).is_empty());
    let terminal = self::model(true)?;
    let timer = M2ModelSequenceTimer::new(
        &terminal.animations().sequences()[0],
        M2ModelAnimationMode::Forward,
        1_000,
        0,
        0,
        M2SequenceStartPhase::BeforeSceneUpdate,
    );
    assert_eq!(
        triggered_m2_event_indices(
            terminal.animations(),
            M2EventTimeWindow::new(0, 0.0, 0.0, false, false).with_scene_timer(timer, 1_000, 3_000)
        ),
        [0, 1, 0, 0]
    );
    Ok(())
}
