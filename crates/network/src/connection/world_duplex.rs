//! Reversible full-duplex ownership for logout on the existing world connection.

use tokio::io::{AsyncRead, AsyncWrite, ReadHalf, WriteHalf};

use super::{InWorldSession, WorldPacketReader, WorldPacketWriter, WorldSession, WorldSessionInfo};
use crate::protocol::WorldAddonManifest;
use crate::session::WorldSessionError;

/// Retains authentication metadata while independently servicing encrypted I/O.
pub struct WorldSessionDuplex<S> {
    reader: WorldPacketReader<ReadHalf<S>>,
    writer: WorldPacketWriter<WriteHalf<S>>,
    account_name: String,
    realm_id: u8,
    info: WorldSessionInfo,
    addon_manifest: WorldAddonManifest,
}

impl<S: AsyncRead + AsyncWrite> InWorldSession<S> {
    /// Splits I/O while retaining everything needed by stock's logout transition.
    pub fn into_duplex(self) -> WorldSessionDuplex<S> {
        let session = self.session;
        let (reader, writer) = tokio::io::split(session.stream);
        WorldSessionDuplex {
            reader: WorldPacketReader {
                stream: reader,
                decrypter: session.decrypter,
            },
            writer: WorldPacketWriter {
                stream: writer,
                encrypter: session.encrypter,
            },
            account_name: session.account_name,
            realm_id: session.realm_id,
            info: session.info,
            addon_manifest: session.addon_manifest,
        }
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> WorldSessionDuplex<S> {
    /// Borrows both directions for futures kept alive through complete packets.
    pub fn io(
        &mut self,
    ) -> (
        &mut WorldPacketReader<ReadHalf<S>>,
        &mut WorldPacketWriter<WriteHalf<S>>,
    ) {
        (&mut self.reader, &mut self.writer)
    }

    /// Restores character-screen ownership after both packet futures have completed.
    ///
    /// # Errors
    /// Returns an ownership error if a caller exchanged one borrowed I/O half.
    pub fn into_session(self) -> Result<WorldSession<S>, WorldSessionError> {
        if !self.reader.stream.is_pair_of(&self.writer.stream) {
            return Err(WorldSessionError::MismatchedTransport);
        }
        Ok(WorldSession {
            stream: self.reader.stream.unsplit(self.writer.stream),
            encrypter: self.writer.encrypter,
            decrypter: self.reader.decrypter,
            account_name: self.account_name,
            realm_id: self.realm_id,
            info: self.info,
            addon_manifest: self.addon_manifest,
        })
    }
}
