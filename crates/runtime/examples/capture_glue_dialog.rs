//! Reproduces the native login status dialog through the production Vulkan path.
//! Pass `<capture directory> --connection-failure <runtime options>` with an
//! unavailable loopback login endpoint for a real network failure. A message
//! file in place of `--connection-failure` performs an injected-dialog comparison.

use std::error::Error;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::time::Duration;

use solarity_runtime::{
    ClientApplication, GlueBenchmarkAction, GlueBenchmarkScreen, GlueBenchmarkStep,
    RuntimeConfiguration,
};
use solarity_ui::{UiEventArgument, UiEventPayload};

fn main() -> Result<(), Box<dyn Error>> {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| run().map_err(|error| error.to_string()))?
        .join()
        .map_err(|_| "diagnostic thread failed")??;
    Ok(())
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let capture = PathBuf::from(args.next().ok_or("missing capture directory")?);
    let message_path = args
        .next()
        .ok_or("missing message file or --connection-failure")?;
    let live = message_path == "--connection-failure";
    let message = if live {
        String::new()
    } else {
        std::fs::read_to_string(message_path)?
    };
    let configuration = RuntimeConfiguration::from_arguments(args)?;
    if live && configuration.login().endpoint().host() != "127.0.0.1" {
        return Err("connection failure diagnostic requires an isolated loopback endpoint".into());
    }
    let mut app = ClientApplication::start(configuration)?;
    let mut steps = vec![
        GlueBenchmarkStep {
            name: "login".to_owned(),
            action: GlueBenchmarkAction::Screen(GlueBenchmarkScreen::Login),
            expected_screen: GlueBenchmarkScreen::Login,
        },
        GlueBenchmarkStep {
            name: "connecting".to_owned(),
            action: GlueBenchmarkAction::Event {
                name: "OPEN_STATUS_DIALOG".to_owned(),
                payload: UiEventPayload::new([
                    UiEventArgument::String("CANCEL".to_owned()),
                    UiEventArgument::String("Connecting".to_owned()),
                ]),
            },
            expected_screen: GlueBenchmarkScreen::Login,
        },
        GlueBenchmarkStep {
            name: "failed-connection".to_owned(),
            action: GlueBenchmarkAction::Event {
                name: "OPEN_STATUS_DIALOG".to_owned(),
                payload: UiEventPayload::new([
                    UiEventArgument::String("OKAY".to_owned()),
                    UiEventArgument::String(message),
                ]),
            },
            expected_screen: GlueBenchmarkScreen::Login,
        },
    ];
    if live {
        steps.truncate(1);
        steps.push(GlueBenchmarkStep {
            name: "actual-connection-failure".to_owned(),
            action: GlueBenchmarkAction::ConnectionFailure,
            expected_screen: GlueBenchmarkScreen::Login,
        });
    }
    let result = app.benchmark_glue_steps(
        &steps,
        NonZeroUsize::new(60).ok_or("invalid frame count")?,
        Duration::from_secs(60),
        Some(&capture),
    );
    let failure = app.take_login_failure();
    let shutdown = app.shutdown();
    result?;
    if live {
        let failure = failure.ok_or("login did not publish a failure")?;
        eprintln!("observed real connection failure: {failure}");
    }
    shutdown?;
    Ok(())
}
