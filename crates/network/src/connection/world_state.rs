//! Owned world-authentication states independent of packet dependency types.

use tokio::io::{AsyncRead, AsyncWrite};
use wow_srp::wrath_header::{ClientCrypto, ClientDecrypterHalf, ClientEncrypterHalf};

use crate::protocol::{
    CharacterLoginRejection, WorldAddonManifest, WorldLocation, WorldServerPacket,
};

use super::{WorldAuthError, wow_connection::WorldHandshake};

/// The account expansion entitlement returned by the world server.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccountExpansion {
    /// Original World of Warcraft content.
    Original,
    /// The Burning Crusade content.
    TheBurningCrusade,
    /// Wrath of the Lich King content.
    WrathOfTheLichKing,
}

/// Billing and entitlement state returned by a successful authentication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldSessionInfo {
    pub(crate) billing_time: u32,
    pub(crate) billing_flags: u8,
    pub(crate) billing_rested: u32,
    pub(crate) expansion: AccountExpansion,
}

impl WorldSessionInfo {
    /// Returns the remaining billing-time field supplied by the world.
    #[must_use]
    pub const fn billing_time(self) -> u32 {
        self.billing_time
    }

    /// Returns the exact stock billing-plan flag byte.
    #[must_use]
    pub const fn billing_flags(self) -> u8 {
        self.billing_flags
    }

    /// Returns the rested-billing field supplied by the world.
    #[must_use]
    pub const fn billing_rested(self) -> u32 {
        self.billing_rested
    }

    /// Returns the account's highest enabled expansion.
    #[must_use]
    pub const fn expansion(self) -> AccountExpansion {
        self.expansion
    }
}

/// An authenticated world transport with live Wrath header cryptography.
pub struct WorldSession<S> {
    pub(crate) stream: S,
    pub(crate) crypto: ClientCrypto,
    pub(crate) account_name: String,
    pub(crate) realm_id: u8,
    pub(crate) info: WorldSessionInfo,
    pub(crate) addon_manifest: WorldAddonManifest,
}

/// A selected character awaiting the server's login result and world bootstrap.
pub struct CharacterLogin<S> {
    pub(crate) session: WorldSession<S>,
    pub(crate) character_guid: u64,
    pub(crate) character_name: String,
}

impl<S> CharacterLogin<S> {
    /// Returns the selected character's world object GUID.
    #[must_use]
    pub const fn character_guid(&self) -> u64 {
        self.character_guid
    }

    /// Returns the selected character's display name.
    #[must_use]
    pub fn character_name(&self) -> &str {
        &self.character_name
    }

    /// Returns the authenticated account name.
    #[must_use]
    pub fn account_name(&self) -> &str {
        self.session.account_name()
    }

    /// Returns the selected realmd realm identifier.
    #[must_use]
    pub const fn realm_id(&self) -> u8 {
        self.session.realm_id()
    }

    /// Returns billing and expansion state for the world session.
    #[must_use]
    pub const fn info(&self) -> WorldSessionInfo {
        self.session.info()
    }

    /// Returns the exact add-on manifest sent during world authentication.
    #[must_use]
    pub const fn addon_manifest(&self) -> &WorldAddonManifest {
        self.session.addon_manifest()
    }
}

/// The next state reached while waiting for selected-character world entry.
pub enum CharacterLoginProgress<S> {
    /// A normal setup packet arrived before the terminal login result.
    Awaiting {
        /// Pending selected-character login with live transport ownership.
        login: CharacterLogin<S>,
        /// Retained setup packet for normal subsystem dispatch.
        packet: WorldServerPacket,
    },
    /// The server accepted the character and supplied its initial world location.
    Entered(InWorldSession<S>),
    /// The server rejected the character and returned to character selection.
    Rejected {
        /// Restored authenticated character-screen session.
        session: WorldSession<S>,
        /// Exact server rejection result.
        rejection: CharacterLoginRejection,
    },
}

/// Authenticated transport after the selected character has entered a map.
pub struct InWorldSession<S> {
    pub(crate) session: WorldSession<S>,
    pub(crate) character_guid: u64,
    pub(crate) character_name: String,
    pub(crate) location: WorldLocation,
}

/// Encrypted active-world receive half with independent header state.
pub struct WorldPacketReader<R> {
    pub(crate) stream: R,
    pub(crate) decrypter: ClientDecrypterHalf,
}

