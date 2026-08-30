//! External tests for composition-root ownership and shutdown.

use std::error::Error;
use std::ffi::OsString;

use sdl3::event::{Event as SdlEvent, WindowEvent as SdlWindowEvent};
use solarity_runtime::{ClientApplication, PlatformEvent, RuntimeConfiguration, WindowEvent};

use crate::support::ClientFixture;

/// The composition root mounts assets and owns both executor classes.
#[test]
fn application_starts_foundations_and_shuts_down_cleanly() -> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::new()?;
    let configuration = RuntimeConfiguration::from_arguments([
        OsString::from("--data-root"),
        fixture.data_root().into_os_string(),
        OsString::from("--locale"),
        OsString::from("enUS"),
        OsString::from("--cpu-workers"),
        OsString::from("2"),
        OsString::from("--cpu-capacity"),
        OsString::from("8"),
        OsString::from("--network-workers"),
        OsString::from("1"),
        OsString::from("--network-shutdown-ms"),
        OsString::from("250"),
        OsString::from("--window-width"),
        OsString::from("960"),
        OsString::from("--window-height"),
        OsString::from("540"),
        OsString::from("--window-mode"),
        OsString::from("windowed"),
    ])?;

    let mut application = ClientApplication::start(configuration)?;
    let report = application.report();

    assert_eq!(report.archive_count(), 10);
    assert_eq!(report.cpu_worker_count(), 2);
    assert_eq!(report.network_worker_count(), 1);
    assert_ne!(report.window_id(), 0);
    assert_eq!(report.logical_window_extent(), (960, 540));
    assert!(report.pixel_window_extent().0 >= 960);
    assert!(report.pixel_window_extent().1 >= 540);

    // Exercise SDL's real process queue so the test covers both translation
    // and the composition root's ownership of the sole event pump.
    let sdl = sdl3::init()?;
    sdl.event()?.push_event(SdlEvent::Window {
        timestamp: 1,
        window_id: report.window_id(),
        win_event: SdlWindowEvent::PixelSizeChanged(1919, 1079),
    })?;
    let translated = (0..32).find_map(|_| {
        let event = application.poll_platform_event()?;
        matches!(
            &event,
            PlatformEvent::Window {
                event: WindowEvent::PixelSizeChanged {
                    width: 1919,
                    height: 1079
                },
                ..
            }
        )
        .then_some(event)
    });
    assert!(translated.is_some());

    application.shutdown()?;
    Ok(())
}
