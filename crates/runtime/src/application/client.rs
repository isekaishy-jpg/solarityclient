//! Process-level client lifecycle and startup report.

use thiserror::Error;

use solarity_asset::AssetError;
use solarity_cpu::CpuError;
use solarity_network::{AddonManifestError, RealmDirectory};
use solarity_rendering::{BlpTextureUploadError, VulkanError, VulkanReport, WorldCameraError};
use solarity_ui::{
    AddonCatalogError, FontError, GlueError, GlueStartupReport, UiEventError, UiRenderError,
};

use crate::application::character_directory::CharacterProjectionError;
use crate::application::cinematic_coordinator::RuntimeCinematicError;
use crate::application::client_services::ClientServices;
use crate::application::environment_coordinator::RuntimeWorldEnvironmentError;
use crate::application::gameplay_coordinator::RuntimeGameplayError;
use crate::application::login_coordinator::{RuntimeLoginError, RuntimeLoginState};
use crate::application::login_model::RuntimeGlueModelError;
use crate::application::player_coordinator::RuntimePlayerError;
use crate::application::run::{self, ApplicationRunReport};
use crate::application::sound_coordinator::RuntimeSoundError;
use crate::application::terrain_coordinator::{RuntimeCameraError, RuntimeTerrainError};
use crate::application::terrain_frame::RuntimeTerrainFrameError;
use crate::application::world_coordinator::{RuntimeWorldError, RuntimeWorldState};
use crate::configuration::ConfigurationError;
use crate::configuration::RuntimeConfiguration;
use crate::input::{InputControl, InputFrameMotion};
use crate::platform::{PlatformError, PlatformEvent};

