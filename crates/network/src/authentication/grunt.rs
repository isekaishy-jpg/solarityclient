//! Validated credentials and build-12340 login identity from `Grunt.cpp`.

use std::fmt;
use std::net::Ipv4Addr;

use tokio::io::{AsyncWrite, AsyncWriteExt};
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
    /// Simplified Chinese.
    ZhCn,
    /// Legacy English Taiwan token.
    EnTw,
    /// Legacy English China token.
    EnCn,
}

impl LoginLocale {
    /// Returns the four-byte client locale tag before little-endian wire order.
    const fn protocol_tag(self) -> u32 {
        match self {
            Self::EnGb => u32::from_be_bytes(*b"enGB"),
            Self::EnUs => u32::from_be_bytes(*b"enUS"),
            Self::EsMx => u32::from_be_bytes(*b"esMX"),
            Self::PtBr => u32::from_be_bytes(*b"ptBR"),
            Self::FrFr => u32::from_be_bytes(*b"frFR"),
            Self::DeDe => u32::from_be_bytes(*b"deDE"),
            Self::EsEs => u32::from_be_bytes(*b"esES"),
            Self::PtPt => u32::from_be_bytes(*b"ptPT"),
            Self::ItIt => u32::from_be_bytes(*b"itIT"),
            Self::RuRu => u32::from_be_bytes(*b"ruRU"),
            Self::KoKr => u32::from_be_bytes(*b"koKR"),
            Self::ZhTw => u32::from_be_bytes(*b"zhTW"),
            Self::ZhCn => u32::from_be_bytes(*b"zhCN"),
            Self::EnTw => u32::from_be_bytes(*b"enTW"),
            Self::EnCn => u32::from_be_bytes(*b"enCN"),
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

    pub(super) async fn write_challenge(
        self,
        mut stream: impl AsyncWrite + Unpin + Send,
        account_name: &str,
    ) -> Result<(), std::io::Error> {
        // Although Solarity is a 64-bit process, build 12340's wire protocol
        // has only the original x86 Windows identity understood by realmd.
        // This packet is owned here because `wow_login_messages` 0.5 omits
        // build 12340's `zhCN` locale from its otherwise closed locale enum.
        const FIXED_SIZE_WITHOUT_OPCODE: usize = 33;
        let packet_size = FIXED_SIZE_WITHOUT_OPCODE + account_name.len();
        let payload_size = u16::try_from(packet_size - 3).map_err(|_source| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "login challenge exceeds its 16-bit size field",
            )
        })?;
        let account_size = u8::try_from(account_name.len()).map_err(|_source| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "login account exceeds its one-byte size field",
            )
        })?;
        let mut packet = Vec::with_capacity(packet_size + 1);
        packet.push(0x00); // CMD_AUTH_LOGON_CHALLENGE
        packet.push(8); // ProtocolVersion::Eight
        packet.extend_from_slice(&payload_size.to_le_bytes());
        packet.extend_from_slice(&0x0057_6F57_u32.to_le_bytes()); // "WoW\0"
        packet.extend_from_slice(&[3, 3, 5]);
        packet.extend_from_slice(&12_340_u16.to_le_bytes());
        packet.extend_from_slice(&0x0078_3836_u32.to_le_bytes()); // "\0x86"
        packet.extend_from_slice(&0x0057_696E_u32.to_le_bytes()); // "\0Win"
        packet.extend_from_slice(&self.locale.protocol_tag().to_le_bytes());
        packet.extend_from_slice(&self.utc_timezone_offset_minutes.to_le_bytes());
        packet.extend_from_slice(&self.client_ip_address.octets());
        packet.push(account_size);
        packet.extend_from_slice(account_name.as_bytes());
        stream.write_all(&packet).await
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
