//! Opt-in complete F9, game-mixer, framebuffer, worker, and MP4 playback verification.

use std::error::Error;
use std::time::{Duration, Instant};

use sdl3::event::Event;
use sdl3::keyboard::{Keycode, Mod, Scancode};
use solarity_media::CinematicDecoder;
use solarity_runtime::{ApplicationExitReason, ClientApplication};

use super::{ClientFixture, configuration, empty_wdbc};

/// Real input and gameplay sound survive a complete user toggle, including key repeat.
#[test]
#[ignore = "requires an audio device, Vulkan, and hardware H.264 recording"]
fn f9_records_game_picture_and_sound_then_stops_before_shutdown() -> Result<(), Box<dyn Error>> {
    let _subscriber = tracing_subscriber::fmt()
        .with_env_filter("solarity_media=info,solarity_runtime=info")
        .with_test_writer()
        .try_init();
    let _guard = crate::support::SDL_TEST_LOCK
        .lock()
        .map_err(|_| "SDL lock poisoned")?;
    let fixture = ClientFixture::with_common_files(&[
        ("DBFilesClient\\GameObjectDisplayInfo.dbc", &empty_wdbc(19)),
        ("DBFilesClient\\UISoundLookups.dbc", &empty_wdbc(3)),
    ])?;
    wow_mpq::ArchiveBuilder::new()
        .listfile_option(wow_mpq::ListfileOption::Generate)
        .add_file_data(br#"<Ui><Frame name="GlueBootstrap" setAllPoints="true"><Layers>
<Layer level="BACKGROUND"><Texture file="Interface\Icons\INV_Misc_QuestionMark" setAllPoints="true"/></Layer>
</Layers></Frame></Ui>"#.to_vec(), "Interface\\GlueXML\\Bootstrap.xml")
        .add_file_data(br#"
SetCVar("Sound_EnableSoundWhenGameIsInBG", "1")
SetCVar("Sound_EnableMusic", "1")
SetCVar("Sound_MusicVolume", "1")
PlayMusic("Sound\\Test\\Recording.wav")
local elapsed = 0
GlueBootstrap:SetScript("OnUpdate", function(self, delta)
    elapsed = elapsed + delta
    if elapsed > 20 then QuitGame() end
end)
"#.to_vec(), "Interface\\GlueXML\\After.lua")
        .add_file_data(tone_wav(), "Sound\\Test\\Recording.wav")
        .build(fixture.data_root().join("enUS/patch-enUS-3.MPQ"))?;
    let mut application = ClientApplication::start(configuration(&fixture, 0)?)?;
    let window = application.report().window_id();
    let sdl = sdl3::init()?;
    let events = sdl.event()?;
    events.push_event(key(window, false))?;
    events.push_event(key(window, true))?;
    let sender = events.event_sender();
    let directory = fixture.profile_root().join("Videos");
    let stimulus = std::thread::spawn(move || -> Result<(), String> {
        std::thread::sleep(Duration::from_secs(2));
        sender
            .push_event(key(window, false))
            .map_err(|error| error.to_string())?;
        // Wait for MP4's final random-access footer instead of assuming a fixed
        // encoder startup/finalization time on a loaded developer machine.
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut finalized = false;
        while Instant::now() < deadline {
            if let Ok(entries) = std::fs::read_dir(&directory) {
                finalized = entries.filter_map(Result::ok).any(|entry| {
                    std::fs::read(entry.path()).is_ok_and(|bytes| {
                        bytes.len() >= 12 && &bytes[bytes.len() - 12..bytes.len() - 8] == b"mfro"
                    })
                });
            }
            if finalized {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        sender
            .push_event(Event::Quit { timestamp: 0 })
            .map_err(|error| error.to_string())?;
        if !finalized {
            return Err("recording did not finalize after F9 stopped it".to_owned());
        }
        Ok(())
    });
    let result = application.run();
    stimulus.join().map_err(|_| "input producer panicked")??;
    assert_eq!(result?.exit_reason(), ApplicationExitReason::QuitRequested);
    // The second F9 must have finalized the file while the application was still alive.
    let clips =
        std::fs::read_dir(fixture.profile_root().join("Videos"))?.collect::<Result<Vec<_>, _>>()?;
    assert_eq!(clips.len(), 1);
    assert!(
        std::fs::metadata(clips[0].path())?.len() > 8192,
        "recording has no usable game video/audio payload"
    );
    let mut decoder = CinematicDecoder::open(clips[0].path())?;
    let mut count = 0;
    let mut peak = 0_i16;
    while let Some(frame) = decoder.next_video_frame()? {
        assert_eq!((frame.width(), frame.height()), (960, 540));
        count += 1;
        for audio in decoder.take_audio_frames() {
            peak = audio
                .samples()
                .iter()
                .map(|value| value.saturating_abs())
                .fold(peak, i16::max);
        }
    }
    assert!(
        (50..=75).contains(&count),
        "captured {count} frames; repeated F9 must not stop recording"
    );
    assert!(peak > 1000, "game-mix peak was {peak}");
    application.shutdown()?;
    Ok(())
}

/// SDL key events use the same path as a physical F9 press.
fn key(window_id: u32, repeat: bool) -> Event {
    Event::KeyDown {
        timestamp: 0,
        window_id,
        keycode: Some(Keycode::F9),
        scancode: Some(Scancode::F9),
        keymod: Mod::NOMOD,
        repeat,
        which: 0,
        raw: 0,
    }
}

/// A one-second mono PCM tone exercises actual archive playback and the final mixer tap.
fn tone_wav() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(16_044);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&16_036_u32.to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&8_000_u32.to_le_bytes());
    bytes.extend_from_slice(&16_000_u32.to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&16_000_u32.to_le_bytes());
    for index in 0..8_000 {
        let sample =
            ((index as f32 * 440.0 * std::f32::consts::TAU / 8_000.0).sin() * 8_000.0) as i16;
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}
