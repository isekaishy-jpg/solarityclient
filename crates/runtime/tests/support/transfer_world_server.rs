//! Loopback world server using real stock authentication and encrypted framing.

use std::error::Error;

use solarity_network::{
    CharacterLoginProgress, InWorldSession, WorldAddonManifest, WorldAuthProgress, WorldConnection,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use wow_srp::normalized_string::NormalizedString;
use wow_srp::wrath_header::{ProofSeed, ServerCrypto};
use wow_world_messages::Guid;
use wow_world_messages::wrath::opcodes::{ClientOpcodeMessage, ServerOpcodeMessage};
use wow_world_messages::wrath::{
    BillingPlanFlags, Character, Class, Expansion, Gender, Race, SMSG_AUTH_CHALLENGE,
    SMSG_AUTH_RESPONSE, SMSG_CHAR_ENUM,
};

use super::transfer_authentication::authenticated_identity_and_realm;

pub type TestError = Box<dyn Error + Send + Sync>;

pub type RawClientPacket = (u32, Vec<u8>);

/// Reply decoding is chosen by the test so native wire assertions need not
/// inherit a third-party message schema's interpretation of individual fields.
enum ExchangeReply {
    Decoded(oneshot::Sender<Result<Vec<ClientOpcodeMessage>, TestError>>),
    Raw(oneshot::Sender<Result<Vec<RawClientPacket>, TestError>>),
}

/// A controlled publish operation and the exact responses expected afterward.
struct Exchange {
    packets: Vec<(u16, Vec<u8>)>,
    response_count: usize,
    complete: ExchangeReply,
}

/// Owns the server task; dropping the fixture closes all pending I/O.
pub struct WorldServer {
    commands: mpsc::Sender<Exchange>,
    task: JoinHandle<Result<(), TestError>>,
}

impl WorldServer {
    /// Authenticates over loopback and returns an accepted world session.
    pub async fn connect() -> Result<(Self, InWorldSession<TcpStream>), TestError> {
        let (identity, realm, session_key) = authenticated_identity_and_realm().await?;
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let (commands, receiver) = mpsc::channel(8);
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await?;
            let mut crypto = authenticate(&mut stream, session_key).await?;
            let request =
                ClientOpcodeMessage::tokio_read_encrypted(&mut stream, crypto.decrypter()).await?;
            assert!(matches!(request, ClientOpcodeMessage::CMSG_CHAR_ENUM));
            ServerOpcodeMessage::from(SMSG_CHAR_ENUM {
                characters: vec![Character {
                    guid: Guid::new(8),
                    name: "Transferfixture".to_owned(),
                    race: Race::Human,
                    class: Class::Warrior,
                    gender: Gender::Male,
                    ..Character::default()
                }],
            })
            .tokio_write_encrypted_server(&mut stream, crypto.encrypter())
            .await?;
            let request =
                ClientOpcodeMessage::tokio_read_encrypted(&mut stream, crypto.decrypter()).await?;
            assert!(matches!(request, ClientOpcodeMessage::CMSG_PLAYER_LOGIN(_)));
            write_packet(&mut stream, &mut crypto, 0x236, &location_body(0, 12.0)).await?;
            serve_exchanges(stream, crypto, receiver).await
        });
        let server = Self { commands, task };
        let stream = TcpStream::connect(address).await?;
        let mut session = match WorldConnection::authenticate(
            stream,
            identity,
            &realm,
            WorldAddonManifest::empty(),
        )
        .await?
        {
            WorldAuthProgress::Authenticated(session) => session,
            WorldAuthProgress::Queued(_) => {
                return Err("fixture session unexpectedly queued".into());
            }
        };
        session.request_character_directory().await?;
        let packet = session.receive_packet().await?;
        let directory = packet
            .character_directory()?
            .ok_or("missing fixture character directory")?;
        let character = directory
            .entries()
            .first()
            .ok_or("empty fixture character directory")?;
        let world = match session.login_character(character).await?.advance().await? {
            CharacterLoginProgress::Entered(world) => world,
            _ => return Err("fixture character did not enter world".into()),
        };
        Ok((server, world))
    }

    /// Publishes a batch without assuming any main-thread scheduling order.
    pub async fn exchange(
        &self,
        packets: Vec<(u16, Vec<u8>)>,
        response_count: usize,
    ) -> Result<oneshot::Receiver<Result<Vec<ClientOpcodeMessage>, TestError>>, TestError> {
        let (complete, receiver) = oneshot::channel();
        self.commands
            .send(Exchange {
                packets,
                response_count,
                complete: ExchangeReply::Decoded(complete),
            })
            .await?;
        Ok(receiver)
    }

    /// Captures complete native client bodies after decrypting only the header.
    // Shared by independent integration-test binaries; not every binary needs raw replies.
    #[allow(dead_code)]
    pub async fn exchange_raw(
        &self,
        packets: Vec<(u16, Vec<u8>)>,
        response_count: usize,
    ) -> Result<oneshot::Receiver<Result<Vec<RawClientPacket>, TestError>>, TestError> {
        let (complete, receiver) = oneshot::channel();
        self.commands
            .send(Exchange {
                packets,
                response_count,
                complete: ExchangeReply::Raw(complete),
            })
            .await?;
        Ok(receiver)
    }
}

