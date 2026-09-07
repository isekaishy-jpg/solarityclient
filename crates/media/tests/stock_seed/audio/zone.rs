//! Original-executable inheritance captures and observable zone music lifecycle.

use crate::support::{Fixture, FixtureFile};
use solarity_asset::{
    ArchiveCatalog, AreaSoundReferences, AssetStore, ClientDataRoot, Locale, ZoneSoundCatalog,
};
use solarity_media::{
    SoundFade, SoundFadeDirection, SoundGain, ZoneMusicCue, ZoneMusicSelection, ZoneSoundFrame,
    ZoneSoundLayer, ZoneSoundOptions, ZoneSoundState, ZoneSoundTimeOfDay,
    resolve_zone_sound_references,
};
use std::error::Error;
use std::time::Duration;

#[path = "zone/playback.rs"]
mod playback;

/// Complete native address calculation, including masked edges and float stores.
#[test]
fn zone_chunk_coordinates_match_original_executable() -> Result<(), Box<dyn Error>> {
    let mut count = 0;
    for line in include_str!("../../fixtures/zone-sound-native.txt")
        .lines()
        .filter_map(|line| line.strip_prefix("# chunk "))
    {
        let words: Vec<u32> = line
            .split_whitespace()
            .map(|word| u32::from_str_radix(word, 16))
            .collect::<Result<_, _>>()?;
        let key = solarity_media::world_chunk_sound_key(
            words[2],
            f32::from_bits(words[0]),
            f32::from_bits(words[1]),
        )
        .ok_or("chunk key")?;
        assert_eq!(
            [
                key.map_id,
                key.tile[0],
                key.tile[1],
                key.chunk[0],
                key.chunk[1]
            ],
            words[2..],
            "{line}"
        );
        count += 1;
    }
    assert_eq!(count, 180);
    Ok(())
}

/// Native ordered state matches exercise both WMO gates and the mixed priorities.
#[test]
fn zone_world_state_overrides_match_original_executable() -> Result<(), Box<dyn Error>> {
    let mut count = 0;
    for line in include_str!("../../fixtures/zone-sound-native.txt")
        .lines()
        .filter_map(|line| line.strip_prefix("# state "))
    {
        let words: Vec<u32> = line
            .split_whitespace()
            .map(str::parse)
            .collect::<Result<_, _>>()?;
        let rows: Vec<_> = words[9..41]
            .as_chunks::<8>()
            .0
            .iter()
            .map(|row| solarity_asset::WorldStateZoneSound {
                state: [row[0], row[1]],
                area_id: row[2],
                world_model_area_id: row[3],
                sounds: AreaSoundReferences {
                    intro_music_id: row[4],
                    zone_music_id: row[5],
                    ambience_id: row[6],
                    sound_provider_id: row[7],
                    underwater_sound_provider_id: 0,
                },
            })
            .collect();
        let selected = solarity_media::resolve_world_state_zone_sounds(
            &rows,
            solarity_media::ZoneSoundLocationIds {
                areas: [words[0], words[1]],
                world_model_areas: [words[2], words[3]],
                world_model_only: words[4] != 0,
            },
            |id| words[5 + (id - 100) as usize],
        );
        assert_eq!(selected.is_some(), words[41] != 0, "{line}");
        if let Some(selected) = selected {
            assert_eq!(
                [
                    selected.intro_music_id,
                    selected.zone_music_id,
                    selected.ambience_id,
                    selected.sound_provider_id
                ],
                words[42..],
                "{line}"
            );
        }
        count += 1;
    }
    assert_eq!(count, 240);
    Ok(())
}

/// Original 76CF10 includes 05:30 and excludes 21:00 from daytime.
#[test]
fn zone_sound_day_columns_match_native_time_boundaries() {
    for (minute, expected) in [
        (0, false),
        (329, false),
        (330, true),
        (331, true),
        (1259, true),
        (1260, false),
        (1261, false),
        (1439, false),
    ] {
        assert_eq!(
            ZoneSoundTimeOfDay::from_day_milliseconds(minute * 60_000) == ZoneSoundTimeOfDay::Day,
            expected
        );
    }
}

