//! Process startup, client lifecycle, subsystem wiring, and orderly shutdown.
//!
//! `Client.cpp` and `ClientServices.cpp` provide the stock composition-root
//! evidence. This is the only module permitted to construct and connect all
//! concrete workspace subsystems.

mod client;
mod client_services;
mod gameplay_session;
mod login_coordinator;
mod login_ui;
mod run;

pub use client::{ApplicationError, ClientApplication, StartupReport};
pub use gameplay_session::{GameplaySession, GameplayUpdateError};
pub use login_coordinator::{
    RuntimeAuthenticatedLogin, RuntimeLoginCoordinator, RuntimeLoginError, RuntimeLoginPoll,
    RuntimeLoginState,
};
pub use run::{ApplicationExitReason, ApplicationRunReport};
