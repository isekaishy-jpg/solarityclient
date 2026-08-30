//! Validated credentials and build-12340 login identity from `Grunt.cpp`.

use std::fmt;
use std::net::Ipv4Addr;

use wow_login_messages::all::CMD_AUTH_LOGON_CHALLENGE_Client;
use wow_login_messages::all::{Locale, Os, Platform, ProtocolVersion, Version};
use wow_srp::normalized_string::NormalizedString;

use super::LoginError;

/// A stock login locale represented without exposing protocol dependency types.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LoginLocale {
    /// British English.
    EnGb,
    /// American English.
    EnUs,
    /// Mexican Spanish.
    EsMx,
    /// Brazilian Portuguese.
    PtBr,
    /// French.
    FrFr,
    /// German.
    DeDe,
    /// European Spanish.
    EsEs,
    /// European Portuguese.
    PtPt,
    /// Italian.
    ItIt,
    /// Russian.
    RuRu,
    /// Korean.
    KoKr,
    /// Traditional Chinese.
    ZhTw,
    /// Legacy English Taiwan token.
    EnTw,
    /// Legacy English China token.
    EnCn,
}

impl LoginLocale {
    pub(super) const fn protocol(self) -> Locale {
        match self {
            Self::EnGb => Locale::EnGb,
            Self::EnUs => Locale::EnUs,
            Self::EsMx => Locale::EsMx,
            Self::PtBr => Locale::PtBr,
            Self::FrFr => Locale::FrFr,
            Self::DeDe => Locale::DeDe,
            Self::EsEs => Locale::EsEs,
            Self::PtPt => Locale::PtPt,
            Self::ItIt => Locale::ItIt,
            Self::RuRu => Locale::RuRu,
            Self::KoKr => Locale::KoKr,
            Self::ZhTw => Locale::ZhTw,
            Self::EnTw => Locale::EnTw,
            Self::EnCn => Locale::EnCn,
        }
    }
}

/// Public inputs used to construct the exact build-12340 challenge packet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GruntLoginOptions {
    locale: LoginLocale,
    utc_timezone_offset_minutes: i32,
    client_ip_address: Ipv4Addr,
}

impl GruntLoginOptions {
    /// Creates explicit login options without guessing locale, timezone, or IP state.
    #[must_use]
    pub const fn new(
        locale: LoginLocale,
        utc_timezone_offset_minutes: i32,
        client_ip_address: Ipv4Addr,
    ) -> Self {
        Self {
            locale,
            utc_timezone_offset_minutes,
            client_ip_address,
        }
    }

    /// Returns the locale sent to the login server.
    #[must_use]
    pub const fn locale(self) -> LoginLocale {
        self.locale
    }

    pub(super) fn challenge(self, account_name: String) -> CMD_AUTH_LOGON_CHALLENGE_Client {
        // Although Solarity is a 64-bit process, build 12340's wire protocol
        // has only the original x86 Windows identity understood by realmd.
        CMD_AUTH_LOGON_CHALLENGE_Client {
            protocol_version: ProtocolVersion::Eight,
            version: Version {
                major: 3,
                minor: 3,
                patch: 5,
                build: 12_340,
            },
            platform: Platform::X86,
            os: Os::Windows,
            locale: self.locale.protocol(),
            utc_timezone_offset: self.utc_timezone_offset_minutes,
            client_ip_address: self.client_ip_address,
            account_name,
        }
    }
}

/// Supplies the challenge-dependent executable integrity proof used by stock login.
///
/// Keeping this as an explicit service prevents the network layer from sending an
/// invented zero hash when the server's CRC salt requires a real build proof.
pub trait GruntIntegrity {
    /// Calculates the 20-byte proof for the server-provided CRC salt.
    ///
    /// # Errors
    ///
    /// `client_public_key` is the exact 32-byte value sent beside the result;
    /// strict build verification binds the version seed to that ephemeral key.
    ///
    /// Returns a stable login error when the challenge or pinned build inputs
    /// cannot produce the exact proof.
    fn proof(
        &self,
        crc_salt: [u8; 16],
        client_public_key: [u8; 32],
    ) -> Result<[u8; 20], LoginError>;
}

/// Uppercased, stock-valid Grunt account credentials.
#[derive(Clone)]
pub struct GruntCredentials {
    pub(super) username: NormalizedString,
    pub(super) password: NormalizedString,
}

impl GruntCredentials {
    /// Validates and normalizes credentials exactly once at the network boundary.
    ///
    /// # Errors
    ///
    /// Returns [`LoginError::InvalidUsername`] or [`LoginError::InvalidPassword`]
    /// when a value violates the stock 16-byte printable-ASCII constraint.
    pub fn new(username: &str, password: &str) -> Result<Self, LoginError> {
        let username =
            NormalizedString::new(username).map_err(|error| LoginError::InvalidUsername {
                message: error.to_string(),
            })?;
        let password =
            NormalizedString::new(password).map_err(|error| LoginError::InvalidPassword {
                message: error.to_string(),
            })?;
        Ok(Self { username, password })
    }

    /// Returns the uppercase account name sent on the wire.
    #[must_use]
    pub fn username(&self) -> &str {
        self.username.as_ref()
    }
}

impl fmt::Debug for GruntCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GruntCredentials")
            .field("username", &self.username())
            .field("password", &"<redacted>")
            .finish()
    }
}