impl Drop for WorldServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Writes map/XYZ/orientation fields exactly as the native handlers consume them.
pub fn location_body(map_id: u32, x: f32) -> Vec<u8> {
    let mut body = map_id.to_le_bytes().to_vec();
    for value in [x, -4.0, 100.0, 0.75] {
        body.extend_from_slice(&value.to_le_bytes());
    }
    body
}

/// Processes each publish/response exchange without cancelling encrypted reads.
async fn serve_exchanges(
    mut stream: TcpStream,
    mut crypto: ServerCrypto,
    mut receiver: mpsc::Receiver<Exchange>,
) -> Result<(), TestError> {
    while let Some(exchange) = receiver.recv().await {
        for (opcode, body) in exchange.packets {
            write_packet(&mut stream, &mut crypto, opcode, &body).await?;
        }
        match exchange.complete {
            ExchangeReply::Decoded(complete) => {
                let mut responses = Vec::new();
                for _ in 0..exchange.response_count {
                    responses.push(
                        ClientOpcodeMessage::tokio_read_encrypted(&mut stream, crypto.decrypter())
                            .await?,
                    );
                }
                let _receiver_closed = complete.send(Ok(responses));
            }
            ExchangeReply::Raw(complete) => {
                let mut responses = Vec::new();
                for _ in 0..exchange.response_count {
                    let mut header = [0_u8; 6];
                    stream.read_exact(&mut header).await?;
                    let header = crypto.decrypter().decrypt_client_header(header);
                    let size = header
                        .size
                        .checked_sub(4)
                        .ok_or("invalid client frame size")?;
                    let mut body = vec![0_u8; usize::from(size)];
                    stream.read_exact(&mut body).await?;
                    responses.push((header.opcode, body));
                }
                let _receiver_closed = complete.send(Ok(responses));
            }
        }
    }
    Ok(())
}

/// Performs the real header-crypto proof exchange for the synthetic identity.
pub(crate) async fn authenticate(
    stream: &mut TcpStream,
    session_key: [u8; 40],
) -> Result<ServerCrypto, TestError> {
    let seed = ProofSeed::new();
    ServerOpcodeMessage::from(SMSG_AUTH_CHALLENGE {
        unknown1: 1,
        server_seed: seed.seed(),
        seed: [0xC3; 32],
    })
    .tokio_write_unencrypted_server(&mut *stream)
    .await?;
    let ClientOpcodeMessage::CMSG_AUTH_SESSION(auth) =
        ClientOpcodeMessage::tokio_read_unencrypted(&mut *stream).await?
    else {
        return Err("missing fixture world authentication proof".into());
    };
    let mut crypto = seed.into_server_header_crypto(
        &NormalizedString::new("testaccount")?,
        session_key,
        auth.client_proof,
        auth.client_seed,
    )?;
    ServerOpcodeMessage::from(SMSG_AUTH_RESPONSE::AuthOk {
        billing_flags: BillingPlanFlags::empty(),
        billing_rested: 0,
        billing_time: 0,
        expansion: Expansion::WrathOfTheLichKing,
    })
    .tokio_write_encrypted_server(&mut *stream, crypto.encrypter())
    .await?;
    Ok(crypto)
}

/// Writes one complete encrypted server frame.
async fn write_packet(
    stream: &mut TcpStream,
    crypto: &mut ServerCrypto,
    opcode: u16,
    body: &[u8],
) -> Result<(), TestError> {
    let header = crypto
        .encrypter()
        .encrypt_server_header(u32::try_from(body.len())? + 2, opcode)
        .to_vec();
    stream.write_all(&header).await?;
    stream.write_all(body).await?;
    Ok(())
}
