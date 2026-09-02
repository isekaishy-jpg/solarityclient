//! Cancellable transition from authenticated realmd state to a world session.

use thiserror::Error;
use tokio::net::TcpStream;
use tokio::runtime::Handle;
use tokio::sync::oneshot::{self, Receiver, error::TryRecvError};
use tokio::task::JoinHandle;

use solarity_network::{
    AddonPolicyError, CharacterCreation, CharacterCreationError, CharacterCreationResult,
    CharacterDirectory, CharacterDirectoryError, CharacterLoginProgress, CharacterLoginRejection,
    InWorldSession, RealmEntry, TcpEndpoint, TcpTransport, TransportError, WorldAddonManifest,
    WorldAddonPolicy, WorldAuthError, WorldAuthProgress, WorldConnection, WorldServerPacket,
    WorldSession, WorldSessionError,
};

use crate::application::RuntimeAuthenticatedLogin;

/// Stable main-thread ownership state of the selected world connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeWorldState {
    /// No world transport or task is owned.
    Idle,
    /// One selected realm is connecting or authenticating.
    Connecting,
    /// One selected character is awaiting authoritative world entry.
    EnteringWorld,
    /// The world accepted the session and supplied character-selection state.
    CharacterSelection,
    /// The selected character entered a world map.
    InWorld,
}