/// A failure while constructing or stopping concrete client services.
#[derive(Debug, Error)]
pub enum ApplicationError {
    /// Persistent startup-profile state could not be loaded or consumed.
    #[error(transparent)]
    Configuration(#[from] ConfigurationError),
    /// First-run movie decode, timing, or presentation failed.
    #[error(transparent)]
    Cinematic(#[from] RuntimeCinematicError),
    /// Client archive discovery, mount, or validation failed.
    #[error(transparent)]
    Asset(#[from] AssetError),
    /// AddOn discovery or TOC metadata validation failed.
    #[error(transparent)]
    AddonCatalog(#[from] AddonCatalogError),
    /// AddOn identities could not be represented in world authentication.
    #[error(transparent)]
    AddonManifest(#[from] AddonManifestError),
    /// A world character references metadata absent from the mounted client.
    #[error(transparent)]
    CharacterProjection(#[from] CharacterProjectionError),
    /// Active-world packet I/O, decoding, or ECS projection failed.
    #[error(transparent)]
    Gameplay(#[from] RuntimeGameplayError),
    /// Active-world view or exterior-light composition failed.
    #[error(transparent)]
    WorldEnvironment(#[from] RuntimeWorldEnvironmentError),
    /// Local-player model residency or authored measurements failed.
    #[error(transparent)]
    Player(#[from] RuntimePlayerError),
    /// Active-map or player-tile terrain residency failed.
    #[error(transparent)]
    Terrain(#[from] RuntimeTerrainError),
    /// Final player camera composition against the resident scene failed.
    #[error(transparent)]
    Camera(#[from] RuntimeCameraError),
    /// The resolved camera could not form renderer projection state.
    #[error(transparent)]
    WorldCamera(#[from] WorldCameraError),
    /// A resident ADT could not enter renderer-owned GPU state.
    #[error(transparent)]
    TerrainFrame(#[from] RuntimeTerrainFrameError),
    /// A pre-world stock model could not enter the Glue compositor.
    #[error(transparent)]
    GlueModel(#[from] RuntimeGlueModelError),
    /// The private CPU executor failed to start or drain.
    #[error(transparent)]
    Cpu(#[from] CpuError),
    /// SDL could not construct the primary window or event source.
    #[error(transparent)]
    Platform(#[from] PlatformError),
    /// Vulkan 1.3 instance, device, or presentation initialization failed.
    #[error(transparent)]
    Vulkan(#[from] VulkanError),
    /// Built-in GlueXML, FrameXML, fonts, layout, or Lua startup failed.
    #[error(transparent)]
    Ui(#[from] GlueError),
    /// Live UI assets or geometry could not enter the renderer boundary.
    #[error(transparent)]
    UiRender(#[from] UiRenderError),
    /// A runtime-native archive-backed text overlay could not be prepared.
    #[error(transparent)]
    NativeText(#[from] FontError),
    /// A runtime-owned stock Glue event could not be delivered.
    #[error(transparent)]
    UiEvent(#[from] UiEventError),
    /// A selected UI BLP could not decode or enter device-local storage.
    #[error(transparent)]
    BlpTextureUpload(#[from] BlpTextureUploadError),
    /// Device output, sound policy, or advanced-emitter service failed.
    #[error(transparent)]
    Sound(#[from] RuntimeSoundError),
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
    addon_count: usize,
    cpu_worker_count: usize,
    network_worker_count: usize,
    window_id: u32,
    logical_window_extent: (u32, u32),
    pixel_window_extent: (u32, u32),
    sound_sample_rate_hz: u32,
    sound_output_channels: u8,
    sound_software_channel_count: usize,
    sound_virtual_voice_capacity: usize,
    glue: GlueStartupReport,
}

impl StartupReport {
    /// Returns the number of mounted stock and patch archives.
    #[must_use]
    pub const fn archive_count(self) -> usize {
        self.archive_count
    }

    /// Returns the number of AddOn identities available to UI and auth.
    #[must_use]
    pub const fn addon_count(self) -> usize {
        self.addon_count
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

    /// Returns the actual sample rate selected by SDL's default device.
    #[must_use]
    pub const fn sound_sample_rate_hz(self) -> u32 {
        self.sound_sample_rate_hz
    }

    /// Returns the actual speaker-channel count selected by SDL.
    #[must_use]
    pub const fn sound_output_channels(self) -> u8 {
        self.sound_output_channels
    }

    /// Returns the real software-mix count from startup `Sound_NumChannels`.
    #[must_use]
    pub const fn sound_software_channel_count(self) -> usize {
        self.sound_software_channel_count
    }

    /// Returns build 12340's hard FMOD virtual-voice capacity.
    #[must_use]
    pub const fn sound_virtual_voice_capacity(self) -> usize {
        self.sound_virtual_voice_capacity
    }

    /// Returns facts proving the stock built-in login UI executed.
    #[must_use]
    pub const fn glue(self) -> GlueStartupReport {
        self.glue
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
        let (services, archive_count, addon_count) = ClientServices::start(&configuration)?;
        let (window_id, logical_window_extent, pixel_window_extent) = services.window_facts();
        let (
            sound_sample_rate_hz,
            sound_output_channels,
            sound_software_channel_count,
            sound_virtual_voice_capacity,
        ) = services.sound_facts();
        let glue = services.glue_report();

        Ok(Self {
            services,
            report: StartupReport {
                archive_count,
                addon_count,
                cpu_worker_count,
                network_worker_count,
                window_id,
                logical_window_extent,
                pixel_window_extent,
                sound_sample_rate_hz,
                sound_output_channels,
                sound_software_channel_count,
                sound_virtual_voice_capacity,
                glue,
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

    /// Returns raw keyboard, pointer-button, modifier, and focus state.
    #[must_use]
    pub const fn input_control(&self) -> &InputControl {
        self.services.input_control()
    }

    /// Takes relative pointer and wheel movement accumulated since the last take.
    pub fn take_input_frame_motion(&mut self) -> InputFrameMotion {
        self.services.take_input_frame_motion()
    }

    /// Keeps the process alive on the main thread until stock termination.
    ///
    /// FIFO swapchain presentation paces the loop without a guessed timer. SDL
    /// events are drained in source order before each service/frame boundary.
    ///
    /// # Errors
    ///
    /// Returns [`ApplicationError`] when a paced presentation frame fails.
    pub fn run(&mut self) -> Result<ApplicationRunReport, ApplicationError> {
        let primary_window = self.report.window_id;
        let mut admitted_event_count = 0_u64;
        loop {
            while let Some(event) = self.services.poll_platform_event() {
                admitted_event_count = admitted_event_count.saturating_add(1);
                if let Some(exit_reason) = run::exit_reason(&event, primary_window) {
                    return Ok(ApplicationRunReport::new(exit_reason, admitted_event_count));
                }
                self.services.service_platform_event(&event)?;
            }
            self.services.service_login()?;
            self.services.present_frame()?;
        }
    }

    /// Returns the physical adapter, queue, and swapchain selected at startup.
    #[must_use]
    pub fn vulkan_report(&self) -> &VulkanReport {
        self.services.vulkan_report()
    }

    /// Returns synchronous ownership of the login-server phase.
    #[must_use]
    pub const fn login_state(&self) -> RuntimeLoginState {
        self.services.login_state()
    }

    /// Returns the authenticated realm directory before realm selection.
    #[must_use]
    pub fn realm_directory(&self) -> Option<&RealmDirectory> {
        Some(self.services.authenticated_login()?.realms())
    }

    /// Takes the oldest terminal login failure observed by the main thread.
    pub fn take_login_failure(&mut self) -> Option<RuntimeLoginError> {
        self.services.take_login_failure()
    }

    /// Returns synchronous ownership of the selected world-server phase.
    #[must_use]
    pub const fn world_state(&self) -> RuntimeWorldState {
        self.services.world_state()
    }

    /// Takes the oldest terminal world-authentication failure.
    pub fn take_world_failure(&mut self) -> Option<RuntimeWorldError> {
        self.services.take_world_failure()
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
