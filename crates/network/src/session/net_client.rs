//! Owned encrypted packet I/O recovered from `NetClient.cpp`.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use wow_srp::wrath_header::WrathServerAttempt;
use wow_world_messages::Guid;
use wow_world_messages::wrath::CMSG_PLAYER_LOGIN;
use wow_world_messages::wrath::opcodes::ClientOpcodeMessage;

use crate::connection::{CharacterLogin, CharacterLoginProgress, InWorldSession, WorldSession};
use crate::protocol::{CharacterEntry, WorldServerPacket};

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

    /// Selects one enumerated character and sends `CMSG_PLAYER_LOGIN`.
    ///
    /// Consuming the character-screen session makes the pending login state
    /// explicit until the server either rejects the selection or completes the
    /// world bootstrap.
    ///
    /// # Errors
    ///
    /// Returns [`WorldSessionError`] when the encrypted packet cannot be written.
    pub async fn login_character(
        mut self,
        character: &CharacterEntry,
    ) -> Result<CharacterLogin<S>, WorldSessionError> {
        ClientOpcodeMessage::from(CMSG_PLAYER_LOGIN {
            guid: Guid::new(character.guid()),
        })
        .tokio_write_encrypted_client(&mut self.stream, self.crypto.encrypter())
        .await
        .map_err(|error| WorldSessionError::Io {
            stage: WorldSessionStage::Send,
            message: error.to_string(),
        })?;
        Ok(CharacterLogin {
            session: self,
            character_guid: character.guid(),
            character_name: character.name().to_owned(),
        })
    }

    /// Receives one encrypted packet without discarding unsupported opcodes.
    ///
    /// # Errors
    ///
    /// Returns [`WorldSessionError`] when the encrypted header or packet body
    /// cannot be read and decoded.
    pub async fn receive_packet(&mut self) -> Result<WorldServerPacket, WorldSessionError> {
        receive_packet(&mut self.stream, &mut self.crypto).await
    }
}

impl<S> CharacterLogin<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    /// Receives one encrypted bootstrap packet without discarding unsupported opcodes.
    ///
    /// # Errors
    ///
    /// Returns [`WorldSessionError`] when the encrypted header or packet body
    /// cannot be read and decoded.
    pub async fn receive_packet(&mut self) -> Result<WorldServerPacket, WorldSessionError> {
        receive_packet(&mut self.session.stream, &mut self.session.crypto).await
    }

    /// Advances world entry by one encrypted server packet.
    ///
    /// Setup packets received before the terminal result are retained in
    /// [`CharacterLoginProgress::Awaiting`] for normal subsystem dispatch.
    ///
    /// # Errors
    ///
    /// Returns [`WorldSessionError`] when packet I/O or terminal-result decoding fails.
    pub async fn advance(mut self) -> Result<CharacterLoginProgress<S>, WorldSessionError> {
        let packet = receive_packet(&mut self.session.stream, &mut self.session.crypto).await?;
        if let Some(location) = packet.world_location().map_err(world_entry_decode_error)? {
            return Ok(CharacterLoginProgress::Entered(InWorldSession {
                session: self.session,
                character_guid: self.character_guid,
                character_name: self.character_name,
                location,
            }));
        }
        if let Some(rejection) = packet
            .character_login_rejection()
            .map_err(world_entry_decode_error)?
        {
            return Ok(CharacterLoginProgress::Rejected {
                session: self.session,
                rejection,
            });
        }
        Ok(CharacterLoginProgress::Awaiting {
            login: self,
            packet,
        })
    }
}

impl<S> InWorldSession<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    /// Receives one encrypted active-world packet without discarding unsupported opcodes.
    ///
    /// # Errors
    ///
    /// Returns [`WorldSessionError`] when the encrypted header or body cannot be read.
    pub async fn receive_packet(&mut self) -> Result<WorldServerPacket, WorldSessionError> {
        receive_packet(&mut self.session.stream, &mut self.session.crypto).await
    }
}

fn world_entry_decode_error(error: crate::protocol::WorldEntryPacketError) -> WorldSessionError {
    WorldSessionError::Decode {
        stage: WorldSessionStage::Receive,
        message: error.to_string(),
    }
}

async fn receive_packet<S>(
    stream: &mut S,
    crypto: &mut wow_srp::wrath_header::ClientCrypto,
) -> Result<WorldServerPacket, WorldSessionError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    let mut header_bytes = [0_u8; 4];
    stream
        .read_exact(&mut header_bytes)
        .await
        .map_err(|error| WorldSessionError::Io {
            stage: WorldSessionStage::Receive,
            message: error.to_string(),
        })?;
    let header = match crypto
        .decrypter()
        .attempt_decrypt_server_header(header_bytes)
    {
        WrathServerAttempt::Header(header) => header,
        WrathServerAttempt::AdditionalByteRequired => {
            let additional = stream
                .read_u8()
                .await
                .map_err(|error| WorldSessionError::Io {
                    stage: WorldSessionStage::Receive,
                    message: error.to_string(),
                })?;
            crypto.decrypter().decrypt_large_server_header(additional)
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
    stream
        .read_exact(&mut payload)
        .await
        .map_err(|error| WorldSessionError::Io {
            stage: WorldSessionStage::Receive,
            message: error.to_string(),
        })?;
    Ok(WorldServerPacket::new(header.opcode, payload))
}
