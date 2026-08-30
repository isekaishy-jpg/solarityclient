//! Persistent active-world packet pump and main-thread ECS dispatch.

use std::collections::VecDeque;
use std::time::Duration;

use solarity_ecs::ActiveWorld;
use solarity_network::{
    InWorldSession, WorldLivenessPacketError, WorldPacketReader, WorldPacketWriter,
    WorldServerPacket, WorldSessionError, WorldTimePacketError,
};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio::runtime::Handle;
use tokio::sync::mpsc::{self, Receiver, error::TryRecvError};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::application::gameplay_session::{
    GameplaySession, GameplayUpdateError, apply_object_updates,
};
use crate::application::world_coordinator::RuntimeWorldEntry;
use crate::time::RealmClock;

const PACKET_CHANNEL_CAPACITY: usize = 256;
const LIVENESS_CHANNEL_CAPACITY: usize = 32;
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
    /// An object-update packet was malformed.
    #[error(transparent)]
    ObjectUpdate(#[from] solarity_network::ObjectUpdateError),
    /// A realm clock packet was malformed.
    #[error(transparent)]
    WorldTime(#[from] WorldTimePacketError),
    /// The network task ended without publishing its terminal result.
    #[error("active-world network task ended unexpectedly")]
    TaskEnded,
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
    unhandled_packets: VecDeque<WorldServerPacket>,
}

impl RuntimeGameplayCoordinator {
    /// Creates an empty gameplay boundary.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            active: None,
            world: None,
            realm_clock: None,
            unhandled_packets: VecDeque::new(),
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
        entry: RuntimeWorldEntry,
    ) -> Result<(), RuntimeGameplayError> {
        if self.active.is_some() || self.world.is_some() {
            return Err(RuntimeGameplayError::AlreadyActive);
        }
        let (network, setup_packets) = entry.into_parts();
        let mut gameplay = GameplaySession::enter(network);
        let mut retained = VecDeque::new();
        let mut realm_clock = None;
        for packet in setup_packets {
            dispatch_setup_packet(&mut gameplay, packet, &mut realm_clock, &mut retained)?;
        }
        let (network, world) = gameplay.into_parts();
        let (sender, receiver) = mpsc::channel(PACKET_CHANNEL_CAPACITY);
        let task = runtime.spawn(pump_world_packets(network, sender));
        self.active = Some(ActiveGameplayNetwork { receiver, task });
        self.world = Some(world);
        self.realm_clock = realm_clock;
        self.unhandled_packets = retained;
        Ok(())
    }

    /// Drains all packets admitted since the previous main-thread frame.
    ///
    /// # Errors
    ///
    /// Returns a network, decode, retention, or ECS update failure.
    pub fn service(&mut self) -> Result<usize, RuntimeGameplayError> {
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
                    match dispatch_world_packet(
                        world,
                        packet,
                        &mut self.realm_clock,
                        &mut self.unhandled_packets,
                    ) {
                        Ok(true) => applied += 1,
                        Ok(false) => {}
                        Err(error) => {
                            active.task.abort();
                            self.world = None;
                            self.realm_clock = None;
                            self.unhandled_packets.clear();
                            return Err(error);
                        }
                    }
                }
                Ok(Err(error)) => {
                    active.task.abort();
                    self.world = None;
                    self.realm_clock = None;
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
                    self.unhandled_packets.clear();
                    return Err(RuntimeGameplayError::TaskEnded);
                }
            }
        }
        Ok(applied)
    }

    /// Returns the authoritative active ECS world.
    #[must_use]
    pub const fn world(&self) -> Option<&ActiveWorld> {
        self.world.as_ref()
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
        self.unhandled_packets.clear();
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
    task: JoinHandle<()>,
}

async fn pump_world_packets(
    session: InWorldSession<TcpStream>,
    sender: mpsc::Sender<Result<WorldServerPacket, RuntimeGameplayError>>,
) {
    let (mut reader, mut writer) = session.split();
    let (liveness_sender, liveness_receiver) = mpsc::channel(LIVENESS_CHANNEL_CAPACITY);
    let result = tokio::select! {
        result = receive_world_packets(&mut reader, &sender, liveness_sender) => result,
        result = service_world_liveness(&mut writer, liveness_receiver) => result,
    };
    if let Err(error) = result {
        let _send_result = sender.send(Err(error)).await;
    }
}

async fn receive_world_packets<R>(
    reader: &mut WorldPacketReader<R>,
    sender: &mpsc::Sender<Result<WorldServerPacket, RuntimeGameplayError>>,
    liveness_sender: mpsc::Sender<WorldLivenessEvent>,
) -> Result<(), RuntimeGameplayError>
where
    R: AsyncRead + Unpin + Send,
{
    loop {
        let packet = reader.receive_packet().await?;
        if let Some(sequence) = packet.pong_sequence()? {
            if liveness_sender
                .send(WorldLivenessEvent::Pong(sequence))
                .await
                .is_err()
            {
                return Ok(());
            }
            continue;
        }
        if let Some(counter) = packet.time_sync_counter()? {
            if liveness_sender
                .send(WorldLivenessEvent::TimeSync(counter))
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

async fn service_world_liveness<W>(
    writer: &mut WorldPacketWriter<W>,
    mut receiver: Receiver<WorldLivenessEvent>,
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
                    WorldLivenessEvent::Pong(received) => {
                        if let Some((expected, sent_at)) = pending_ping
                            && received == expected
                        {
                            round_time = duration_millis_u32(sent_at.elapsed());
                            pending_ping = None;
                        }
                    }
                    WorldLivenessEvent::TimeSync(counter) => {
                        writer
                            .send_time_sync_response(
                                counter,
                                duration_millis_u32(process_start.elapsed()),
                            )
                            .await?;
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
enum WorldLivenessEvent {
    Pong(u32),
    TimeSync(u32),
}

fn dispatch_setup_packet<S>(
    gameplay: &mut GameplaySession<S>,
    packet: WorldServerPacket,
    realm_clock: &mut Option<RealmClock>,
    unhandled: &mut VecDeque<WorldServerPacket>,
) -> Result<(), RuntimeGameplayError> {
    if let Some(source) = packet.world_time_speed()? {
        *realm_clock = Some(RealmClock::new(source));
        return Ok(());
    }
    if let Some(updates) = packet.object_updates()? {
        gameplay.apply_object_updates(&updates)?;
        return Ok(());
    }
    retain_unhandled(unhandled, packet)
}

fn dispatch_world_packet(
    world: &mut ActiveWorld,
    packet: WorldServerPacket,
    realm_clock: &mut Option<RealmClock>,
    unhandled: &mut VecDeque<WorldServerPacket>,
) -> Result<bool, RuntimeGameplayError> {
    if let Some(source) = packet.world_time_speed()? {
        *realm_clock = Some(RealmClock::new(source));
        return Ok(false);
    }
    if let Some(updates) = packet.object_updates()? {
        apply_object_updates(world, &updates)?;
        return Ok(true);
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
    unhandled.push_back(packet);
    Ok(())
}
