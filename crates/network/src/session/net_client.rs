//! Owned encrypted packet I/O recovered from `NetClient.cpp`.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use wow_srp::wrath_header::WrathServerAttempt;
use wow_world_messages::wrath::opcodes::ClientOpcodeMessage;

use crate::connection::WorldSession;
use crate::protocol::WorldServerPacket;

use super::{WorldSessionError, WorldSessionStage};

const MAX_SERVER_PACKET_BODY_BYTES: u32 = 0x7F_FFFF - 2;

impl<S> WorldSession<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    /// Sends `CMSG_CHAR_ENUM` through the authenticated encrypted header stream.
    ///
    /// The corresponding response is obtained with [`Self::receive_packet`],
    /// allowing normal interleaved setup packets to be retained and dispatched.
    ///
    /// # Errors
    ///
    /// Returns [`WorldSessionError`] when the encrypted header or packet body
    /// cannot be written.
    pub async fn request_character_directory(&mut self) -> Result<(), WorldSessionError> {
        ClientOpcodeMessage::CMSG_CHAR_ENUM
            .tokio_write_encrypted_client(&mut self.stream, self.crypto.encrypter())
            .await
            .map_err(|error| WorldSessionError::Io {
                stage: WorldSessionStage::Send,
                message: error.to_string(),
            })
    }

    /// Receives one encrypted packet without discarding unsupported opcodes.
    ///
    /// # Errors
    ///
    /// Returns [`WorldSessionError`] when the encrypted header or packet body
    /// cannot be read and decoded.
    pub async fn receive_packet(&mut self) -> Result<WorldServerPacket, WorldSessionError> {
        let mut header_bytes = [0_u8; 4];
        self.stream
            .read_exact(&mut header_bytes)
            .await
            .map_err(|error| WorldSessionError::Io {
                stage: WorldSessionStage::Receive,
                message: error.to_string(),
            })?;
        let header = match self
            .crypto
            .decrypter()
            .attempt_decrypt_server_header(header_bytes)
        {
            WrathServerAttempt::Header(header) => header,
            WrathServerAttempt::AdditionalByteRequired => {
                let additional =
                    self.stream
                        .read_u8()
                        .await
                        .map_err(|error| WorldSessionError::Io {
                            stage: WorldSessionStage::Receive,
                            message: error.to_string(),
                        })?;
                self.crypto
                    .decrypter()
                    .decrypt_large_server_header(additional)
            }
        };
        let body_size = header
            .size
            .checked_sub(2)
            .ok_or_else(|| WorldSessionError::Decode {
                stage: WorldSessionStage::Receive,
                message: "server header size is smaller than its opcode".to_owned(),
            })?;
        if body_size > MAX_SERVER_PACKET_BODY_BYTES {
            return Err(WorldSessionError::Decode {
                stage: WorldSessionStage::Receive,
                message: format!(
                    "server body has {body_size} bytes; protocol maximum is {MAX_SERVER_PACKET_BODY_BYTES}"
                ),
            });
        }
        let mut payload = vec![0_u8; body_size as usize];
        self.stream
            .read_exact(&mut payload)
            .await
            .map_err(|error| WorldSessionError::Io {
                stage: WorldSessionStage::Receive,
                message: error.to_string(),
            })?;
        Ok(WorldServerPacket::new(header.opcode, payload))
    }
}
