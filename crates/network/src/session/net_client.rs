//! Owned encrypted packet I/O recovered from `NetClient.cpp`.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use wow_srp::wrath_header::WrathServerAttempt;
use wow_world_messages::Guid;
use wow_world_messages::wrath::opcodes::ClientOpcodeMessage;
use wow_world_messages::wrath::{
    CMSG_AREATRIGGER, CMSG_CHAR_CREATE, CMSG_CHAR_DELETE, CMSG_CHAR_RENAME, CMSG_CREATURE_QUERY,
    CMSG_GAMEOBJECT_QUERY, CMSG_PING, CMSG_PLAYER_LOGIN, CMSG_READY_FOR_ACCOUNT_DATA_TIMES,
    CMSG_REALM_SPLIT, CMSG_TIME_SYNC_RESP, MSG_MOVE_WORLDPORT_ACK,
};

use crate::connection::{
    CharacterLogin, CharacterLoginProgress, InWorldSession, WorldPacketReader, WorldPacketWriter,
    WorldSession,
};
use crate::protocol::{CharacterCreation, CharacterEntry, WorldMovementMessage, WorldServerPacket};

use super::{WorldSessionError, WorldSessionStage};

const MAX_SERVER_PACKET_BODY_BYTES: u32 = 0x7F_FFFF - 2;

impl<S> WorldSession<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    /// Sends stock's empty account-data readiness notification.
    ///
    /// # Errors
    ///
    /// Returns [`WorldSessionError`] when the encrypted packet cannot be written.
    pub async fn ready_for_account_data_times(&mut self) -> Result<(), WorldSessionError> {
        self.send_character_screen_message(ClientOpcodeMessage::from(
            CMSG_READY_FOR_ACCOUNT_DATA_TIMES {},
        ))
        .await
    }

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
        self.send_character_screen_message(ClientOpcodeMessage::CMSG_CHAR_ENUM)
            .await
    }

    /// Sends one validated `CMSG_CHAR_CREATE` request.
    ///
    /// The authoritative result is obtained with [`Self::receive_packet`] so
    /// unrelated setup packets remain available to the character-screen owner.
    ///
    /// # Errors
    ///
    /// Returns [`WorldSessionError`] when the encrypted packet cannot be written.
    pub async fn create_character(
        &mut self,
        request: &CharacterCreation,
    ) -> Result<(), WorldSessionError> {
        let [skin_color, face, hair_style, hair_color, facial_hair] = request.appearance();
        self.send_character_screen_message(ClientOpcodeMessage::from(CMSG_CHAR_CREATE {
            name: request.name().to_owned(),
            race: request.race(),
            class: request.class(),
            gender: request.gender(),
            skin_color,
            face,
            hair_style,
            hair_color,
            facial_hair,
        }))
        .await
    }

    /// Sends one exact `CMSG_CHAR_DELETE` for an enumerated character GUID.
    ///
    /// The authoritative result is obtained with [`Self::receive_packet`] so
    /// unrelated setup packets remain available to the character-screen owner.
    ///
    /// # Errors
    ///
    /// Returns [`WorldSessionError`] when the encrypted packet cannot be written.
    pub async fn delete_character(&mut self, guid: u64) -> Result<(), WorldSessionError> {
        self.send_character_screen_message(ClientOpcodeMessage::from(CMSG_CHAR_DELETE {
            guid: Guid::new(guid),
        }))
        .await
    }

    /// Sends one exact `CMSG_CHAR_RENAME` for an enumerated character GUID.
    ///
    /// # Errors
    ///
    /// Returns [`WorldSessionError`] when the encrypted packet cannot be written.
    pub async fn rename_character(
        &mut self,
        guid: u64,
        request: &crate::CharacterRename,
    ) -> Result<(), WorldSessionError> {
        self.send_character_screen_message(ClientOpcodeMessage::from(CMSG_CHAR_RENAME {
            character: Guid::new(guid),
            new_name: request.name().to_owned(),
        }))
        .await
    }

    /// Sends `CMSG_REALM_SPLIT` for the realm selected during authentication.
    ///
    /// # Errors
    ///
    /// Returns [`WorldSessionError`] when the encrypted packet cannot be written.
    pub async fn request_realm_split_info(&mut self) -> Result<(), WorldSessionError> {
        let realm_id = u32::from(self.realm_id());
        self.send_character_screen_message(ClientOpcodeMessage::from(CMSG_REALM_SPLIT { realm_id }))
            .await
    }

    async fn send_character_screen_message(
        &mut self,
        message: ClientOpcodeMessage,
    ) -> Result<(), WorldSessionError> {
        message
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

    /// Sends stock's periodic encrypted latency probe.
    ///
    /// # Errors
    ///
    /// Returns [`WorldSessionError`] when the encrypted packet cannot be written.
    pub async fn send_ping(
        &mut self,
        sequence_id: u32,
        round_time_in_ms: u32,
    ) -> Result<(), WorldSessionError> {
        ClientOpcodeMessage::from(CMSG_PING {
            sequence_id,
            round_time_in_ms,
        })
        .tokio_write_encrypted_client(&mut self.session.stream, self.session.crypto.encrypter())
        .await
        .map_err(|error| WorldSessionError::Io {
            stage: WorldSessionStage::Send,
            message: error.to_string(),
        })
    }

    /// Acknowledges one server time-sync counter with process-monotonic ticks.
    ///
    /// # Errors
    ///
    /// Returns [`WorldSessionError`] when the encrypted packet cannot be written.
    pub async fn send_time_sync_response(
        &mut self,
        counter: u32,
        client_ticks: u32,
    ) -> Result<(), WorldSessionError> {
        ClientOpcodeMessage::from(CMSG_TIME_SYNC_RESP {
            time_sync: counter,
            client_ticks,
        })
        .tokio_write_encrypted_client(&mut self.session.stream, self.session.crypto.encrypter())
        .await
        .map_err(|error| WorldSessionError::Io {
            stage: WorldSessionStage::Send,
            message: error.to_string(),
        })
    }
}

