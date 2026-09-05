//! Persistent active-world packet pump and main-thread ECS dispatch.

use std::collections::VecDeque;
use std::time::Duration;

use glam::Vec3;
use solarity_ecs::{ActiveWorld, WorldBootstrap, WorldMapId, WorldStateError};
use solarity_network::{
    InWorldSession, WorldActionButtonPacketError, WorldActionButtons, WorldLivenessPacketError,
    WorldLocation, WorldMovementMessage, WorldPacketReader, WorldPacketWriter, WorldServerPacket,
    WorldSessionError, WorldTimePacketError, WorldTransfer,
};
use solarity_systems::{WorldEntryGroundContact, WorldEntryGroundContactError};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio::runtime::Handle;
use tokio::sync::mpsc::{
    self, Receiver,
    error::{TryRecvError, TrySendError},
};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::application::gameplay_session::{
    GameplaySession, GameplayUpdateError, apply_object_updates,
};
use crate::time::RealmClock;

const PACKET_CHANNEL_CAPACITY: usize = 256;
const WRITER_CHANNEL_CAPACITY: usize = 32;
const MAX_RETAINED_UNHANDLED_PACKETS: usize = 4_096;
const PING_INTERVAL: Duration = Duration::from_secs(30);

/// A failure in active-world network or ECS ownership.
#[derive(Debug, Error)]
pub enum RuntimeGameplayError {
    /// A second active-world session was supplied before disconnect.
    #[error("an active gameplay session is already owned")]
    AlreadyActive,
    /// Encrypted active-world packet I/O failed.
    #[error(transparent)]
    Session(#[from] WorldSessionError),
    /// A world liveness packet had an invalid body.
    #[error(transparent)]
    Liveness(#[from] WorldLivenessPacketError),
    /// An authoritative object update could not enter ECS state.
    #[error(transparent)]
    Update(#[from] GameplayUpdateError),
    /// The active world lost a required controlled-player invariant.
    #[error(transparent)]
    World(#[from] WorldStateError),
    /// The first movement-owned resident ground contact was invalid.
    #[error(transparent)]
    GroundContact(#[from] WorldEntryGroundContactError),
    /// An object-update packet was malformed.
    #[error(transparent)]
    ObjectUpdate(#[from] solarity_network::ObjectUpdateError),
    /// A realm clock packet was malformed.
    #[error(transparent)]
    WorldTime(#[from] WorldTimePacketError),
    /// An authoritative action-button image was malformed.
    #[error(transparent)]
    ActionButtons(#[from] WorldActionButtonPacketError),
    /// The network task ended without publishing its terminal result.
    #[error("active-world network task ended unexpectedly")]
    TaskEnded,
    /// A destination replacement lost the selected character's durable identity.
    #[error("world replacement has no selected player identity")]
    MissingPlayerIdentity,
    /// Unsupported packets exceeded the explicit retention boundary.
    #[error("active world retained more than {maximum} unhandled packets")]
    UnhandledPacketLimit {
        /// Maximum packets retained for later subsystem dispatch.
        maximum: usize,
    },
}

/// Main-thread ECS owner paired with one cancellable async network pump.
pub struct RuntimeGameplayCoordinator {
    active: Option<ActiveGameplayNetwork>,
    world: Option<ActiveWorld>,
    realm_clock: Option<RealmClock>,
    action_buttons: Option<WorldActionButtons>,
    unhandled_packets: VecDeque<WorldServerPacket>,
    world_entry_grounded: bool,
    /// Packet dispatch yields to the composition root at each transfer packet.
    transfer: Option<WorldTransfer>,
}

impl RuntimeGameplayCoordinator {
    /// Creates an empty gameplay boundary.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            active: None,
            world: None,
            realm_clock: None,
            action_buttons: None,
            unhandled_packets: VecDeque::new(),
            world_entry_grounded: false,
            transfer: None,
        }
    }

    /// Transfers accepted world-entry ownership into ECS and the packet task.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeGameplayError`] for duplicate ownership, malformed
    /// retained setup packets, or ECS projection failures.
    pub fn begin(
        &mut self,
        runtime: &Handle,
        network: InWorldSession<TcpStream>,
        setup_packets: Vec<WorldServerPacket>,
    ) -> Result<(), RuntimeGameplayError> {
        if self.active.is_some() || self.world.is_some() {
            return Err(RuntimeGameplayError::AlreadyActive);
        }
        let setup_packet_count = setup_packets.len();
        let mut gameplay = GameplaySession::enter(network);
        let mut retained = VecDeque::new();
        let mut realm_clock = None;
        let mut action_buttons = None;
        for packet in setup_packets {
            dispatch_setup_packet(
                &mut gameplay,
                packet,
                &mut realm_clock,
                &mut action_buttons,
                &mut retained,
            )?;
        }
        let (network, world) = gameplay.into_parts();
        let map_id = world.map_id().value();
        let (sender, receiver) = mpsc::channel(PACKET_CHANNEL_CAPACITY);
        let (commands, command_receiver) = mpsc::channel(WRITER_CHANNEL_CAPACITY);
        let task = runtime.spawn(pump_world_packets(
            network,
            sender,
            commands.clone(),
            command_receiver,
        ));
        self.active = Some(ActiveGameplayNetwork {
            receiver,
            commands,
            task,
        });
        self.world = Some(world);
        self.realm_clock = realm_clock;
        self.action_buttons = action_buttons;
        self.unhandled_packets = retained;
        self.world_entry_grounded = false;
        tracing::info!(
            map_id,
            setup_packet_count,
            realm_clock_ready = self.realm_clock.is_some(),
            retained_packet_count = self.unhandled_packets.len(),
            "active gameplay ownership initialized"
        );
        Ok(())
    }

    /// Drains all packets admitted since the previous main-thread frame.
    ///
    /// # Errors
    ///
    /// Returns a network, decode, retention, or ECS update failure.
    pub fn service(&mut self) -> Result<usize, RuntimeGameplayError> {
        if self.transfer.is_some() {
            return Ok(0);
        }
        let Some(mut active) = self.active.take() else {
            return Ok(0);
        };
        let Some(world) = self.world.as_mut() else {
            active.task.abort();
            return Ok(0);
        };
        let mut applied = 0;
        loop {
            match active.receiver.try_recv() {
                Ok(Ok(packet)) => {
                    match packet.world_transfer() {
                        Ok(Some(transfer)) => {
                            self.transfer = Some(transfer);
                            self.active = Some(active);
                            break;
                        }
                        Ok(None) => {}
                        Err(error) => {
                            // 0x00403D10 diagnoses a malformed destination and
                            // returns without scheduling or disconnecting.
                            tracing::warn!(%error, "discarded malformed world-transfer packet");
                            continue;
                        }
                    }
                    match dispatch_world_packet(
                        world,
                        packet,
                        &mut self.realm_clock,
                        &mut self.action_buttons,
                        &mut self.unhandled_packets,
                    ) {
                        Ok(true) => applied += 1,
                        Ok(false) => {}
                        Err(error) => {
                            active.task.abort();
                            self.world = None;
                            self.realm_clock = None;
                            self.action_buttons = None;
                            self.unhandled_packets.clear();
                            return Err(error);
                        }
                    }
                }
                Ok(Err(error)) => {
                    active.task.abort();
                    self.world = None;
                    self.realm_clock = None;
                    self.action_buttons = None;
                    self.unhandled_packets.clear();
                    return Err(error);
                }
                Err(TryRecvError::Empty) => {
                    self.active = Some(active);
                    break;
                }
                Err(TryRecvError::Disconnected) => {
                    self.world = None;
                    self.realm_clock = None;
                    self.action_buttons = None;
                    self.unhandled_packets.clear();
                    return Err(RuntimeGameplayError::TaskEnded);
                }
            }
        }
        Ok(applied)
    }

    /// Takes the transfer packet that paused main-thread dispatch.
    pub fn take_world_transfer(&mut self) -> Option<WorldTransfer> {
        self.transfer.take()
    }

    /// Replaces replicated ECS ownership while preserving the live connection.
    ///
    /// Stock 0x00403B70 destroys the old object manager before loading the
    /// destination. The selected identity seeds only a placeholder; all old
    /// replicated fields, remote objects, movement, and player effects expire.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeGameplayError`] if the selected player identity is absent.
    pub fn replace_world(&mut self, location: WorldLocation) -> Result<(), RuntimeGameplayError> {
        let world = self
            .world
            .as_ref()
            .ok_or(RuntimeGameplayError::MissingPlayerIdentity)?;
        let identity = world
            .local_player_identity()
            .ok_or(RuntimeGameplayError::MissingPlayerIdentity)?;
        let bootstrap = WorldBootstrap::new(
            WorldMapId::new(location.map_id()),
            world.local_player_guid()?,
            identity.name(),
            Vec3::new(location.x(), location.y(), location.z()),
            location.orientation(),
        );
        self.world = Some(ActiveWorld::enter_with_view(
            bootstrap,
            world.local_player_view()?,
        ));
        self.world_entry_grounded = false;
        Ok(())
    }

    /// Admits one map-completion ACK to the sole encrypted writer.
    ///
    /// `false` means bounded queue backpressure; the caller retains its map
    /// completion obligation until admission succeeds on a later frame.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeGameplayError::TaskEnded`] if the active writer is gone.
    pub fn acknowledge_world_transfer(&self) -> Result<bool, RuntimeGameplayError> {
        let active = self
            .active
            .as_ref()
            .ok_or(RuntimeGameplayError::TaskEnded)?;
        match active
            .commands
            .try_send(WorldWriterCommand::WorldportAcknowledgement)
        {
            Ok(()) => Ok(true),
            Err(TrySendError::Full(_)) => Ok(false),
            Err(TrySendError::Closed(_)) => Err(RuntimeGameplayError::TaskEnded),
        }
    }

    /// Admits one frozen movement event to the sole encrypted writer.
    ///
    /// A `false` result is bounded queue backpressure: the movement owner must
    /// retain this exact message and its ordering obligation until admitted.
    /// The writer never reads a later ECS transform to reconstruct the event.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeGameplayError::TaskEnded`] if the active writer is gone.
    pub fn send_movement(
        &self,
        message: WorldMovementMessage,
    ) -> Result<bool, RuntimeGameplayError> {
        let active = self
            .active
            .as_ref()
            .ok_or(RuntimeGameplayError::TaskEnded)?;
        match active
            .commands
            .try_send(WorldWriterCommand::Movement(message))
        {
            Ok(()) => Ok(true),
            Err(TrySendError::Full(_)) => Ok(false),
            Err(TrySendError::Closed(_)) => Err(RuntimeGameplayError::TaskEnded),
        }
    }

    /// Returns the authoritative active ECS world.
    #[must_use]
    pub const fn world(&self) -> Option<&ActiveWorld> {
        self.world.as_ref()
    }

    /// Returns whether initial resident-world support still needs resolution.
    #[must_use]
    pub const fn world_entry_ground_contact_pending(&self) -> bool {
        self.world.is_some() && !self.world_entry_grounded
    }

    /// Applies the movement-owned first terrain contact to the local player.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeGameplayError`] if ECS invariants or the resolved
    /// surface are invalid.
    pub fn apply_world_entry_ground_contact(
        &mut self,
        surface_height: f32,
    ) -> Result<(), RuntimeGameplayError> {
        let Some(world) = self.world.as_mut() else {
            return Ok(());
        };
        if self.world_entry_grounded {
            return Ok(());
        }
        let guid = world.local_player_guid()?;
        let transform = world.local_player_transform()?;
        let contact = WorldEntryGroundContact::resolve(transform, surface_height)?;
        world.update_transform(guid, contact.transform(transform.orientation()))?;
        self.world_entry_grounded = true;
        Ok(())
    }

    /// Returns the running authoritative realm clock, when received.
    #[must_use]
    pub const fn realm_clock(&self) -> Option<&RealmClock> {
        self.realm_clock.as_ref()
    }

    /// Returns the current stock DBC half-minute, when server time is known.
    #[must_use]
    pub fn realm_half_minutes(&self) -> Option<u32> {
        self.realm_clock.as_ref().map(RealmClock::half_minutes)
    }

    /// Returns the latest complete server-authored action-button image.
    #[must_use]
    pub const fn action_buttons(&self) -> Option<&WorldActionButtons> {
        self.action_buttons.as_ref()
    }

    /// Returns unsupported packets retained for their future owning subsystem.
    #[must_use]
    pub fn unhandled_packets(&self) -> &VecDeque<WorldServerPacket> {
        &self.unhandled_packets
    }

    /// Aborts packet I/O and drops active ECS state.
    pub fn disconnect(&mut self) {
        if let Some(active) = self.active.take() {
            active.task.abort();
        }
        self.world = None;
        self.realm_clock = None;
        self.action_buttons = None;
        self.unhandled_packets.clear();
        self.world_entry_grounded = false;
        self.transfer = None;
    }
}

impl Default for RuntimeGameplayCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for RuntimeGameplayCoordinator {
    fn drop(&mut self) {
        self.disconnect();
    }
}

struct ActiveGameplayNetwork {
    receiver: Receiver<Result<WorldServerPacket, RuntimeGameplayError>>,
    commands: mpsc::Sender<WorldWriterCommand>,
    task: JoinHandle<()>,
}

async fn pump_world_packets(
    session: InWorldSession<TcpStream>,
    sender: mpsc::Sender<Result<WorldServerPacket, RuntimeGameplayError>>,
    commands: mpsc::Sender<WorldWriterCommand>,
    command_receiver: Receiver<WorldWriterCommand>,
) {
    let (mut reader, mut writer) = session.split();
    let result = tokio::select! {
        result = receive_world_packets(&mut reader, &sender, commands) => result,
        result = service_world_writer(&mut writer, command_receiver) => result,
    };
    if let Err(error) = result {
        let _send_result = sender.send(Err(error)).await;
    }
}

async fn receive_world_packets<R>(
    reader: &mut WorldPacketReader<R>,
    sender: &mpsc::Sender<Result<WorldServerPacket, RuntimeGameplayError>>,
    liveness_sender: mpsc::Sender<WorldWriterCommand>,
) -> Result<(), RuntimeGameplayError>
where
    R: AsyncRead + Unpin + Send,
{
    loop {
        let packet = reader.receive_packet().await?;
        if let Some(sequence) = packet.pong_sequence()? {
            if liveness_sender
                .send(WorldWriterCommand::Pong(sequence))
                .await
                .is_err()
            {
                return Ok(());
            }
            continue;
        }
        if let Some(counter) = packet.time_sync_counter()? {
            if liveness_sender
                .send(WorldWriterCommand::TimeSync(counter))
                .await
                .is_err()
            {
                return Ok(());
            }
            continue;
        }
        if sender.send(Ok(packet)).await.is_err() {
            return Ok(());
        }
    }
}

/// Serializes application commands and liveness without cancelling partial writes.
async fn service_world_writer<W>(
    writer: &mut WorldPacketWriter<W>,
    mut receiver: Receiver<WorldWriterCommand>,
) -> Result<(), RuntimeGameplayError>
where
    W: AsyncWrite + Unpin + Send,
{
    let process_start = Instant::now();
    let mut ping_interval = tokio::time::interval_at(process_start + PING_INTERVAL, PING_INTERVAL);
    let mut sequence = 0_u32;
    let mut round_time = 0_u32;
    let mut pending_ping = None;
    loop {
        tokio::select! {
            _instant = ping_interval.tick() => {
                sequence = sequence.wrapping_add(1);
                writer.send_ping(sequence, round_time).await?;
                pending_ping = Some((sequence, Instant::now()));
            }
            event = receiver.recv() => {
                let Some(event) = event else {
                    return Ok(());
                };
                match event {
                    WorldWriterCommand::Pong(received) => {
                        if let Some((expected, sent_at)) = pending_ping
                            && received == expected
                        {
                            round_time = duration_millis_u32(sent_at.elapsed());
                            pending_ping = None;
                        }
                    }
                    WorldWriterCommand::TimeSync(counter) => {
                        writer
                            .send_time_sync_response(
                                counter,
                                duration_millis_u32(process_start.elapsed()),
                            )
                            .await?;
                    }
                    WorldWriterCommand::WorldportAcknowledgement => {
                        writer.send_worldport_acknowledgement().await?;
                    }
                    WorldWriterCommand::Movement(message) => {
                        writer.send_movement(&message).await?;
                    }
                }
            }
        }
    }
}

fn duration_millis_u32(duration: Duration) -> u32 {
    // Stock's millisecond clock is a wrapping 32-bit counter. Preserve that
    // wire behavior even though this process and `Duration` are 64-bit.
    duration.as_millis() as u32
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// All writes share one queue and one continuously owned cipher half.
enum WorldWriterCommand {
    Pong(u32),
    TimeSync(u32),
    WorldportAcknowledgement,
    Movement(WorldMovementMessage),
}

fn dispatch_setup_packet<S>(
    gameplay: &mut GameplaySession<S>,
    packet: WorldServerPacket,
    realm_clock: &mut Option<RealmClock>,
    action_buttons: &mut Option<WorldActionButtons>,
    unhandled: &mut VecDeque<WorldServerPacket>,
) -> Result<(), RuntimeGameplayError> {
    if let Some(source) = packet.world_time_speed()? {
        tracing::info!(
            hour = source.hour(),
            minute = source.minute(),
            "realm clock became authoritative"
        );
        *realm_clock = Some(RealmClock::new(source));
        return Ok(());
    }
    if let Some(updates) = packet.object_updates()? {
        gameplay.apply_object_updates(&updates)?;
        return Ok(());
    }
    if let Some(buttons) = packet.action_buttons()? {
        if buttons.slots().is_some() {
            *action_buttons = Some(buttons);
        }
        return Ok(());
    }
    retain_unhandled(unhandled, packet)
}

fn dispatch_world_packet(
    world: &mut ActiveWorld,
    packet: WorldServerPacket,
    realm_clock: &mut Option<RealmClock>,
    action_buttons: &mut Option<WorldActionButtons>,
    unhandled: &mut VecDeque<WorldServerPacket>,
) -> Result<bool, RuntimeGameplayError> {
    if let Some(source) = packet.world_time_speed()? {
        tracing::info!(
            hour = source.hour(),
            minute = source.minute(),
            "realm clock became authoritative"
        );
        *realm_clock = Some(RealmClock::new(source));
        return Ok(false);
    }
    if let Some(updates) = packet.object_updates()? {
        apply_object_updates(world, &updates)?;
        return Ok(true);
    }
    if let Some(buttons) = packet.action_buttons()? {
        if buttons.slots().is_some() {
            *action_buttons = Some(buttons);
        }
        return Ok(false);
    }
    retain_unhandled(unhandled, packet)?;
    Ok(false)
}

fn retain_unhandled(
    unhandled: &mut VecDeque<WorldServerPacket>,
    packet: WorldServerPacket,
) -> Result<(), RuntimeGameplayError> {
    if unhandled.len() == MAX_RETAINED_UNHANDLED_PACKETS {
        return Err(RuntimeGameplayError::UnhandledPacketLimit {
            maximum: MAX_RETAINED_UNHANDLED_PACKETS,
        });
    }
    tracing::debug!(
        opcode = format_args!("{:#06X}", packet.opcode()),
        name = packet.name().unwrap_or("unknown"),
        payload_bytes = packet.payload().len(),
        "retaining unhandled active-world packet"
    );
    unhandled.push_back(packet);
    Ok(())
}
