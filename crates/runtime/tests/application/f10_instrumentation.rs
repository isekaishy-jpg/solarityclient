//! Diagnostic hotkey routing requires no SDL window or desktop input.

use std::io;

use solarity_profiling::Capture;

use super::RuntimeInstrumentation;
use crate::platform::{
    ButtonState, KeyModifiers, KeyStateEvent, PlatformEvent, ScanCode, WindowId,
};

#[test]
fn f10_consumes_repeat_and_release_without_retoggling() -> io::Result<()> {
    let root = std::env::temp_dir().join(format!("solarity-f10-test-{}", std::process::id()));
    let mut profiler = RuntimeInstrumentation {
        capture: Capture::new(&root, "fixture=true".to_owned()),
    };
    let window = WindowId::new(11);
    let key = KeyStateEvent {
        window_id: window,
        state: ButtonState::Pressed,
        key_code: None,
        scan_code: Some(ScanCode::new(sdl3::keyboard::Scancode::F10 as i32)),
        modifiers: KeyModifiers::from_bits(0),
        is_repeat: false,
    };
    assert!(!profiler.service_event(&PlatformEvent::Key(key), WindowId::new(12)));
    assert!(!solarity_profiling::enabled());
    assert!(profiler.service_event(&PlatformEvent::Key(key), window));
    assert!(solarity_profiling::enabled());
    assert!(profiler.service_event(
        &PlatformEvent::Key(KeyStateEvent {
            is_repeat: true,
            ..key
        }),
        window
    ));
    assert!(profiler.service_event(
        &PlatformEvent::Key(KeyStateEvent {
            state: ButtonState::Released,
            ..key
        }),
        window
    ));
    assert!(solarity_profiling::enabled());
    assert!(profiler.service_event(&PlatformEvent::Key(key), window));
    assert!(!solarity_profiling::enabled());
    profiler.capture.shutdown()?;
    assert_eq!(std::fs::read_dir(root.join("Profiles"))?.count(), 5);
    std::fs::remove_dir_all(root)
}
