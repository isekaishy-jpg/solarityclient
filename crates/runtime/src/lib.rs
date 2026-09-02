//! Application composition, lifecycle, and top-level orchestration.

#[cfg(not(target_pointer_width = "64"))]
compile_error!("solarity-runtime supports only 64-bit application targets");

mod application;
mod configuration;
mod console;
mod event;
mod foundation;
mod input;
mod legal;
mod loading;
mod performance;
mod platform;
mod random;
mod security;
mod telemetry;
mod time;

pub use random::{BlizzardRand, CrtRand};
pub use time::RealmClock;

pub use application::{
    ApplicationError, ApplicationExitReason, ApplicationRunReport, CharacterProjectionError,
    ClientApplication, GameplaySession, GameplayUpdateError, RuntimeAuthenticatedLogin,
    RuntimeCameraError, RuntimeCameraSceneError, RuntimeCharacterSelection, RuntimeCreaturePoll,
    RuntimeGameplayCoordinator, RuntimeGameplayError, RuntimeLoginCoordinator, RuntimeLoginError,
    RuntimeLoginPoll, RuntimeLoginState, RuntimePlayerCatalogs, RuntimePlayerError,
    RuntimePlayerItemCatalogs, RuntimePlayerPoll, RuntimePlayerPresentation,
    RuntimeRemotePlayerPoll, RuntimeSoundError, RuntimeTerrainCoordinator, RuntimeTerrainError,
    RuntimeTerrainFrameError, RuntimeTerrainPoll, RuntimeWorldCoordinator, RuntimeWorldEntry,
    RuntimeWorldEnvironment, RuntimeWorldEnvironmentError, RuntimeWorldEnvironmentFrame,
    RuntimeWorldError, RuntimeWorldPoll, RuntimeWorldState, StartupReport,
    world_model_environment_emissive,
};
pub use configuration::{
    ConfigurationError, LoginConfiguration, RuntimeConfiguration, StartupProfile,
    WindowConfiguration, WindowMode,
};
pub use input::{
    InputBindingInvocation, InputBindingPhase, InputBindingRouter, InputControl, InputFrameMotion,
    PointerPosition,
};
pub use performance::FrameRateCounter;
pub use platform::{
    ButtonState, KeyCode, KeyModifiers, KeyStateEvent, MouseButton, MouseButtonEvent,
    MouseMotionEvent, MouseWheelDirection, MouseWheelEvent, PlatformError, PlatformEvent, ScanCode,
    TextEditingEvent, TextInputEvent, WindowEvent, WindowId,
};
