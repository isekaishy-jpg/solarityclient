//! Main-thread actions and status shared by Glue Lua and runtime networking.

use std::collections::VecDeque;
use std::fmt;

use zeroize::Zeroize;

mod character;
mod realm;

pub use character::{UiCharacterDirectory, UiCharacterInfo};
pub use realm::{
    UiRealmCategory, UiRealmDirectory, UiRealmFlags, UiRealmInfo, UiRealmSort, UiRealmVersion,
};

/// One credential submission emitted by stock `DefaultServerLogin`.
pub struct UiLoginRequest {
    account_name: String,
    password: Vec<u8>,
}

impl UiLoginRequest {
    pub(super) fn new(account_name: String, password: Vec<u8>) -> Self {
        Self {
            account_name,
            password,
        }
    }

    /// Returns the account text exactly as supplied by the Glue edit box.
    #[must_use]
    pub fn account_name(&self) -> &str {
        &self.account_name
    }

    /// Returns the temporary password bytes for immediate network validation.
    #[must_use]
    pub fn password_bytes(&self) -> &[u8] {
        &self.password
    }
}

impl fmt::Debug for UiLoginRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UiLoginRequest")
            .field("account_name", &self.account_name)
            .field("password", &"<redacted>")
            .finish()
    }
}

impl Drop for UiLoginRequest {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

/// One ordered native-network action emitted by built-in GlueXML.
#[derive(Debug)]
pub enum UiGlueNetworkAction {
    /// Begin the stock login-server exchange with submitted credentials.
    Login(UiLoginRequest),
    /// Cancel the currently active login exchange.
    CancelLogin,
    /// Close an authenticated login or world-server connection.
    Disconnect,
    /// Refresh the authenticated login server's realm directory.
    RequestRealmList {
        /// Whether stock Glue requested the modal progress dialog.
        show_progress_dialog: bool,
        /// Localized `REALM_LIST_IN_PROGRESS` text captured by Glue.
        status_message: Option<String>,
    },
    /// Cancel an outstanding realm-directory refresh.
    CancelRealmListQuery,
    /// Begin the world-server handoff for one stable realm identifier.
    ChangeRealm {
        /// Identifier carried by the authenticated realm-list response.
        realm_id: u32,
    },
    /// Select a stock-preferred category and rule-set combination.
    SetPreferredRealmInfo {
        /// One-based index among categories visible to Glue.
        category_index: u32,
        /// Requested player-killing rule.
        player_killing_allowed: bool,
        /// Requested roleplaying rule.
        roleplaying: bool,
    },
    /// Apply the stock realm-list sort criteria.
    SortRealms {
        /// Stock column selected by the authored realm-list button.
        sort: UiRealmSort,
    },
    /// Report that the player closed the realm-list dialog.
    RealmListDialogCancelled {
        /// Whether the active Glue screen was the account-login screen.
        from_login_screen: bool,
    },
    /// Announce that Glue can receive account-data timestamps.
    ReadyForAccountDataTimes,
    /// Request a fresh authoritative character enumeration.
    RequestCharacterListUpdate,
    /// Request the selected realm's split-status metadata.
    RequestRealmSplitInfo,
    /// Submit the current stock character-creation selections.
    CreateCharacter(crate::UiCharacterCreationRequest),
    /// Select one character identity for character-screen presentation.
    SelectCharacter {
        /// World object GUID returned by enumeration.
        guid: u64,
    },
    /// Enter the world with the currently selected character.
    EnterWorld {
        /// World object GUID returned by enumeration.
        guid: u64,
    },
}

/// Main-thread network facts queried synchronously by Glue Lua.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UiGlueNetworkStatus {
    server_name: Option<String>,
    connected: bool,
    player_killing_allowed: bool,
    roleplaying: bool,
    server_down: bool,
}

impl UiGlueNetworkStatus {
    /// Captures the selected server label and current transport state.
    #[must_use]
    pub fn new(server_name: Option<String>, connected: bool) -> Self {
        Self {
            server_name,
            connected,
            player_killing_allowed: false,
            roleplaying: false,
            server_down: false,
        }
    }

    /// Attaches the selected realm properties returned by `GetServerName`.
    #[must_use]
    pub const fn with_realm_rules(
        mut self,
        player_killing_allowed: bool,
        roleplaying: bool,
        server_down: bool,
    ) -> Self {
        self.player_killing_allowed = player_killing_allowed;
        self.roleplaying = roleplaying;
        self.server_down = server_down;
        self
    }

    /// Returns the selected server label, if a realm has supplied one.
    #[must_use]
    pub fn server_name(&self) -> Option<&str> {
        self.server_name.as_deref()
    }

    /// Returns whether runtime owns an authenticated server connection.
    #[must_use]
    pub const fn is_connected(&self) -> bool {
        self.connected
    }

    /// Reports whether the selected realm permits player killing.
    #[must_use]
    pub const fn player_killing_allowed(&self) -> bool {
        self.player_killing_allowed
    }

    /// Reports whether the selected realm uses roleplaying rules.
    #[must_use]
    pub const fn roleplaying(&self) -> bool {
        self.roleplaying
    }

    /// Reports whether the selected realm is currently down.
    #[must_use]
    pub const fn is_server_down(&self) -> bool {
        self.server_down
    }
}

/// Ordered action mailbox plus synchronously readable connection status.
#[derive(Default)]
pub(crate) struct UiGlueNetworkBridge {
    actions: VecDeque<UiGlueNetworkAction>,
    status: UiGlueNetworkStatus,
    realms: UiRealmDirectory,
    characters: UiCharacterDirectory,
}

impl UiGlueNetworkBridge {
    pub(crate) fn push(&mut self, action: UiGlueNetworkAction) {
        self.actions.push_back(action);
    }

    pub(crate) fn take(&mut self) -> Option<UiGlueNetworkAction> {
        self.actions.pop_front()
    }

    pub(crate) const fn status(&self) -> &UiGlueNetworkStatus {
        &self.status
    }

    pub(crate) fn set_status(&mut self, status: UiGlueNetworkStatus) {
        self.status = status;
    }

    pub(crate) const fn realms(&self) -> &UiRealmDirectory {
        &self.realms
    }

    pub(crate) fn realms_mut(&mut self) -> &mut UiRealmDirectory {
        &mut self.realms
    }

    pub(crate) fn set_realms(&mut self, realms: UiRealmDirectory) {
        self.realms = realms;
    }

    pub(crate) const fn characters(&self) -> &UiCharacterDirectory {
        &self.characters
    }

    pub(crate) fn set_characters(&mut self, characters: UiCharacterDirectory) {
        self.characters = characters;
    }

    pub(crate) fn select_character_index(&mut self, one_based_index: u32) -> Option<u64> {
        self.characters.select_index(one_based_index)
    }
}