/// Both fade directions use the original arithmetic, including partial reversals.
#[test]
fn sound_fade_gain_matches_original_executable() -> Result<(), Box<dyn Error>> {
    for line in include_str!("../../fixtures/zone-sound-native.txt")
        .lines()
        .filter_map(|line| line.strip_prefix("# fade "))
    {
        let words: Vec<u32> = line
            .split_whitespace()
            .map(|word| u32::from_str_radix(word, 16))
            .collect::<Result<_, _>>()?;
        let mut fade = SoundFade::new(SoundGain::new(f32::from_bits(words[1]))?);
        fade.retarget(
            if words[0] == 0 {
                SoundFadeDirection::In
            } else {
                SoundFadeDirection::Out
            },
            Duration::from_secs_f32(f32::from_bits(words[2])),
        );
        fade.advance(Duration::from_secs_f32(f32::from_bits(words[3])));
        assert_eq!(fade.gain().to_bits(), words[4], "{line}");
    }
    let mut fade = SoundFade::new(SoundGain::new(0.0)?);
    fade.retarget(SoundFadeDirection::In, Duration::from_secs(4));
    assert!(!fade.advance(Duration::from_secs(1)));
    assert_eq!(fade.gain(), 0.25);
    fade.retarget(SoundFadeDirection::Out, Duration::from_secs(4));
    assert!(fade.advance(Duration::from_secs(1)));
    assert_eq!(fade.gain(), 0.0);
    Ok(())
}

/// All expected relations were recorded from unmodified 0x0078e9a0 instructions.
#[test]
fn zone_inheritance_matches_original_executable() -> Result<(), Box<dyn Error>> {
    for line in include_str!("../../fixtures/zone-sound-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let words: Vec<u32> = line
            .split_whitespace()
            .map(str::parse)
            .collect::<Result<_, _>>()?;
        let source = |index: usize| {
            (words[0] & (1 << index) != 0).then(|| references(&words[2 + index * 5..][..5]))
        };
        assert_eq!(
            resolve_zone_sound_references(
                source(0),
                source(1),
                source(2),
                source(3),
                words[1] != 0
            ),
            references(&words[22..]),
            "{line}"
        );
    }
    Ok(())
}

/// Builds independently numbered fields to expose accidental cross-table joins.
fn references(words: &[u32]) -> AreaSoundReferences {
    AreaSoundReferences {
        sound_provider_id: words[0],
        underwater_sound_provider_id: words[1],
        ambience_id: words[2],
        zone_music_id: words[3],
        intro_music_id: words[4],
    }
}

/// A normal track starts its delay on completion, preserving silence across zones.
#[test]
fn zone_music_waits_after_completion_and_no_delay_only_bypasses_normal_silence()
-> Result<(), Box<dyn Error>> {
    let (_fixture, catalog) = catalog()?;
    let mut state = ZoneSoundState::default();
    let mut frame = frame(100);
    state.set_location(
        ZoneSoundLayer::Area,
        Some(references(&[1, 2, 7, 11, 0])),
        &catalog,
        frame,
    );
    let cue = selected(&state, frame)?;
    assert_eq!(cue.sound_entry_id(), 101);
    let mut ranges = Vec::new();
    frame.now_ms = 1_000;
    state.finished(cue, frame, &mut |upper| {
        ranges.push(upper);
        499
    });
    assert_eq!(ranges, [4_000]);
    frame.now_ms = 2_498;
    assert_eq!(state.music(frame), ZoneMusicSelection::Delay);
    state.set_location(
        ZoneSoundLayer::Area,
        Some(references(&[1, 2, 7, 12, 0])),
        &catalog,
        frame,
    );
    assert_eq!(state.music(frame), ZoneMusicSelection::Delay);
    frame.options.music_no_delay = true;
    assert_eq!(selected(&state, frame)?.sound_entry_id(), 201);
    frame.options.music_no_delay = false;
    frame.now_ms = 2_499;
    assert_eq!(selected(&state, frame)?.sound_entry_id(), 201);
    frame.time = ZoneSoundTimeOfDay::Night;
    assert_eq!(selected(&state, frame)?.sound_entry_id(), 202);
    Ok(())
}

