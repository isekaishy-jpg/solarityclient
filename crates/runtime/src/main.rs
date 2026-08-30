//! Solarity process entry point.

use std::env;
use std::process::ExitCode;

use solarity_runtime::{ClientApplication, RuntimeConfiguration};
use tracing::{error, info};

/// Parses configuration, starts the implemented foundation, and drains it.
fn main() -> ExitCode {
    let _subscriber_result = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let configuration = match RuntimeConfiguration::from_arguments(env::args_os().skip(1)) {
        Ok(configuration) => configuration,
        Err(failure) => {
            error!(error = %failure, usage = RuntimeConfiguration::usage(), "configuration rejected");
            return ExitCode::FAILURE;
        }
    };
    let application = match ClientApplication::start(configuration) {
        Ok(application) => application,
        Err(failure) => {
            error!(error = %failure, "client foundation startup failed");
            return ExitCode::FAILURE;
        }
    };

    let report = application.report();
    info!(
        archive_count = report.archive_count(),
        cpu_workers = report.cpu_worker_count(),
        network_workers = report.network_worker_count(),
        window_id = report.window_id(),
        logical_width = report.logical_window_extent().0,
        logical_height = report.logical_window_extent().1,
        pixel_width = report.pixel_window_extent().0,
        pixel_height = report.pixel_window_extent().1,
        "client foundation started"
    );
    if let Err(failure) = application.shutdown() {
        error!(error = %failure, "client foundation shutdown failed");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
