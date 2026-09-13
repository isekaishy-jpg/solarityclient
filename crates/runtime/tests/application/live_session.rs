//! Explicitly authorized live login; never run as part of the offline test suite.

use super::*;
use std::{
    error::Error,
    ffi::OsString,
    time::{Duration, Instant},
};

#[test]
#[ignore = "requires explicit live-account authorization and credential environment variables"]
fn authorized_live_character_session() -> Result<(), Box<dyn Error>> {
    let _guard = crate::test_support::SDL_TEST_LOCK
        .lock()
        .map_err(|_| "SDL lock poisoned")?;
    let _subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .try_init();
    let account = std::env::var("SOLARITY_DIAGNOSTIC_ACCOUNT")?;
    let password = std::env::var("SOLARITY_DIAGNOSTIC_PASSWORD")?;
    let character = std::env::var("SOLARITY_DIAGNOSTIC_CHARACTER")?;
    let mut arguments: Vec<OsString> = [
        "--locale",
        "enUS",
        "--cpu-workers",
        "4",
        "--cpu-capacity",
        "256",
        "--network-workers",
        "2",
        "--network-shutdown-ms",
        "2000",
        "--login-endpoint",
        "127.0.0.1:3724",
        "--login-timezone-minutes",
        "-240",
        "--login-client-ip",
        "127.0.0.1",
        "--window-width",
        "2560",
        "--window-height",
        "1440",
        "--window-mode",
        "fullscreen-windowed",
        "--gpu-index",
        "0",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    arguments.extend([
        OsString::from("--data-root"),
        std::env::var_os("SOLARITY_STOCK_DATA_ROOT").ok_or("data root")?,
        OsString::from("--profile-root"),
        std::env::var_os("SOLARITY_DIAGNOSTIC_PROFILE").ok_or("profile root")?,
    ]);
    let mut application =
        ClientApplication::start(RuntimeConfiguration::from_arguments(arguments)?)?;
    println!(
        "diagnostic build={} adapter={} extent={:?}",
        crate::CLIENT_BUILD,
        application.vulkan_report().device_name(),
        application.vulkan_report().extent()
    );
    application
        .services
        .diagnostic_submit_login(&account, &password)?;
    let mut password = password.into_bytes();
    password.fill(0);
    drop(password);
    let deadline = Instant::now() + Duration::from_secs(180);
    let mut realm_requested = false;
    let mut character_requested = false;
    let mut ready_since = None;
    loop {
        for _ in 0..run::MAX_PLATFORM_EVENTS_PER_FRAME {
            let Some(event) = application.services.poll_platform_event() else {
                break;
            };
            if application
                .dispatch_run_event(
                    &event.event,
                    event.timestamp_ms,
                    application.report.window_id,
                )?
                .is_some()
            {
                return Err("live diagnostic cancelled".into());
            }
        }
        application.services.service_login()?;
        application.services.present_frame()?;
        if let Some(error) = application.take_login_failure() {
            return Err(error.into());
        }
        if let Some(error) = application.take_world_failure() {
            return Err(error.into());
        }
        let ready = application.services.diagnostic_advance_login(
            &character,
            &mut realm_requested,
            &mut character_requested,
        )?;
        if ready {
            let started = *ready_since.get_or_insert_with(Instant::now);
            if started.elapsed() >= Duration::from_secs(20) {
                break;
            }
        } else {
            ready_since = None;
        }
        if Instant::now() >= deadline {
            return Err("live character entry timed out".into());
        }
    }
    if let Some(path) = std::env::var_os("SOLARITY_DIAGNOSTIC_CAPTURE") {
        application
            .services
            .diagnostic_capture_scene(std::path::Path::new(&path))?;
    }
    tracing::info!(character = %character, "live diagnostic measurement begins; normal client loop");
    application.run()?;
    application.shutdown()?;
    Ok(())
}
