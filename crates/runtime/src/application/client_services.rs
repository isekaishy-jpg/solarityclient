//! Concrete ownership and shutdown of initial client services.

#![allow(unsafe_code)]

use std::collections::VecDeque;

use tokio::runtime::{Builder, Runtime};

use solarity_asset::{ArchiveCatalog, AssetStore};
use solarity_cpu::CpuExecutor;
use solarity_rendering::{VulkanBootstrap, VulkanRenderer, VulkanReport};
use solarity_ui::{GlueManager, GlueStartupReport, UiGlueNetworkAction, UiGlueNetworkStatus};

use crate::application::ApplicationError;
use crate::application::login_coordinator::{
    RuntimeAuthenticatedLogin, RuntimeLoginCoordinator, RuntimeLoginError, RuntimeLoginPoll,
    RuntimeLoginState,
};
use crate::application::login_ui::LoginUiFrame;
use crate::configuration::RuntimeConfiguration;
use crate::platform::{PlatformEvent, SdlPlatform};

/// Concrete services owned exclusively by the application composition root.
pub(crate) struct ClientServices {
    renderer: VulkanRenderer,
    login_ui: LoginUiFrame,
    platform: SdlPlatform,
    glue: GlueManager,
    cpu: CpuExecutor,
    network: Option<Runtime>,
    login: RuntimeLoginCoordinator,
    login_failures: VecDeque<RuntimeLoginError>,
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
        let mut platform = SdlPlatform::start(configuration.window())?;
        let instance_extensions = platform.vulkan_instance_extensions()?;
        let bootstrap = VulkanBootstrap::start(&instance_extensions)?;
        // SAFETY: The bootstrap enabled SDL's exact extension list and remains
        // live while SDL creates a surface for the owned window.
        let surface = unsafe { platform.create_vulkan_surface(bootstrap.instance_handle()) }?;
        // SAFETY: SDL created `surface` from this bootstrap's instance, and
        // ownership transfers immediately to the rendering owner.
        let mut renderer = unsafe {
            bootstrap.attach_surface(surface, platform.pixel_extent(), configuration.gpu_index())
        }?;
        let glue = GlueManager::start(assets, platform.logical_extent(), false)?;
        let login_ui = LoginUiFrame::prepare(&mut renderer, &glue)?;
        login_ui.present(&mut renderer)?;
        platform.show()?;
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
        let login = RuntimeLoginCoordinator::new(configuration.login().clone());

        Ok((
            Self {
                renderer,
                login_ui,
                platform,
                glue,
                cpu,
                network: Some(network),
                login,
                login_failures: VecDeque::new(),
                network_shutdown_timeout: configuration.network_shutdown_timeout(),
            },
            archive_count,
        ))
    }

    /// Returns the concrete adapter and swapchain facts selected at startup.
    pub(crate) fn vulkan_report(&self) -> &VulkanReport {
        self.renderer.report()
    }

    /// Returns proof that the built-in pre-world UI finished ordered startup.
    pub(crate) fn glue_report(&self) -> GlueStartupReport {
        self.glue.report()
    }

    /// Polls one translated main-thread platform event without allocating a batch.
    pub(crate) fn poll_platform_event(&mut self) -> Option<PlatformEvent> {
        self.platform.poll_event()
    }

    /// Presents one FIFO-paced login frame after main-thread service polling.
    pub(crate) fn present_login_frame(&mut self) -> Result<(), ApplicationError> {
        self.login_ui.present(&mut self.renderer)?;
        Ok(())
    }

    /// Applies ordered Glue actions and polls one asynchronous login result.
    pub(crate) fn service_login(&mut self) {
        let Some(network) = self.network.as_ref() else {
            return;
        };
        let handle = network.handle().clone();
        while let Some(action) = self.glue.take_network_action() {
            match action {
                UiGlueNetworkAction::Login(request) => {
                    let result = std::str::from_utf8(request.password_bytes())
                        .map_err(|_source| RuntimeLoginError::PasswordEncoding)
                        .and_then(|password| {
                            self.login.begin(&handle, request.account_name(), password)
                        });
                    match result {
                        Ok(())
                        | Err(RuntimeLoginError::AlreadyActive)
                        | Err(RuntimeLoginError::AlreadyAuthenticated) => {}
                        Err(error) => self.publish_login_failure(error),
                    }
                }
                UiGlueNetworkAction::CancelLogin => {
                    self.login.cancel();
                    self.glue.set_network_status(UiGlueNetworkStatus::default());
                }
                UiGlueNetworkAction::Disconnect => {
                    self.login.disconnect();
                    self.glue.set_network_status(UiGlueNetworkStatus::default());
                }
            }
        }

        match self.login.poll() {
            Ok(RuntimeLoginPoll::Idle | RuntimeLoginPoll::Pending) => {}
            Ok(RuntimeLoginPoll::Authenticated) => self
                .glue
                .set_network_status(UiGlueNetworkStatus::new(None, true)),
            Err(error) => self.publish_login_failure(error),
        }
    }

    /// Returns synchronous login ownership for diagnostics and Glue routing.
    pub(crate) const fn login_state(&self) -> RuntimeLoginState {
        self.login.state()
    }

    /// Returns the retained authenticated result before explicit realm selection.
    pub(crate) const fn authenticated_login(&self) -> Option<&RuntimeAuthenticatedLogin> {
        self.login.authenticated()
    }

    /// Takes the oldest login failure without collapsing distinct attempts.
    pub(crate) fn take_login_failure(&mut self) -> Option<RuntimeLoginError> {
        self.login_failures.pop_front()
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
        self.login.disconnect();
        let renderer_result = self.renderer.shutdown().map_err(ApplicationError::from);
        let cpu_result = self.cpu.shutdown().map_err(ApplicationError::from);
        if let Some(network) = self.network.take() {
            network.shutdown_timeout(self.network_shutdown_timeout);
        }
        renderer_result?;
        cpu_result
    }
}

impl ClientServices {
    fn publish_login_failure(&mut self, error: RuntimeLoginError) {
        self.glue.set_network_status(UiGlueNetworkStatus::default());
        tracing::warn!(error = %error, "login exchange failed");
        self.login_failures.push_back(error);
    }
}

impl Drop for ClientServices {
    fn drop(&mut self) {
        let _shutdown_result = self.shutdown();
    }
}