impl<R> WorldPacketReader<R>
where
    R: AsyncRead + Unpin + Send,
{
    /// Receives one encrypted packet while the send half remains independent.
    ///
    /// # Errors
    ///
    /// Returns [`WorldSessionError`] when the header or body cannot be read.
    pub async fn receive_packet(&mut self) -> Result<WorldServerPacket, WorldSessionError> {
        receive_packet_from(&mut self.stream, &mut self.decrypter).await
    }
}

impl<W> WorldPacketWriter<W>
where
    W: AsyncWrite + Unpin + Send,
{
    /// Queries the native creature cache by entry and full requesting GUID.
    ///
    /// # Errors
    /// Returns an I/O error if the encrypted packet cannot be completed.
    pub async fn send_creature_query(
        &mut self,
        entry: u32,
        guid: u64,
    ) -> Result<(), WorldSessionError> {
        ClientOpcodeMessage::from(CMSG_CREATURE_QUERY {
            creature: entry,
            guid: Guid::new(guid),
        })
        .tokio_write_encrypted_client(&mut self.stream, &mut self.encrypter)
        .await
        .map_err(|error| WorldSessionError::Io {
            stage: WorldSessionStage::Send,
            message: error.to_string(),
        })
    }

    /// Queries one template using its entry and the requesting instance's full GUID.
    ///
    /// # Errors
    /// Returns an I/O error if the encrypted packet cannot be completed.
    pub async fn send_game_object_query(
        &mut self,
        entry: u32,
        guid: u64,
    ) -> Result<(), WorldSessionError> {
        ClientOpcodeMessage::from(CMSG_GAMEOBJECT_QUERY {
            entry_id: entry,
            guid: Guid::new(guid),
        })
        .tokio_write_encrypted_client(&mut self.stream, &mut self.encrypter)
        .await
        .map_err(|error| WorldSessionError::Io {
            stage: WorldSessionStage::Send,
            message: error.to_string(),
        })
    }

    /// Sends native `0x00717D90`'s packed mover and unsigned skipped interval.
    ///
    /// # Errors
    /// Returns an I/O error if the encrypted packet cannot be completed.
    pub async fn send_movement_time_skipped(
        &mut self,
        guid: u64,
        milliseconds: u32,
    ) -> Result<(), WorldSessionError> {
        let mut body = [0_u8; 13];
        let mut length = 1;
        for (index, byte) in guid.to_le_bytes().into_iter().enumerate() {
            if byte != 0 {
                body[0] |= 1 << index;
                body[length] = byte;
                length += 1;
            }
        }
        body[length..length + 4].copy_from_slice(&milliseconds.to_le_bytes());
        self.send_local_movement_auxiliary(0x2ce, &body[..length + 4])
            .await
    }

    /// Sends the local player's requested stand state (`CMSG_STANDSTATECHANGE`).
    ///
    /// # Errors
    /// Returns an I/O error if the encrypted packet cannot be completed.
    pub async fn send_stand_state(&mut self, state: u32) -> Result<(), WorldSessionError> {
        self.send_local_movement_auxiliary(0x101, &state.to_le_bytes())
            .await
    }

    /// Acknowledges one zero-based tutorial (`CMSG_TUTORIAL_FLAG`).
    ///
    /// # Errors
    /// Returns an I/O error if the encrypted packet cannot be completed.
    pub async fn send_tutorial_flag(&mut self, index: u32) -> Result<(), WorldSessionError> {
        self.send_local_movement_auxiliary(0xfe, &index.to_le_bytes())
            .await
    }

    /// Completes all tutorial flags (`CMSG_TUTORIAL_CLEAR`).
    ///
    /// # Errors
    /// Returns an I/O error if the encrypted packet cannot be completed.
    pub async fn send_tutorial_clear(&mut self) -> Result<(), WorldSessionError> {
        self.send_local_movement_auxiliary(0xff, &[]).await
    }

    /// Resets tutorial flags (`CMSG_TUTORIAL_RESET`).
    ///
    /// # Errors
    /// Returns an I/O error if the encrypted packet cannot be completed.
    pub async fn send_tutorial_reset(&mut self) -> Result<(), WorldSessionError> {
        self.send_local_movement_auxiliary(0x100, &[]).await
    }

    /// Selects a client-owned mover using `0x026A`'s full, unpacked GUID.
    ///
    /// # Errors
    /// Returns an I/O error if the encrypted packet cannot be completed.
    pub async fn send_active_mover(&mut self, guid: u64) -> Result<(), WorldSessionError> {
        self.send_local_movement_auxiliary(0x26a, &guid.to_le_bytes())
            .await
    }

    async fn send_local_movement_auxiliary(
        &mut self,
        opcode: u32,
        body: &[u8],
    ) -> Result<(), WorldSessionError> {
        let mut packet = [0_u8; 19];
        let header = self
            .encrypter
            .encrypt_client_header((body.len() + 4) as u16, opcode);
        packet[..6].copy_from_slice(&header);
        packet[6..6 + body.len()].copy_from_slice(body);
        self.stream
            .write_all(&packet[..6 + body.len()])
            .await
            .map_err(|error| WorldSessionError::Io {
                stage: WorldSessionStage::Send,
                message: error.to_string(),
            })
    }

    /// Sends a validated event-time movement image through this cipher owner.
    ///
    /// As with other encrypted writes, the session owner must keep the future
    /// alive through partial I/O or terminate the connection when cancelling it.
    ///
    /// # Errors
    ///
    /// Returns [`WorldSessionError`] if the encrypted packet cannot be written.
    pub async fn send_movement(
        &mut self,
        message: &WorldMovementMessage,
    ) -> Result<(), WorldSessionError> {
        let body = message.body();
        // Six header bytes plus the proven 97-byte maximum movement body.
        let mut packet = [0_u8; 103];
        let header = self
            .encrypter
            .encrypt_client_header((body.len() + 4) as u16, message.kind() as u32);
        packet[..6].copy_from_slice(&header);
        packet[6..6 + body.len()].copy_from_slice(body);
        self.stream
            .write_all(&packet[..6 + body.len()])
            .await
            .map_err(|error| WorldSessionError::Io {
                stage: WorldSessionStage::Send,
                message: error.to_string(),
            })
    }

    /// Notifies the server that the player entered an authored AreaTrigger volume.
    ///
    /// # Errors
    ///
    /// Returns [`WorldSessionError`] when the encrypted packet cannot be written.
    pub async fn send_area_trigger(&mut self, trigger_id: u32) -> Result<(), WorldSessionError> {
        ClientOpcodeMessage::from(CMSG_AREATRIGGER { trigger_id })
            .tokio_write_encrypted_client(&mut self.stream, &mut self.encrypter)
            .await
            .map_err(|error| WorldSessionError::Io {
                stage: WorldSessionStage::Send,
                message: error.to_string(),
            })
    }

    /// Acknowledges a loaded destination with stock's empty opcode `0x00DC`.
    ///
    /// The application calls this after map loading, before waiting for the
    /// replacement player (`0x00403B70`). It is not a loading-card dismissal.
    ///
    /// # Errors
    ///
    /// Returns [`WorldSessionError`] when the encrypted packet cannot be written.
    pub async fn send_worldport_acknowledgement(&mut self) -> Result<(), WorldSessionError> {
        ClientOpcodeMessage::from(MSG_MOVE_WORLDPORT_ACK {})
            .tokio_write_encrypted_client(&mut self.stream, &mut self.encrypter)
            .await
            .map_err(|error| WorldSessionError::Io {
                stage: WorldSessionStage::Send,
                message: error.to_string(),
            })
    }

    /// Sends stock's periodic encrypted latency probe.
    ///
    /// # Errors
    ///
    /// Returns [`WorldSessionError`] when the packet cannot be written.
    pub async fn send_ping(
        &mut self,
        sequence_id: u32,
        round_time_in_ms: u32,
    ) -> Result<(), WorldSessionError> {
        ClientOpcodeMessage::from(CMSG_PING {
            sequence_id,
            round_time_in_ms,
        })
        .tokio_write_encrypted_client(&mut self.stream, &mut self.encrypter)
        .await
        .map_err(|error| WorldSessionError::Io {
            stage: WorldSessionStage::Send,
            message: error.to_string(),
        })
    }

    /// Acknowledges one server time-sync request.
    ///
    /// # Errors
    ///
    /// Returns [`WorldSessionError`] when the packet cannot be written.
    pub async fn send_time_sync_response(
        &mut self,
        counter: u32,
        client_ticks: u32,
    ) -> Result<(), WorldSessionError> {
        ClientOpcodeMessage::from(CMSG_TIME_SYNC_RESP {
            time_sync: counter,
            client_ticks,
        })
        .tokio_write_encrypted_client(&mut self.stream, &mut self.encrypter)
        .await
        .map_err(|error| WorldSessionError::Io {
            stage: WorldSessionStage::Send,
            message: error.to_string(),
        })
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
    receive_packet_from(stream, crypto.decrypter()).await
}

async fn receive_packet_from<S>(
    stream: &mut S,
    decrypter: &mut wow_srp::wrath_header::ClientDecrypterHalf,
) -> Result<WorldServerPacket, WorldSessionError>
where
    S: AsyncRead + Unpin + Send,
{
    let mut header_bytes = [0_u8; 4];
    stream
        .read_exact(&mut header_bytes)
        .await
        .map_err(|error| WorldSessionError::Io {
            stage: WorldSessionStage::Receive,
            message: error.to_string(),
        })?;
    let header = match decrypter.attempt_decrypt_server_header(header_bytes) {
        WrathServerAttempt::Header(header) => header,
        WrathServerAttempt::AdditionalByteRequired => {
            let additional = stream
                .read_u8()
                .await
                .map_err(|error| WorldSessionError::Io {
                    stage: WorldSessionStage::Receive,
                    message: error.to_string(),
                })?;
            decrypter.decrypt_large_server_header(additional)
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
