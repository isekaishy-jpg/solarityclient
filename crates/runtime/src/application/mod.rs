//! Process startup, client lifecycle, subsystem wiring, and orderly shutdown.
//!
//! `Client.cpp` and `ClientServices.cpp` provide the stock composition-root
//! evidence. This is the only module permitted to construct and connect all
//! concrete workspace subsystems.

mod character_directory;
mod cinematic_coordinator;
mod client;
mod client_services;
mod developer_console;
mod environment_coordinator;
mod frame_profile;
mod game_object_behavior;
mod game_object_coordinator;
mod gameplay_coordinator;
mod gameplay_session;
mod login_coordinator;
mod login_model;
mod login_ui;
mod model_playback;
mod performance_overlay;
mod player_coordinator;
mod player_movement;
mod realm_directory;
mod run;
mod sound_coordinator;
mod terrain_coordinator;
mod terrain_frame;
pub(crate) mod ui_frame;
mod world_coordinator;
mod world_transfer;
mod world_ui;

pub use character_directory::CharacterProjectionError;
pub use client::{ApplicationError, ClientApplication, StartupReport};
pub use client_services::glue_benchmark::{
    GlueBenchmarkAction, GlueBenchmarkError, GlueBenchmarkResult, GlueBenchmarkScreen,
    GlueBenchmarkStep,
};
pub use environment_coordinator::{
    RuntimeWorldEnvironment, RuntimeWorldEnvironmentError, RuntimeWorldEnvironmentFrame,
    world_model_environment_emissive,
};
pub use game_object_coordinator::{
    RuntimeGameObjectError, RuntimeGameObjectPresentation, RuntimeGameObjectResourceKind,
    RuntimeTransportPoll,
};
pub use gameplay_coordinator::{RuntimeGameplayCoordinator, RuntimeGameplayError};
pub use gameplay_session::{GameplaySession, GameplayUpdateError};
pub use login_coordinator::{
    RuntimeAuthenticatedLogin, RuntimeLoginCoordinator, RuntimeLoginError, RuntimeLoginPoll,
    RuntimeLoginState,
};
pub use player_coordinator::{
    RuntimeCreaturePoll, RuntimePlayerCatalogs, RuntimePlayerError, RuntimePlayerItemCatalogs,
    RuntimePlayerPoll, RuntimePlayerPresentation, RuntimeRemotePlayerPoll,
};
pub use player_movement::RuntimePlayerMovementError;
pub use run::{ApplicationExitReason, ApplicationRunReport};
pub use sound_coordinator::RuntimeSoundError;
pub use terrain_coordinator::{
    RuntimeCameraError, RuntimeCameraSceneError, RuntimeMovementGeometry,
    RuntimeMovementGeometryFailure, RuntimeMovementOwner, RuntimeMovementQuery,
    RuntimeMovementReference, RuntimeMovementRegistrationError, RuntimeMovementRegistrationQuery,
    RuntimeStaticMovementError, RuntimeStaticMovementOwner, RuntimeStaticMovementQuery,
    RuntimeStaticMovementResidency, RuntimeTerrainCoordinator, RuntimeTerrainError,
    RuntimeTerrainPoll, RuntimeTerrainStreamPoll, RuntimeWorldModelMovementOwner,
};
pub use terrain_frame::RuntimeTerrainFrameError;
pub use world_coordinator::{
    RuntimeCharacterSelection, RuntimeWorldCoordinator, RuntimeWorldEntry, RuntimeWorldError,
    RuntimeWorldPoll, RuntimeWorldState,
};
pub use world_transfer::{
    RuntimeWorldReplacement, RuntimeWorldTransferCoordinator, RuntimeWorldTransferEffect,
    RuntimeWorldTransferError,
};
pub use world_ui::RuntimeWorldUiError;