/// Encrypted active-world send half with independent header state.
pub struct WorldPacketWriter<W> {
    pub(crate) stream: W,
    pub(crate) encrypter: ClientEncrypterHalf,
}

impl<S> InWorldSession<S> {
    /// Returns the active local player's world object GUID.
    #[must_use]
    pub const fn character_guid(&self) -> u64 {
        self.character_guid
    }

    /// Returns the active local player's display name.
    #[must_use]
    pub fn character_name(&self) -> &str {
        &self.character_name
    }

    /// Returns the initial authoritative map and transform.
    #[must_use]
    pub const fn location(&self) -> WorldLocation {
        self.location
    }

    /// Returns the authenticated account name.
    #[must_use]
    pub fn account_name(&self) -> &str {
        self.session.account_name()
    }

    /// Returns the selected realmd realm identifier.
    #[must_use]
    pub const fn realm_id(&self) -> u8 {
        self.session.realm_id()
    }

    /// Returns billing and expansion state for the world session.
    #[must_use]
    pub const fn info(&self) -> WorldSessionInfo {
        self.session.info()
    }

    /// Returns the add-on manifest sent during authentication.
    #[must_use]
    pub const fn addon_manifest(&self) -> &WorldAddonManifest {
        self.session.addon_manifest()
    }

    /// Separates full-duplex transport and header cryptography for a live pump.
    ///
    /// Reads may then remain pending while latency and clock packets are sent;
    /// neither operation can cancel a partially consumed encrypted frame.
    pub fn split(
        self,
    ) -> (
        WorldPacketReader<tokio::io::ReadHalf<S>>,
        WorldPacketWriter<tokio::io::WriteHalf<S>>,
    )
    where
        S: AsyncRead + AsyncWrite,
    {
        let (reader, writer) = tokio::io::split(self.session.stream);
        let (encrypter, decrypter) = self.session.crypto.split();
        (
            WorldPacketReader {
                stream: reader,
                decrypter,
            },
            WorldPacketWriter {
                stream: writer,
                encrypter,
            },
        )
    }
}

impl<S> WorldSession<S> {
    /// Returns the normalized account name authenticated on this world.
    #[must_use]
    pub fn account_name(&self) -> &str {
        &self.account_name
    }

    /// Returns the selected realmd realm identifier.
    #[must_use]
    pub const fn realm_id(&self) -> u8 {
        self.realm_id
    }

    /// Returns billing and expansion state from `SMSG_AUTH_RESPONSE`.
    #[must_use]
    pub const fn info(&self) -> WorldSessionInfo {
        self.info
    }

    /// Returns the exact ordered manifest sent during world authentication.
    ///
    /// The server's add-on policy response is positional and must be decoded
    /// against this manifest.
    #[must_use]
    pub const fn addon_manifest(&self) -> &WorldAddonManifest {
        &self.addon_manifest
    }

    /// Consumes the session and returns its transport after discarding crypto state.
    #[must_use]
    pub fn into_stream(self) -> S {
        let Self {
            stream,
            crypto: _crypto,
            ..
        } = self;
        stream
    }
}

/// A normal world login queue state awaiting another encrypted response.
pub struct WorldQueue<S> {
    pub(crate) handshake: WorldHandshake<S>,
    pub(crate) position: u32,
    pub(crate) realm_has_free_character_migration: bool,
}

impl<S> WorldQueue<S> {
    /// Returns the server-reported queue position.
    #[must_use]
    pub const fn position(&self) -> u32 {
        self.position
    }

    /// Returns whether free character migration is available while queued.
    #[must_use]
    pub const fn realm_has_free_character_migration(&self) -> bool {
        self.realm_has_free_character_migration
    }
}

impl<S> WorldQueue<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    /// Waits for the next encrypted queue update or final authentication result.
    ///
    /// # Errors
    ///
    /// Returns [`WorldAuthError`] for transport, decode, ordering, or rejection failures.
    pub async fn advance(self) -> Result<WorldAuthProgress<S>, WorldAuthError> {
        self.handshake.advance().await
    }
}

/// The next normal state reached by a world authentication response.
pub enum WorldAuthProgress<S> {
    /// The account has entered the world login queue.
    Queued(WorldQueue<S>),
    /// The world accepted the session and enabled encrypted packet headers.
    Authenticated(WorldSession<S>),
}
