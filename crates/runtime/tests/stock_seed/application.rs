//! External tests for composition-root ownership and shutdown.

use std::error::Error;
use std::ffi::OsString;

use sdl3::event::{Event as SdlEvent, WindowEvent as SdlWindowEvent};
use solarity_rendering::VulkanError;
use solarity_runtime::{
    ApplicationError, ApplicationExitReason, ClientApplication, ConfigurationError, PlatformEvent,
    RuntimeConfiguration, WindowEvent,
};

use crate::support::ClientFixture;

/// The composition root mounts assets and owns both executor classes.
#[test]
fn application_starts_foundations_and_shuts_down_cleanly() -> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::new()?;
    let runtime_configuration = configuration(&fixture, 0)?;

    let mut application = ClientApplication::start(runtime_configuration)?;
    let report = application.report();

    assert_eq!(report.archive_count(), 10);
    assert_eq!(report.cpu_worker_count(), 2);
    assert_eq!(report.network_worker_count(), 1);
    assert_ne!(report.window_id(), 0);
    assert_eq!(report.logical_window_extent(), (960, 540));
    assert!(report.pixel_window_extent().0 >= 960);
    assert!(report.pixel_window_extent().1 >= 540);
    let vulkan = application.vulkan_report();
    assert!(!vulkan.device_name().is_empty());
    assert!(vulkan.api_version() >= ash::vk::API_VERSION_1_3);
    assert!(vulkan.swapchain_image_count() >= 2);
    assert_eq!(vulkan.extent(), report.pixel_window_extent());
    assert_eq!(vulkan.presented_texture_extent(), None);
    assert_eq!(vulkan.presented_ui_draw_count(), Some(1));
    assert_eq!(report.glue().resource_count(), 2);
    assert_eq!(report.glue().action_count(), 2);
    assert_eq!(report.glue().object_count(), 3);
    assert_eq!(report.glue().executed_chunk_count(), 1);
    assert_eq!(report.glue().executed_load_handler_count(), 1);

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

    // Another SDL window cannot terminate the primary client lifetime.
    sdl.event()?.push_event(SdlEvent::Window {
        timestamp: 2,
        window_id: report.window_id().saturating_add(1),
        win_event: SdlWindowEvent::CloseRequested,
    })?;
    sdl.event()?.push_event(SdlEvent::Window {
        timestamp: 3,
        window_id: report.window_id(),
        win_event: SdlWindowEvent::CloseRequested,
    })?;
    let run_report = application.run();
    assert_eq!(
        run_report.exit_reason(),
        ApplicationExitReason::PrimaryWindowCloseRequested
    );
    assert!(run_report.admitted_event_count() >= 2);

    // The process-wide route ends a subsequent loop without window identity.
    sdl.event()?.push_event(SdlEvent::Quit { timestamp: 4 })?;
    assert_eq!(
        application.run().exit_reason(),
        ApplicationExitReason::QuitRequested
    );
    drop(sdl);
    application.shutdown()?;

    // An impossible explicit index proves initialization does not scan for or
    // silently substitute another adapter, and that partial owners clean up.
    let invalid_adapter = ClientApplication::start(configuration(&fixture, usize::MAX)?);
    assert!(matches!(
        invalid_adapter,
        Err(ApplicationError::Vulkan(VulkanError::AdapterUnavailable {
            requested,
            ..
        })) if requested == usize::MAX
    ));
    Ok(())
}

/// Builds the complete runtime profile with an explicit Vulkan adapter index.
fn configuration(
    fixture: &ClientFixture,
    gpu_index: usize,
) -> Result<RuntimeConfiguration, ConfigurationError> {
    RuntimeConfiguration::from_arguments([
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
        OsString::from("--gpu-index"),
        OsString::from(gpu_index.to_string()),
    ])
}
