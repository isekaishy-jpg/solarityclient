//! Cancellable transition from authenticated realmd state to a world session.

use thiserror::Error;
use tokio::net::TcpStream;
use tokio::runtime::Handle;
use tokio::sync::oneshot::{self, Receiver, error::TryRecvError};
use tokio::task::JoinHandle;

use solarity_network::{
    RealmEntry, TcpEndpoint, TcpTransport, TransportError, WorldAddonManifest, WorldAuthError,
    WorldAuthProgress, WorldConnection, WorldSession,
};

use crate::application::RuntimeAuthenticatedLogin;

/// Stable main-thread ownership state of the selected world connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeWorldState {
    /// No world transport or task is owned.
    Idle,
    /// One selected realm is connecting or authenticating.
    Connecting,
    /// The world accepted the encrypted account session.
    Authenticated,
}

/// Result of polling one asynchronous world transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeWorldPoll {
    /// No task or world session is owned.
    Idle,
    /// The selected realm has not completed authentication.
    Pending,
    /// The world session became authenticated at this poll boundary.
    Authenticated,
}

/// A failure to start or complete selected-realm authentication.
#[derive(Debug, Error)]
pub enum RuntimeWorldError {
    /// A second realm was selected while the first task was active.
    #[error("a world authentication exchange is already active")]
    AlreadyActive,
    /// A realm was selected while a world session was already retained.
    #[error("an authenticated world connection is already active")]
    AlreadyAuthenticated,
    /// The selected realm address is malformed or cannot be connected.
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// The build-12340 world authentication exchange failed.
    #[error(transparent)]
    Authentication(#[from] WorldAuthError),
    /// The task ended without publishing its owned result.
    #[error("world authentication task ended without publishing a result")]
    TaskEnded,
}

/// Main-thread owner of at most one selected world transition.
pub struct RuntimeWorldCoordinator {
    active: Option<ActiveWorld>,
    authenticated: Option<WorldSession<TcpStream>>,
}

impl RuntimeWorldCoordinator {
    /// Creates an idle world boundary without opening a socket.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            active: None,
            authenticated: None,
        }
    }

    /// Returns current synchronous world ownership state.
    #[must_use]
    pub const fn state(&self) -> RuntimeWorldState {
        if self.authenticated.is_some() {
            RuntimeWorldState::Authenticated
        } else if self.active.is_some() {
            RuntimeWorldState::Connecting
        } else {
            RuntimeWorldState::Idle
        }
    }

    /// Starts one selected-realm TCP and world-authentication exchange.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeWorldError`] when another task/session still owns the
    /// phase. Endpoint validation and connection failures are published by
    /// [`Self::poll`] because the server address travels with the owned task.
    pub fn begin(
        &mut self,
        runtime: &Handle,
        authenticated: RuntimeAuthenticatedLogin,
        realm: RealmEntry,
        addons: WorldAddonManifest,
    ) -> Result<(), RuntimeWorldError> {
        if self.active.is_some() {
            return Err(RuntimeWorldError::AlreadyActive);
        }
        if self.authenticated.is_some() {
            return Err(RuntimeWorldError::AlreadyAuthenticated);
        }
        let (sender, receiver) = oneshot::channel();
        let task = runtime.spawn(async move {
            let result = authenticate_world(authenticated, realm, addons).await;
            let _send_result = sender.send(result);
        });
        self.active = Some(ActiveWorld { receiver, task });
        Ok(())
    }

    /// Polls the selected world transition without blocking the main thread.
    ///
    /// # Errors
    ///
    /// Returns a transport/authentication failure or a stable task-boundary
    /// error when the worker disappears without its owned result.
    pub fn poll(&mut self) -> Result<RuntimeWorldPoll, RuntimeWorldError> {
        let Some(active) = self.active.as_mut() else {
            return Ok(if self.authenticated.is_some() {
                RuntimeWorldPoll::Authenticated
            } else {
                RuntimeWorldPoll::Idle
            });
        };
        let result = match active.receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return Ok(RuntimeWorldPoll::Pending),
            Err(TryRecvError::Closed) => Err(RuntimeWorldError::TaskEnded),
        };
        self.active = None;
        match result {
            Ok(session) => {
                self.authenticated = Some(session);
                Ok(RuntimeWorldPoll::Authenticated)
            }
            Err(error) => Err(error),
        }
    }

    /// Closes the active task or authenticated world session.
    pub fn disconnect(&mut self) {
        if let Some(active) = self.active.take() {
            active.task.abort();
        }
        self.authenticated = None;
    }

    /// Returns the retained encrypted world session.
    #[must_use]
    pub const fn authenticated(&self) -> Option<&WorldSession<TcpStream>> {
        self.authenticated.as_ref()
    }
}

impl Default for RuntimeWorldCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for RuntimeWorldCoordinator {
    fn drop(&mut self) {
        self.disconnect();
    }
}

struct ActiveWorld {
    receiver: Receiver<Result<WorldSession<TcpStream>, RuntimeWorldError>>,
    task: JoinHandle<()>,
}

async fn authenticate_world(
    authenticated: RuntimeAuthenticatedLogin,
    realm: RealmEntry,
    addons: WorldAddonManifest,
) -> Result<WorldSession<TcpStream>, RuntimeWorldError> {
    let endpoint = TcpEndpoint::parse(realm.address())?;
    let stream = TcpTransport::connect(&endpoint).await?;
    let (login, _realms) = authenticated.into_parts();
    let identity = login.into_world_identity();
    let mut progress = WorldConnection::authenticate(stream, identity, &realm, addons).await?;
    loop {
        progress = match progress {
            WorldAuthProgress::Authenticated(session) => return Ok(session),
            WorldAuthProgress::Queued(queue) => queue.advance().await?,
        };
    }
}
