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
    /// The authenticated stream is fetching a newer realm directory.
    RefreshingRealms,
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
    /// An authenticated realm-directory refresh completed.
    RealmDirectoryUpdated,
    /// A realm-directory refresh was cancelled without dropping realmd.
    RealmDirectoryCancelled,
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
    /// A realm refresh was requested without an authenticated realmd stream.
    #[error("realm-list refresh requires an authenticated login connection")]
    NotAuthenticated,
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

    /// Reports whether realmd granted this account arena-tournament access.
    #[must_use]
    pub const fn has_tournament_access(&self) -> bool {
        self.login.has_tournament_access()
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
        } else if let Some(active) = &self.active {
            match active.kind {
                ActiveLoginKind::Authentication => RuntimeLoginState::Authenticating,
                ActiveLoginKind::RealmRefresh => RuntimeLoginState::RefreshingRealms,
            }
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
            let result = authenticate(configuration, credentials)
                .await
                .map(|authenticated| ActiveLoginOutput {
                    authenticated,
                    completion: ActiveLoginCompletion::Authentication,
                });
            // A dropped receiver means the main thread cancelled or shut down;
            // the owned result is then dropped on this network worker.
            let _send_result = sender.send(result);
        });
        self.active = Some(ActiveLogin {
            kind: ActiveLoginKind::Authentication,
            cancellation: None,
            receiver,
            task,
        });
        Ok(())
    }

    /// Starts one realm-directory refresh on the authenticated realmd stream.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeLoginError::AlreadyActive`] while another task owns the
    /// stream or [`RuntimeLoginError::NotAuthenticated`] before login succeeds.
    pub fn refresh_realms(&mut self, runtime: &Handle) -> Result<(), RuntimeLoginError> {
        if self.active.is_some() {
            return Err(RuntimeLoginError::AlreadyActive);
        }
        let Some(authenticated) = self.authenticated.take() else {
            return Err(RuntimeLoginError::NotAuthenticated);
        };
        let (sender, receiver) = oneshot::channel();
        let (cancellation, cancelled) = oneshot::channel();
        let task = runtime.spawn(async move {
            let result = refresh_realm_directory(authenticated, cancelled).await;
            let _send_result = sender.send(result);
        });
        self.active = Some(ActiveLogin {
            kind: ActiveLoginKind::RealmRefresh,
            cancellation: Some(cancellation),
            receiver,
            task,
        });
        Ok(())
    }

    /// Cancels only an in-flight realm query while retaining its realmd stream.
    ///
    /// Returns `true` when cancellation was delivered to a realm refresh.
    pub fn cancel_realm_refresh(&mut self) -> bool {
        let Some(active) = self.active.as_mut() else {
            return false;
        };
        if !matches!(active.kind, ActiveLoginKind::RealmRefresh) {
            return false;
        }
        active
            .cancellation
            .take()
            .is_some_and(|cancellation| cancellation.send(()).is_ok())
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
                RuntimeLoginState::Authenticating | RuntimeLoginState::RefreshingRealms => {
                    unreachable!("active state was inspected")
                }
            });
        };
        let result = match active.receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return Ok(RuntimeLoginPoll::Pending),
            Err(TryRecvError::Closed) => Err(RuntimeLoginError::TaskEnded),
        };
        self.active = None;
        match result {
            Ok(output) => {
                self.authenticated = Some(output.authenticated);
                Ok(match output.completion {
                    ActiveLoginCompletion::Authentication => RuntimeLoginPoll::Authenticated,
                    ActiveLoginCompletion::RealmRefresh => RuntimeLoginPoll::RealmDirectoryUpdated,
                    ActiveLoginCompletion::RealmRefreshCancelled => {
                        RuntimeLoginPoll::RealmDirectoryCancelled
                    }
                })
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
    kind: ActiveLoginKind,
    cancellation: Option<oneshot::Sender<()>>,
    receiver: Receiver<Result<ActiveLoginOutput, RuntimeLoginError>>,
    task: JoinHandle<()>,
}

#[derive(Clone, Copy)]
enum ActiveLoginKind {
    Authentication,
    RealmRefresh,
}

struct ActiveLoginOutput {
    authenticated: RuntimeAuthenticatedLogin,
    completion: ActiveLoginCompletion,
}

#[derive(Clone, Copy)]
enum ActiveLoginCompletion {
    Authentication,
    RealmRefresh,
    RealmRefreshCancelled,
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

async fn refresh_realm_directory(
    mut authenticated: RuntimeAuthenticatedLogin,
    mut cancellation: oneshot::Receiver<()>,
) -> Result<ActiveLoginOutput, RuntimeLoginError> {
    let completion = tokio::select! {
        realms = authenticated.login.request_realms() => {
            authenticated.realms = realms?;
            ActiveLoginCompletion::RealmRefresh
        }
        _cancelled = &mut cancellation => ActiveLoginCompletion::RealmRefreshCancelled,
    };
    Ok(ActiveLoginOutput {
        authenticated,
        completion,
    })
}
