//! Process-level client lifecycle and startup report.

use thiserror::Error;

use solarity_asset::AssetError;
use solarity_cpu::CpuError;
use solarity_rendering::{VulkanError, VulkanReport};

use crate::application::client_services::ClientServices;
use crate::configuration::RuntimeConfiguration;
use crate::platform::{PlatformError, PlatformEvent};

/// A failure while constructing or stopping concrete client services.
#[derive(Debug, Error)]
pub enum ApplicationError {
    /// Client archive discovery, mount, or validation failed.
    #[error(transparent)]
    Asset(#[from] AssetError),
    /// The private CPU executor failed to start or drain.
    #[error(transparent)]
    Cpu(#[from] CpuError),
    /// SDL could not construct the primary window or event source.
    #[error(transparent)]
    Platform(#[from] PlatformError),
    /// Vulkan 1.3 instance, device, or presentation initialization failed.
    #[error(transparent)]
    Vulkan(#[from] VulkanError),
    /// Tokio could not construct the private network runtime.
    #[error("failed to create network runtime: {message}")]
    NetworkRuntime {
        /// Dependency context without exposing Tokio implementation types.
        message: String,
    },
}

/// Immutable facts proving which foundational services started.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StartupReport {
    archive_count: usize,
    cpu_worker_count: usize,
    network_worker_count: usize,
    window_id: u32,
    logical_window_extent: (u32, u32),
    pixel_window_extent: (u32, u32),
}

impl StartupReport {
    /// Returns the number of mounted stock and patch archives.
    #[must_use]
    pub const fn archive_count(self) -> usize {
        self.archive_count
    }

    /// Returns the configured CPU worker count.
    #[must_use]
    pub const fn cpu_worker_count(self) -> usize {
        self.cpu_worker_count
    }

    /// Returns the configured Tokio worker count.
    #[must_use]
    pub const fn network_worker_count(self) -> usize {
        self.network_worker_count
    }

    /// Returns the SDL identifier of the primary client window.
    #[must_use]
    pub const fn window_id(self) -> u32 {
        self.window_id
    }

    /// Returns the initial logical width and height used by UI coordinates.
    #[must_use]
    pub const fn logical_window_extent(self) -> (u32, u32) {
        self.logical_window_extent
    }

    /// Returns the initial physical drawable extent intended for the swapchain.
    #[must_use]
    pub const fn pixel_window_extent(self) -> (u32, u32) {
        self.pixel_window_extent
    }
}

/// The sole owner of cross-crate concrete service wiring.
pub struct ClientApplication {
    services: ClientServices,
    report: StartupReport,
}

impl ClientApplication {
    /// Starts the currently implemented client foundation in dependency order.
    ///
    /// # Errors
    ///
    /// Returns [`ApplicationError`] if archives, CPU workers, or network workers
    /// cannot be constructed.
    pub fn start(configuration: RuntimeConfiguration) -> Result<Self, ApplicationError> {
        let cpu_worker_count = configuration.cpu_pool().worker_count().get();
        let network_worker_count = configuration.network_workers().get();
        let (services, archive_count) = ClientServices::start(&configuration)?;
        let (window_id, logical_window_extent, pixel_window_extent) = services.window_facts();

        Ok(Self {
            services,
            report: StartupReport {
                archive_count,
                cpu_worker_count,
                network_worker_count,
                window_id,
                logical_window_extent,
                pixel_window_extent,
            },
        })
    }

    /// Returns immutable startup evidence for diagnostics and tests.
    #[must_use]
    pub const fn report(&self) -> StartupReport {
        self.report
    }

    /// Returns the next stock-relevant platform event currently queued by SDL.
    #[must_use]
    pub fn poll_platform_event(&mut self) -> Option<PlatformEvent> {
        self.services.poll_platform_event()
    }

    /// Returns the physical adapter, queue, and swapchain selected at startup.
    #[must_use]
    pub fn vulkan_report(&self) -> &VulkanReport {
        self.services.vulkan_report()
    }

    /// Drains owned executors in explicit shutdown order.
    ///
    /// # Errors
    ///
    /// Returns a CPU lifecycle error after still consuming the network runtime.
    pub fn shutdown(mut self) -> Result<(), ApplicationError> {
        self.services.shutdown()
    }
}
