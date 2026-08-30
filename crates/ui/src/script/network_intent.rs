//! Main-thread actions and status shared by Glue Lua and runtime networking.

use std::collections::VecDeque;
use std::fmt;

use zeroize::Zeroize;

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
}

/// Main-thread network facts queried synchronously by Glue Lua.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UiGlueNetworkStatus {
    server_name: Option<String>,
    connected: bool,
}

impl UiGlueNetworkStatus {
    /// Captures the selected server label and current transport state.
    #[must_use]
    pub fn new(server_name: Option<String>, connected: bool) -> Self {
        Self {
            server_name,
            connected,
        }
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
}

/// Ordered action mailbox plus synchronously readable connection status.
#[derive(Default)]
pub(crate) struct UiGlueNetworkBridge {
    actions: VecDeque<UiGlueNetworkAction>,
    status: UiGlueNetworkStatus,
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
}
