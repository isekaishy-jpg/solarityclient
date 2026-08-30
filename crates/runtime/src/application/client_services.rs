//! Concrete ownership and shutdown of initial client services.

#![allow(unsafe_code)]

use tokio::runtime::{Builder, Runtime};

use solarity_asset::{ArchiveCatalog, AssetStore};
use solarity_cpu::CpuExecutor;
use solarity_rendering::{VulkanBootstrap, VulkanRenderer, VulkanReport};

use crate::application::ApplicationError;
use crate::configuration::RuntimeConfiguration;
use crate::platform::{PlatformEvent, SdlPlatform};

/// Concrete services owned exclusively by the application composition root.
pub(crate) struct ClientServices {
    renderer: VulkanRenderer,
    platform: SdlPlatform,
    _assets: AssetStore,
    cpu: CpuExecutor,
    network: Option<Runtime>,
    network_shutdown_timeout: std::time::Duration,
}

impl ClientServices {
    /// Constructs services in dependency order after all configuration validates.
    pub(crate) fn start(
        configuration: &RuntimeConfiguration,
    ) -> Result<(Self, usize), ApplicationError> {
        let catalog =
            ArchiveCatalog::discover(configuration.data_root().clone(), configuration.locale())?;
        let archive_count = catalog.descriptors().len();
        let assets = AssetStore::mount(catalog)?;
        // SDL must be initialized by the process main thread before worker
        // construction can make lifecycle mistakes harder to diagnose.
        let platform = SdlPlatform::start(configuration.window())?;
        let instance_extensions = platform.vulkan_instance_extensions()?;
        let bootstrap = VulkanBootstrap::start(&instance_extensions)?;
        // SAFETY: The bootstrap enabled SDL's exact extension list and remains
        // live while SDL creates a surface for the owned window.
        let surface = unsafe { platform.create_vulkan_surface(bootstrap.instance_handle()) }?;
        // SAFETY: SDL created `surface` from this bootstrap's instance, and
        // ownership transfers immediately to the rendering owner.
        let renderer = unsafe {
            bootstrap.attach_surface(surface, platform.pixel_extent(), configuration.gpu_index())
        }?;
        let cpu = CpuExecutor::new(configuration.cpu_pool())?;
        let network = Builder::new_multi_thread()
            .worker_threads(configuration.network_workers().get())
            .thread_name("solarity-network")
            .enable_io()
            .enable_time()
            .build()
            .map_err(|source| ApplicationError::NetworkRuntime {
                message: source.to_string(),
            })?;

        Ok((
            Self {
                renderer,
                platform,
                _assets: assets,
                cpu,
                network: Some(network),
                network_shutdown_timeout: configuration.network_shutdown_timeout(),
            },
            archive_count,
        ))
    }

    /// Returns the concrete adapter and swapchain facts selected at startup.
    pub(crate) fn vulkan_report(&self) -> &VulkanReport {
        self.renderer.report()
    }

    /// Polls one translated main-thread platform event without allocating a batch.
    pub(crate) fn poll_platform_event(&mut self) -> Option<PlatformEvent> {
        self.platform.poll_event()
    }

    /// Returns startup facts that prove the configured window exists.
    pub(crate) fn window_facts(&self) -> (u32, (u32, u32), (u32, u32)) {
        (
            self.platform.window_id().value(),
            self.platform.logical_extent(),
            self.platform.pixel_extent(),
        )
    }

    /// Shuts down task admission before consuming the async runtime.
    pub(crate) fn shutdown(&mut self) -> Result<(), ApplicationError> {
        let renderer_result = self.renderer.shutdown().map_err(ApplicationError::from);
        let cpu_result = self.cpu.shutdown().map_err(ApplicationError::from);
        if let Some(network) = self.network.take() {
            network.shutdown_timeout(self.network_shutdown_timeout);
        }
        renderer_result?;
        cpu_result
    }
}

impl Drop for ClientServices {
    fn drop(&mut self) {
        let _shutdown_result = self.shutdown();
    }
}
