//! Cancellable ownership of the asynchronous Grunt login phase.

use thiserror::Error;
use tokio::net::TcpStream;
use tokio::runtime::Handle;
use tokio::sync::oneshot::{self, Receiver, error::TryRecvError};
use tokio::task::JoinHandle;

use solarity_network::{
    AuthenticatedGrunt, Build12340WindowsIntegrity, GruntCredentials, GruntLogin, LoginError,
    RealmDirectory, TcpTransport, TransportError,
};

use crate::configuration::LoginConfiguration;

/// Stable main-thread state of the login-server coordinator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeLoginState {
    /// No login connection or task is owned.
    Idle,
    /// One cancellable transport/authentication/realm-list task is active.
    Authenticating,
    /// The realmd stream, SRP identity, and realm directory are retained.
    Authenticated,
}

/// Result of polling the coordinator at one main-thread service boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeLoginPoll {
    /// No asynchronous login task is active.
    Idle,
    /// The active exchange has not produced a result yet.
    Pending,
    /// Authentication and the first realm directory completed.
    Authenticated,
}

/// A failure to start or complete the runtime-owned login exchange.
#[derive(Debug, Error)]
pub enum RuntimeLoginError {
    /// A second login was requested before cancelling the first.
    #[error("a login exchange is already active")]
    AlreadyActive,
    /// A new login was requested while an authenticated stream was retained.
    #[error("an authenticated login connection is already active")]
    AlreadyAuthenticated,
    /// Glue supplied password bytes that are not valid UTF-8.
    #[error("login password is not valid UTF-8")]
    PasswordEncoding,
    /// The OS transport could not connect or configure its socket.
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// The build-12340 authentication or realm-list exchange failed.
    #[error(transparent)]
    Login(#[from] LoginError),
    /// The task ended without publishing its owned result.
    #[error("login task ended without publishing a result")]
    TaskEnded,
}

/// Authenticated realmd ownership retained until realm selection or disconnect.
pub struct RuntimeAuthenticatedLogin {
    login: AuthenticatedGrunt<TcpStream>,
    realms: RealmDirectory,
}

impl RuntimeAuthenticatedLogin {
    /// Returns the normalized account name proven to realmd.
    #[must_use]
    pub fn account_name(&self) -> &str {
        self.login.account_name()
    }

    /// Returns the first realm directory in exact server order.
    #[must_use]
    pub const fn realms(&self) -> &RealmDirectory {
        &self.realms
    }

    /// Transfers authenticated identity and realm rows to realm selection.
    #[must_use]
    pub fn into_parts(self) -> (AuthenticatedGrunt<TcpStream>, RealmDirectory) {
        (self.login, self.realms)
    }
}

/// Main-thread owner of at most one asynchronous login attempt.
pub struct RuntimeLoginCoordinator {
    configuration: LoginConfiguration,
    active: Option<ActiveLogin>,
    authenticated: Option<RuntimeAuthenticatedLogin>,
}

impl RuntimeLoginCoordinator {
    /// Creates an idle coordinator without opening a socket.
    #[must_use]
    pub const fn new(configuration: LoginConfiguration) -> Self {
        Self {
            configuration,
            active: None,
            authenticated: None,
        }
    }

    /// Returns current synchronous ownership state.
    #[must_use]
    pub const fn state(&self) -> RuntimeLoginState {
        if self.authenticated.is_some() {
            RuntimeLoginState::Authenticated
        } else if self.active.is_some() {
            RuntimeLoginState::Authenticating
        } else {
            RuntimeLoginState::Idle
        }
    }

    /// Starts one transport, SRP, and realm-directory exchange.
    ///
    /// Credential normalization occurs before task admission. The temporary
    /// caller-owned password may therefore be wiped as soon as this returns.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeLoginError`] for invalid stock credentials or when an
    /// earlier active/authenticated exchange still owns the login boundary.
    pub fn begin(
        &mut self,
        runtime: &Handle,
        account_name: &str,
        password: &str,
    ) -> Result<(), RuntimeLoginError> {
        if self.active.is_some() {
            return Err(RuntimeLoginError::AlreadyActive);
        }
        if self.authenticated.is_some() {
            return Err(RuntimeLoginError::AlreadyAuthenticated);
        }
        let credentials = GruntCredentials::new(account_name, password)?;
        let configuration = self.configuration.clone();
        let (sender, receiver) = oneshot::channel();
        let task = runtime.spawn(async move {
            let result = authenticate(configuration, credentials).await;
            // A dropped receiver means the main thread cancelled or shut down;
            // the owned result is then dropped on this network worker.
            let _send_result = sender.send(result);
        });
        self.active = Some(ActiveLogin { receiver, task });
        Ok(())
    }

    /// Polls without blocking the main thread and retains successful ownership.
    ///
    /// # Errors
    ///
    /// Returns the transport/login failure published by the worker, or a stable
    /// task-boundary error if the worker vanished without publishing a result.
    pub fn poll(&mut self) -> Result<RuntimeLoginPoll, RuntimeLoginError> {
        let Some(active) = self.active.as_mut() else {
            return Ok(match self.state() {
                RuntimeLoginState::Idle => RuntimeLoginPoll::Idle,
                RuntimeLoginState::Authenticated => RuntimeLoginPoll::Authenticated,
                RuntimeLoginState::Authenticating => unreachable!("active state was inspected"),
            });
        };
        let result = match active.receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return Ok(RuntimeLoginPoll::Pending),
            Err(TryRecvError::Closed) => Err(RuntimeLoginError::TaskEnded),
        };
        self.active = None;
        match result {
            Ok(authenticated) => {
                self.authenticated = Some(authenticated);
                Ok(RuntimeLoginPoll::Authenticated)
            }
            Err(error) => Err(error),
        }
    }

    /// Aborts only an in-progress attempt and returns whether one existed.
    pub fn cancel(&mut self) -> bool {
        let Some(active) = self.active.take() else {
            return false;
        };
        active.task.abort();
        true
    }

    /// Closes whichever login-phase ownership currently exists.
    pub fn disconnect(&mut self) {
        self.cancel();
        self.authenticated = None;
    }

    /// Returns authenticated state for explicit realm selection.
    #[must_use]
    pub const fn authenticated(&self) -> Option<&RuntimeAuthenticatedLogin> {
        self.authenticated.as_ref()
    }

    /// Transfers authenticated state to the next runtime phase.
    #[must_use]
    pub fn take_authenticated(&mut self) -> Option<RuntimeAuthenticatedLogin> {
        self.authenticated.take()
    }
}

impl Drop for RuntimeLoginCoordinator {
    fn drop(&mut self) {
        self.disconnect();
    }
}

struct ActiveLogin {
    receiver: Receiver<Result<RuntimeAuthenticatedLogin, RuntimeLoginError>>,
    task: JoinHandle<()>,
}

async fn authenticate(
    configuration: LoginConfiguration,
    credentials: GruntCredentials,
) -> Result<RuntimeAuthenticatedLogin, RuntimeLoginError> {
    let stream = TcpTransport::connect(configuration.endpoint()).await?;
    let mut login = GruntLogin::authenticate(
        stream,
        credentials,
        configuration.options(),
        &Build12340WindowsIntegrity,
    )
    .await?;
    let realms = login.request_realms().await?;
    Ok(RuntimeAuthenticatedLogin { login, realms })
}