/// Intro priority is sticky while playing, with cooldowns shared by sound ID.
#[test]
fn zone_intro_finishes_before_another_intro_and_uses_completion_cooldown()
-> Result<(), Box<dyn Error>> {
    let (_fixture, catalog) = catalog()?;
    let mut state = ZoneSoundState::default();
    let mut frame = frame(0);
    state.set_location(
        ZoneSoundLayer::Area,
        Some(references(&[0, 0, 7, 11, 21])),
        &catalog,
        frame,
    );
    let intro = selected(&state, frame)?;
    assert!(intro.is_intro());
    assert_eq!(intro.sound_entry_id(), 501);
    frame.active = Some(intro);
    state.set_location(
        ZoneSoundLayer::WorldState,
        Some(references(&[0, 0, 0, 12, 22])),
        &catalog,
        frame,
    );
    assert_eq!(selected(&state, frame)?, intro);
    frame.active = None;
    frame.now_ms = 1_000;
    state.finished(intro, frame, &mut |_| {
        panic!("intro cooldown must not use RNG")
    });
    // A different intro row naming the same SoundEntries cue shares its cooldown.
    assert_eq!(selected(&state, frame)?.sound_entry_id(), 201);
    frame.options.music_no_delay = true;
    frame.now_ms = 180_999;
    assert_eq!(selected(&state, frame)?.sound_entry_id(), 201);
    frame.now_ms = 181_000;
    assert_eq!(selected(&state, frame)?.sound_entry_id(), 501);
    Ok(())
}

/// Authored zero suppresses lower ambience, and live options retain their domains.
#[test]
fn zone_ambience_respects_silent_rows_underwater_and_independent_options()
-> Result<(), Box<dyn Error>> {
    let (_fixture, catalog) = catalog()?;
    let mut state = ZoneSoundState::default();
    let mut frame = frame(0);
    state.set_location(
        ZoneSoundLayer::Area,
        Some(references(&[1, 2, 7, 11, 0])),
        &catalog,
        frame,
    );
    assert_eq!(state.ambience(frame.time, frame.options), Some(701));
    state.set_location(
        ZoneSoundLayer::WorldState,
        Some(references(&[3, 4, 8, 0, 0])),
        &catalog,
        frame,
    );
    assert_eq!(state.ambience(frame.time, frame.options), Some(0));
    frame.time = ZoneSoundTimeOfDay::Night;
    assert_eq!(state.ambience(frame.time, frame.options), Some(802));
    assert!(state.set_underwater(1, 99));
    assert!(!state.set_underwater(1, 99));
    assert!(state.set_underwater(3, 99));
    assert!(!state.set_underwater(3, 99));
    assert_eq!(state.sound_provider_id(), 99);
    assert_eq!(state.ambience(frame.time, frame.options), Some(4209));
    frame.options.music = false;
    assert_eq!(state.music(frame), ZoneMusicSelection::Silence);
    assert_eq!(state.ambience(frame.time, frame.options), Some(4209));
    frame.options.ambience = false;
    assert_eq!(state.ambience(frame.time, frame.options), None);
    assert!(state.set_underwater(0, 99));
    assert_eq!(state.sound_provider_id(), 3);
    Ok(())
}

/// Constructs a predictable live frame without reading a wall clock.
fn frame(now_ms: u32) -> ZoneSoundFrame {
    ZoneSoundFrame {
        time: ZoneSoundTimeOfDay::Day,
        now_ms,
        active: None,
        options: ZoneSoundOptions {
            enabled: true,
            music: true,
            ambience: true,
            music_no_delay: false,
        },
    }
}

/// Requires a playable selection while keeping the test error descriptive.
fn selected(state: &ZoneSoundState, frame: ZoneSoundFrame) -> Result<ZoneMusicCue, Box<dyn Error>> {
    match state.music(frame) {
        ZoneMusicSelection::Cue(cue) => Ok(cue),
        other => Err(format!("expected music cue, got {other:?}").into()),
    }
}

/// Mounts real MPQ fixtures containing three independent stock table namespaces.
fn catalog() -> Result<(Fixture, ZoneSoundCatalog), Box<dyn Error>> {
    let music = table(
        8,
        &[
            11, 0, 1000, 2000, 5000, 6000, 101, 102, 12, 0, 0, 0, 0, 0, 201, 202,
        ],
    );
    let intro = table(5, &[21, 0, 501, 1, 3, 22, 0, 501, 1, 4]);
    let ambience = table(3, &[7, 701, 702, 8, 0, 802]);
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\ZoneMusic.dbc",
            bytes: &music,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\ZoneIntroMusicTable.dbc",
            bytes: &intro,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\SoundAmbience.dbc",
            bytes: &ambience,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let catalog = ZoneSoundCatalog::load(&mut store)?;
    Ok((fixture, catalog))
}

/// Serializes fixed-width records with a valid empty string block.
fn table(fields: u32, words: &[u32]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for word in [words.len() as u32 / fields, fields, fields * 4, 1]
        .iter()
        .chain(words)
    {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes.push(0);
    bytes
}