/// Result of polling one asynchronous world transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeWorldPoll {
    /// No task or world session is owned.
    Idle,
    /// The selected realm has not completed authentication.
    Pending,
    /// Character-selection state became available at this poll boundary.
    CharacterScreenReady,
    /// A fresh authoritative character enumeration became available.
    CharacterDirectoryReady,
    /// The world returned the terminal result for one creation request.
    CharacterCreationFinished(CharacterCreationResult),
    /// A locally canceled character operation settled without another Glue result.
    CharacterOperationCancelled,
    /// The selected character entered its authoritative initial map.
    EnteredWorld,
    /// The selected character was rejected and selection state was restored.
    CharacterRejected(CharacterLoginRejection),
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
    /// Character login was requested without character-selection ownership.
    #[error("no authenticated character directory is available")]
    NoCharacterDirectory,
    /// Character-screen requests were made without an authenticated world session.
    #[error("no authenticated character-screen session is available")]
    NoCharacterScreen,
    /// Character-creation fields or the authoritative response were malformed.
    #[error(transparent)]
    CharacterCreation(#[from] CharacterCreationError),
    /// The selected GUID is absent from the authoritative character directory.
    #[error("character directory does not contain GUID {guid}")]
    UnknownCharacter {
        /// Rejected world object GUID.
        guid: u64,
    },
    /// The selected realm address is malformed or cannot be connected.
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// The build-12340 world authentication exchange failed.
    #[error(transparent)]
    Authentication(#[from] WorldAuthError),
    /// Encrypted character-screen packet I/O failed.
    #[error(transparent)]
    Session(#[from] WorldSessionError),
    /// The character enumeration packet was malformed.
    #[error(transparent)]
    CharacterDirectory(#[from] CharacterDirectoryError),
    /// The positional world-server AddOn policy was malformed.
    #[error(transparent)]
    AddonPolicy(#[from] AddonPolicyError),
    /// The server sent too many unrelated setup packets before enumeration.
    #[error("world sent more than {maximum} setup packets before character enumeration")]
    SetupPacketLimit {
        /// Explicit upper bound for retained pre-enumeration packets.
        maximum: usize,
    },
    /// The task ended without publishing its owned result.
    #[error("world authentication task ended without publishing a result")]
    TaskEnded,
}

/// Main-thread owner of at most one selected world transition.
pub struct RuntimeWorldCoordinator {
    active: Option<ActiveWorld>,
    character_screen: Option<RuntimeCharacterScreen>,
    character_selection: Option<RuntimeCharacterSelection>,
    world_entry: Option<RuntimeWorldEntry>,
}

/// Ordered request set emitted by stock `CharacterSelect_OnShow`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RuntimeCharacterScreenRequests {
    pub(crate) ready_for_account_data_times: bool,
    pub(crate) refresh_character_directory: bool,
    pub(crate) request_realm_split_info: bool,
}

impl RuntimeCharacterScreenRequests {
    pub(crate) const fn is_empty(self) -> bool {
        !self.ready_for_account_data_times
            && !self.refresh_character_directory
            && !self.request_realm_split_info
    }

    /// Retains every request in two stock `CharacterSelect_OnShow` emissions.
    pub(crate) fn merge(&mut self, requests: Self) {
        self.ready_for_account_data_times |= requests.ready_for_account_data_times;
        self.refresh_character_directory |= requests.refresh_character_directory;
        self.request_realm_split_info |= requests.request_realm_split_info;
    }
}

struct RuntimeCharacterScreen {
    session: WorldSession<TcpStream>,
    addon_policy: Option<WorldAddonPolicy>,
    setup_packets: Vec<WorldServerPacket>,
}

/// Authenticated character-screen state retained on the main thread.
pub struct RuntimeCharacterSelection {
    session: WorldSession<TcpStream>,
    directory: CharacterDirectory,
    addon_policy: Option<WorldAddonPolicy>,
    setup_packets: Vec<WorldServerPacket>,
}

impl RuntimeCharacterSelection {
    /// Returns the encrypted world session backing character selection.
    #[must_use]
    pub const fn session(&self) -> &WorldSession<TcpStream> {
        &self.session
    }

    /// Returns the most recent authoritative character enumeration.
    #[must_use]
    pub const fn directory(&self) -> &CharacterDirectory {
        &self.directory
    }

    /// Returns positional server AddOn policy when it preceded enumeration.
    #[must_use]
    pub const fn addon_policy(&self) -> Option<&WorldAddonPolicy> {
        self.addon_policy.as_ref()
    }

    /// Returns setup packets retained for later subsystem dispatch.
    #[must_use]
    pub fn setup_packets(&self) -> &[WorldServerPacket] {
        &self.setup_packets
    }
}

/// Entered-world network ownership and retained pre-entry setup packets.
pub struct RuntimeWorldEntry {
    session: InWorldSession<TcpStream>,
    setup_packets: Vec<WorldServerPacket>,
}

impl RuntimeWorldEntry {
    /// Returns the authenticated active-world network state.
    #[must_use]
    pub const fn session(&self) -> &InWorldSession<TcpStream> {
        &self.session
    }

    /// Returns packets retained before `SMSG_LOGIN_VERIFY_WORLD`.
    #[must_use]
    pub fn setup_packets(&self) -> &[WorldServerPacket] {
        &self.setup_packets
    }

    /// Transfers network and setup-packet ownership to gameplay composition.
    #[must_use]
    pub fn into_parts(self) -> (InWorldSession<TcpStream>, Vec<WorldServerPacket>) {
        (self.session, self.setup_packets)
    }
}

impl RuntimeWorldCoordinator {
    /// Creates an idle world boundary without opening a socket.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            active: None,
            character_screen: None,
            character_selection: None,
            world_entry: None,
        }
    }

    /// Returns current synchronous world ownership state.
    #[must_use]
    pub const fn state(&self) -> RuntimeWorldState {
        if self.world_entry.is_some() {
            RuntimeWorldState::InWorld
        } else if self.character_screen.is_some() || self.character_selection.is_some() {
            RuntimeWorldState::CharacterSelection
        } else if let Some(active) = &self.active {
            active.phase.state()
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
        if self.character_screen.is_some()
            || self.character_selection.is_some()
            || self.world_entry.is_some()
        {
            return Err(RuntimeWorldError::AlreadyAuthenticated);
        }
        let (sender, receiver) = oneshot::channel();
        let task = runtime.spawn(async move {
            let result = authenticate_world(authenticated, realm, addons).await;
            let _send_result = sender.send(result);
        });
        self.active = Some(ActiveWorld {
            receiver,
            task,
            phase: ActiveWorldPhase::Connecting,
            cancelled: false,
        });
        Ok(())
    }

    /// Starts the ordered native requests emitted when character selection is shown.
    ///
    /// Session ownership moves to one worker so encrypted writes and interleaved
    /// responses cannot race. A directory refresh waits for `SMSG_CHAR_ENUM`;
    /// send-only request sets restore the prior screen state immediately.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeWorldError`] when a transition is active or no
    /// authenticated character-screen session is retained.
    pub(crate) fn request_character_screen_data(
        &mut self,
        runtime: &Handle,
        requests: RuntimeCharacterScreenRequests,
    ) -> Result<(), RuntimeWorldError> {
        if requests.is_empty() {
            return Ok(());
        }
        if self.active.is_some() {
            return Err(RuntimeWorldError::AlreadyActive);
        }
        let state = if let Some(screen) = self.character_screen.take() {
            RetainedCharacterScreen::AwaitingDirectory(screen)
        } else if let Some(selection) = self.character_selection.take() {
            RetainedCharacterScreen::Ready(selection)
        } else {
            return Err(RuntimeWorldError::NoCharacterScreen);
        };
        let (sender, receiver) = oneshot::channel();
        let task = runtime.spawn(async move {
            let result = request_character_screen_data(state, requests).await;
            let _send_result = sender.send(result);
        });
        self.active = Some(ActiveWorld {
            receiver,
            task,
            phase: ActiveWorldPhase::RefreshingCharacters,
            cancelled: false,
        });
        Ok(())
    }

    /// Starts one selected-character login and transfers session ownership to
    /// the cancellable async task until acceptance or rejection.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeWorldError`] when another transition is active, no
    /// character directory is retained, or `guid` was not enumerated.
    pub fn enter_world(&mut self, runtime: &Handle, guid: u64) -> Result<(), RuntimeWorldError> {
        if self.active.is_some() {
            return Err(RuntimeWorldError::AlreadyActive);
        }
        let selection = self
            .character_selection
            .take()
            .ok_or(RuntimeWorldError::NoCharacterDirectory)?;
        if selection.directory.by_guid(guid).is_none() {
            self.character_selection = Some(selection);
            return Err(RuntimeWorldError::UnknownCharacter { guid });
        }
        let (sender, receiver) = oneshot::channel();
        let task = runtime.spawn(async move {
            let result = login_character(selection, guid).await;
            let _send_result = sender.send(result);
        });
        self.active = Some(ActiveWorld {
            receiver,
            task,
            phase: ActiveWorldPhase::EnteringWorld,
            cancelled: false,
        });
        Ok(())
    }

    /// Starts one character-creation exchange against the retained world session.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeWorldError`] when another transition is active or no
    /// authoritative character-selection state is retained.
    pub fn create_character(
        &mut self,
        runtime: &Handle,
        request: CharacterCreation,
    ) -> Result<(), RuntimeWorldError> {
        if self.active.is_some() {
            return Err(RuntimeWorldError::AlreadyActive);
        }
        let selection = self
            .character_selection
            .take()
            .ok_or(RuntimeWorldError::NoCharacterDirectory)?;
        let (sender, receiver) = oneshot::channel();
        let task = runtime.spawn(async move {
            let result = create_character(selection, request).await;
            let _send_result = sender.send(result);
        });
        self.active = Some(ActiveWorld {
            receiver,
            task,
            phase: ActiveWorldPhase::CreatingCharacter,
            cancelled: false,
        });
        Ok(())
    }

    /// Cancels Glue ownership of an active character operation.
    ///
    /// The encrypted receive remains owned by its worker until a complete
    /// packet arrives because dropping a partially consumed frame would lose
    /// cipher and stream alignment. Stock `0x004D98D0` similarly publishes
    /// `RESPONSE_CANCELLED` locally and sends no world opcode.
    pub fn cancel_character_operation(&mut self) -> bool {
        let Some(active) = self.active.as_mut() else {
            return false;
        };
        if !matches!(
            active.phase,
            ActiveWorldPhase::CreatingCharacter | ActiveWorldPhase::EnteringWorld
        ) || active.cancelled
        {
            return false;
        }
        active.cancelled = true;
        true
    }

    /// Polls the selected world transition without blocking the main thread.
    ///
    /// # Errors
    ///
    /// Returns a transport/authentication failure or a stable task-boundary
    /// error when the worker disappears without its owned result.
    pub fn poll(&mut self) -> Result<RuntimeWorldPoll, RuntimeWorldError> {
        let Some(active) = self.active.as_mut() else {
            return Ok(if self.world_entry.is_some() {
                RuntimeWorldPoll::EnteredWorld
            } else if self.character_selection.is_some() {
                RuntimeWorldPoll::CharacterDirectoryReady
            } else if self.character_screen.is_some() {
                RuntimeWorldPoll::CharacterScreenReady
            } else {
                RuntimeWorldPoll::Idle
            });
        };
        let result = match active.receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return Ok(RuntimeWorldPoll::Pending),
            Err(TryRecvError::Closed) => Err(RuntimeWorldError::TaskEnded),
        };
        let cancelled = active.cancelled;
        self.active = None;
        match result {
            Ok(ActiveWorldResult::CharacterScreen(screen)) => {
                self.character_screen = Some(screen);
                Ok(RuntimeWorldPoll::CharacterScreenReady)
            }
            Ok(ActiveWorldResult::CharacterSelection(selection)) => {
                self.character_selection = Some(selection);
                Ok(RuntimeWorldPoll::CharacterDirectoryReady)
            }
            Ok(ActiveWorldResult::CharacterCreated { selection, result }) => {
                self.character_selection = Some(selection);
                if cancelled {
                    Ok(RuntimeWorldPoll::CharacterOperationCancelled)
                } else {
                    Ok(RuntimeWorldPoll::CharacterCreationFinished(result))
                }
            }
            Ok(ActiveWorldResult::Entered(entry)) => {
                self.world_entry = Some(entry);
                Ok(RuntimeWorldPoll::EnteredWorld)
            }
            Ok(ActiveWorldResult::Rejected {
                selection,
                rejection,
            }) => {
                self.character_selection = Some(selection);
                if cancelled {
                    Ok(RuntimeWorldPoll::CharacterOperationCancelled)
                } else {
                    Ok(RuntimeWorldPoll::CharacterRejected(rejection))
                }
            }
            Err(error) => Err(error),
        }
    }

    /// Closes the active task or authenticated world session.
    pub fn disconnect(&mut self) {
        if let Some(active) = self.active.take() {
            active.task.abort();
        }
        self.character_selection = None;
        self.character_screen = None;
        self.world_entry = None;
    }

    /// Returns the retained encrypted world session.
    #[must_use]
    pub fn authenticated(&self) -> Option<&WorldSession<TcpStream>> {
        match &self.character_selection {
            Some(selection) => Some(selection.session()),
            None => self.character_screen.as_ref().map(|screen| &screen.session),
        }
    }

    /// Returns retained character-selection state after enumeration.
    #[must_use]
    pub const fn character_selection(&self) -> Option<&RuntimeCharacterSelection> {
        self.character_selection.as_ref()
    }

    /// Transfers an accepted world entry to the gameplay owner.
    #[must_use]
    pub fn take_world_entry(&mut self) -> Option<RuntimeWorldEntry> {
        self.world_entry.take()
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
    receiver: Receiver<Result<ActiveWorldResult, RuntimeWorldError>>,
    task: JoinHandle<()>,
    phase: ActiveWorldPhase,
    /// Whether Glue discarded the operation while its packet read settled.
    cancelled: bool,
}

#[derive(Clone, Copy)]
enum ActiveWorldPhase {
    Connecting,
    RefreshingCharacters,
    CreatingCharacter,
    EnteringWorld,
}

impl ActiveWorldPhase {
    const fn state(self) -> RuntimeWorldState {
        match self {
            Self::Connecting => RuntimeWorldState::Connecting,
            Self::RefreshingCharacters => RuntimeWorldState::CharacterSelection,
            Self::CreatingCharacter => RuntimeWorldState::CharacterSelection,
            Self::EnteringWorld => RuntimeWorldState::EnteringWorld,
        }
    }
}

enum ActiveWorldResult {
    CharacterScreen(RuntimeCharacterScreen),
    CharacterSelection(RuntimeCharacterSelection),
    CharacterCreated {
        selection: RuntimeCharacterSelection,
        result: CharacterCreationResult,
    },
    Entered(RuntimeWorldEntry),
    Rejected {
        selection: RuntimeCharacterSelection,
        rejection: CharacterLoginRejection,
    },
}

async fn create_character(
    mut selection: RuntimeCharacterSelection,
    request: CharacterCreation,
) -> Result<ActiveWorldResult, RuntimeWorldError> {
    const MAX_SETUP_PACKETS: usize = 256;

    selection.session.create_character(&request).await?;
    loop {
        let packet = selection.session.receive_packet().await?;
        if let Some(result) = packet.character_creation_result()? {
            return Ok(ActiveWorldResult::CharacterCreated { selection, result });
        }
        if selection.setup_packets.len() == MAX_SETUP_PACKETS {
            return Err(RuntimeWorldError::SetupPacketLimit {
                maximum: MAX_SETUP_PACKETS,
            });
        }
        selection.setup_packets.push(packet);
    }
}

async fn authenticate_world(
    authenticated: RuntimeAuthenticatedLogin,
    realm: RealmEntry,
    addons: WorldAddonManifest,
) -> Result<ActiveWorldResult, RuntimeWorldError> {
    let endpoint = TcpEndpoint::parse(realm.address())?;
    let stream = TcpTransport::connect(&endpoint).await?;
    let (login, _realms) = authenticated.into_parts();
    let identity = login.into_world_identity();
    let mut progress = WorldConnection::authenticate(stream, identity, &realm, addons).await?;
    let session = loop {
        progress = match progress {
            WorldAuthProgress::Authenticated(session) => break session,
            WorldAuthProgress::Queued(queue) => queue.advance().await?,
        };
    };
    Ok(ActiveWorldResult::CharacterScreen(RuntimeCharacterScreen {
        session,
        addon_policy: None,
        setup_packets: Vec::new(),
    }))
}

enum RetainedCharacterScreen {
    AwaitingDirectory(RuntimeCharacterScreen),
    Ready(RuntimeCharacterSelection),
}

async fn request_character_screen_data(
    state: RetainedCharacterScreen,
    requests: RuntimeCharacterScreenRequests,
) -> Result<ActiveWorldResult, RuntimeWorldError> {
    let (mut session, directory, mut addon_policy, mut setup_packets) = match state {
        RetainedCharacterScreen::AwaitingDirectory(screen) => (
            screen.session,
            None,
            screen.addon_policy,
            screen.setup_packets,
        ),
        RetainedCharacterScreen::Ready(selection) => (
            selection.session,
            Some(selection.directory),
            selection.addon_policy,
            selection.setup_packets,
        ),
    };
    if requests.ready_for_account_data_times {
        session.ready_for_account_data_times().await?;
    }
    if requests.refresh_character_directory {
        session.request_character_directory().await?;
    }
    if requests.request_realm_split_info {
        session.request_realm_split_info().await?;
    }
    if !requests.refresh_character_directory {
        return match directory {
            Some(directory) => Ok(ActiveWorldResult::CharacterSelection(
                RuntimeCharacterSelection {
                    session,
                    directory,
                    addon_policy,
                    setup_packets,
                },
            )),
            None => Ok(ActiveWorldResult::CharacterScreen(RuntimeCharacterScreen {
                session,
                addon_policy,
                setup_packets,
            })),
        };
    }

    const MAX_SETUP_PACKETS: usize = 256;
    loop {
        let packet = session.receive_packet().await?;
        if let Some(directory) = packet.character_directory()? {
            return Ok(ActiveWorldResult::CharacterSelection(
                RuntimeCharacterSelection {
                    session,
                    directory,
                    addon_policy,
                    setup_packets,
                },
            ));
        }
        if let Some(policy) = packet.addon_policy(session.addon_manifest())? {
            addon_policy = Some(policy);
            continue;
        }
        if setup_packets.len() == MAX_SETUP_PACKETS {
            return Err(RuntimeWorldError::SetupPacketLimit {
                maximum: MAX_SETUP_PACKETS,
            });
        }
        setup_packets.push(packet);
    }
}

async fn login_character(
    selection: RuntimeCharacterSelection,
    guid: u64,
) -> Result<ActiveWorldResult, RuntimeWorldError> {
    let RuntimeCharacterSelection {
        session,
        directory,
        addon_policy,
        mut setup_packets,
    } = selection;
    let character = directory
        .by_guid(guid)
        .cloned()
        .ok_or(RuntimeWorldError::UnknownCharacter { guid })?;
    let mut login = session.login_character(&character).await?;
    const MAX_SETUP_PACKETS: usize = 256;
    loop {
        match login.advance().await? {
            CharacterLoginProgress::Awaiting {
                login: pending,
                packet,
            } => {
                if setup_packets.len() == MAX_SETUP_PACKETS {
                    return Err(RuntimeWorldError::SetupPacketLimit {
                        maximum: MAX_SETUP_PACKETS,
                    });
                }
                setup_packets.push(packet);
                login = pending;
            }
            CharacterLoginProgress::Entered(session) => {
                return Ok(ActiveWorldResult::Entered(RuntimeWorldEntry {
                    session,
                    setup_packets,
                }));
            }
            CharacterLoginProgress::Rejected { session, rejection } => {
                return Ok(ActiveWorldResult::Rejected {
                    selection: RuntimeCharacterSelection {
                        session,
                        directory,
                        addon_policy,
                        setup_packets,
                    },
                    rejection,
                });
            }
        }
    }
}
