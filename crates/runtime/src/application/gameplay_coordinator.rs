//! Persistent active-world packet pump and main-thread ECS dispatch.

mod game_object_cache;

#[cfg(test)]
#[path = "../../tests/application/game_object_templates.rs"]
mod game_object_template_tests;

pub(in crate::application) use game_object_cache::{
    GameObjectTemplateBinding, GameObjectTemplateCache,
};

#[cfg(test)]
#[path = "../../tests/application/world_entry_movement.rs"]
mod movement_entry_tests;

#[cfg(test)]
#[path = "../../tests/application/remote_movement.rs"]
mod remote_movement_tests;

use std::collections::VecDeque;
use std::time::Duration;

use glam::Vec3;
use solarity_ecs::{ActiveWorld, WorldBootstrap, WorldMapId, WorldStateError};
use solarity_network::{
    InWorldSession, WorldActionButtonPacketError, WorldActionButtons, WorldLivenessPacketError,
    WorldLocation, WorldMovementMessage, WorldPacketReader, WorldPacketWriter, WorldServerPacket,
    WorldSessionError, WorldTimePacketError, WorldTransfer,
};
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

use super::player_control::{PlayerControlEvent, RuntimePlayerControl};
use crate::application::game_object_behavior::GameObjectNotification;
use crate::application::gameplay_session::{
    GameplaySession, GameplayUpdateError, apply_object_updates_with,
};
use crate::time::RealmClock;

const PACKET_CHANNEL_CAPACITY: usize = 256;
const WRITER_CHANNEL_CAPACITY: usize = 32;
const MAX_RETAINED_UNHANDLED_PACKETS: usize = 4_096;
const PING_INTERVAL: Duration = Duration::from_secs(30);

/// A failure in active-world network or ECS ownership.
#[derive(Debug, Error)]
pub enum RuntimeGameplayError {
    /// A retained game-object animation rejected a field callback.
    #[error(transparent)]
    ObjectAnimation(#[from] crate::application::terrain_frame::RuntimeTerrainFrameError),
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
    /// An object-update packet was malformed.
    #[error(transparent)]
    ObjectUpdate(#[from] solarity_network::ObjectUpdateError),
    /// An incoming movement command was malformed.
    #[error(transparent)]
    MovementPacket(#[from] solarity_network::MovementPacketError),
    /// A game-object template response was malformed.
    #[error(transparent)]
    GameObjectQuery(#[from] solarity_network::GameObjectQueryPacketError),
    /// A realm clock packet was malformed.
    #[error(transparent)]
    WorldTime(#[from] WorldTimePacketError),
    /// An authoritative world-state packet was malformed.
    #[error(transparent)]
    WorldStatePacket(#[from] solarity_network::WorldStatePacketError),
    /// An authoritative action-button image was malformed.
    #[error(transparent)]
    ActionButtons(#[from] WorldActionButtonPacketError),
    /// A native unit-control or local stand-state packet was malformed.
    #[error(transparent)]
    PlayerControl(#[from] solarity_network::WorldPlayerControlPacketError),
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

type GameObjectObserver<'a> = dyn FnMut(
        &ActiveWorld,
        solarity_ecs::WorldObjectIdentity,
        GameObjectNotification,
    ) -> Result<(), RuntimeGameplayError>
    + 'a;

/// Main-thread ECS owner paired with one cancellable async network pump.
pub struct RuntimeGameplayCoordinator {
    game_object_templates: GameObjectTemplateCache,
    active: Option<ActiveGameplayNetwork>,
    world: Option<ActiveWorld>,
    realm_clock: Option<RealmClock>,
    action_buttons: Option<WorldActionButtons>,
    player_control: Option<RuntimePlayerControl>,
    unhandled_packets: VecDeque<WorldServerPacket>,
    /// Packet dispatch yields to the composition root at each transfer packet.
    transfer: Option<WorldTransfer>,
    /// Native Unit_C short-stop CVar, refreshed before packet dispatch.
    path_distance_tolerance: f32,
}

impl RuntimeGameplayCoordinator {
    #[cfg(test)]
    pub(super) fn with_test_world(world: ActiveWorld) -> Self {
        let mut coordinator = Self::new();
        coordinator.world = Some(world);
        coordinator
    }

    /// Creates an empty gameplay boundary.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            game_object_templates: GameObjectTemplateCache::new(),
            active: None,
            world: None,
            realm_clock: None,
            action_buttons: None,
            player_control: None,
            unhandled_packets: VecDeque::new(),
            transfer: None,
            path_distance_tolerance: 1.0,
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
        self.begin_with_game_objects(runtime, network, setup_packets, &mut |_, _, _| Ok(()))
    }

    pub(in crate::application) fn begin_with_game_objects(
        &mut self,
        runtime: &Handle,
        network: InWorldSession<TcpStream>,
        setup_packets: Vec<WorldServerPacket>,
        notify: &mut GameObjectObserver<'_>,
    ) -> Result<(), RuntimeGameplayError> {
        if self.active.is_some() || self.world.is_some() {
            return Err(RuntimeGameplayError::AlreadyActive);
        }
        let setup_packet_count = setup_packets.len();
        let mut gameplay = GameplaySession::enter(network);
        let player_identity = gameplay
            .world()
            .object_identity(gameplay.world().local_player_guid()?)
            .ok_or(RuntimeGameplayError::MissingPlayerIdentity)?;
        let mut player_control = RuntimePlayerControl::new(player_identity);
        let mut retained = VecDeque::new();
        let mut realm_clock = None;
        let mut action_buttons = None;
        let mut game_object_templates = GameObjectTemplateCache::new();
        for packet in setup_packets {
            if let Some(response) = packet.game_object_query()? {
                game_object_templates.receive(response);
                continue;
            }
            dispatch_setup_packet(
                &mut gameplay,
                packet,
                &mut realm_clock,
                &mut action_buttons,
                &mut player_control,
                &mut retained,
                self.path_distance_tolerance,
                notify,
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
        self.game_object_templates = game_object_templates;
        self.realm_clock = realm_clock;
        self.action_buttons = action_buttons;
        self.player_control = Some(player_control);
        self.unhandled_packets = retained;
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
        self.service_with_game_objects(&mut |_, _, _| Ok(()))
    }

    pub(in crate::application) fn service_with_game_objects(
        &mut self,
        notify: &mut GameObjectObserver<'_>,
    ) -> Result<usize, RuntimeGameplayError> {
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
                    let result = packet
                        .game_object_query()
                        .map_err(RuntimeGameplayError::from)
                        .and_then(|response| {
                            if let Some(response) = response {
                                self.game_object_templates.receive(response);
                                return Ok(false);
                            }
                            dispatch_world_packet(
                                world,
                                packet,
                                &mut self.realm_clock,
                                &mut self.action_buttons,
                                self.player_control
                                    .as_mut()
                                    .ok_or(RuntimeGameplayError::MissingPlayerIdentity)?,
                                &mut self.unhandled_packets,
                                self.path_distance_tolerance,
                                notify,
                                crate::platform::client_milliseconds(),
                            )
                        });
                    match result {
                        Ok(true) => applied += 1,
                        Ok(false) => {}
                        Err(error) => {
                            active.task.abort();
                            self.world = None;
                            self.realm_clock = None;
                            self.action_buttons = None;
                            self.unhandled_packets.clear();
                            self.game_object_templates.clear();
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
                    self.game_object_templates.clear();
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
                    self.game_object_templates.clear();
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

    /// Gives model admission the session-owned cache without exposing network state.
    pub(in crate::application) fn game_object_templates_mut(
        &mut self,
    ) -> &mut GameObjectTemplateCache {
        &mut self.game_object_templates
    }

    /// Admits pending template requests without dropping them under backpressure.
    pub(in crate::application) fn send_game_object_queries(
        &mut self,
    ) -> Result<(), RuntimeGameplayError> {
        let Some(active) = self.active.as_ref() else {
            return Ok(());
        };
        while let Some((entry, guid)) = self.game_object_templates.pending_request() {
            match active
                .commands
                .try_send(WorldWriterCommand::GameObjectQuery { entry, guid })
            {
                Ok(()) => self.game_object_templates.request_admitted(),
                Err(TrySendError::Full(_)) => break,
                Err(TrySendError::Closed(_)) => return Err(RuntimeGameplayError::TaskEnded),
            }
        }
        Ok(())
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
        let mut destination = ActiveWorld::enter_with_view(bootstrap, world.local_player_view()?);
        // World-state fields live in the session-global BE8F58 hash, outside
        // the object manager replaced during a map transfer.
        if let Some(source) = self.world.as_mut() {
            *destination.world_state_values_mut() = std::mem::take(source.world_state_values_mut());
        }
        self.world = Some(destination);
        let replacement = self
            .world
            .as_ref()
            .ok_or(RuntimeGameplayError::MissingPlayerIdentity)?;
        self.player_control = Some(RuntimePlayerControl::new(
            replacement
                .object_identity(replacement.local_player_guid()?)
                .ok_or(RuntimeGameplayError::MissingPlayerIdentity)?,
        ));
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

    /// Admits the native heartbeat/AreaTrigger pair to the encrypted writer.
    ///
    /// A `false` result retains both frozen packets at the caller under backpressure.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeGameplayError::TaskEnded`] when the writer is gone.
    pub fn send_area_trigger(
        &self,
        heartbeat: WorldMovementMessage,
        trigger_id: u32,
    ) -> Result<bool, RuntimeGameplayError> {
        let active = self
            .active
            .as_ref()
            .ok_or(RuntimeGameplayError::TaskEnded)?;
        match active.commands.try_send(WorldWriterCommand::AreaTrigger {
            heartbeat,
            trigger_id,
        }) {
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

    /// Refreshes native `pathDistTol` before draining the packet queue.
    pub(super) fn set_path_distance_tolerance(&mut self, tolerance: f32) {
        self.path_distance_tolerance = tolerance;
    }

    /// Returns the native selected movement subject, including an explicit zero.
    #[must_use]
    pub fn active_mover_guid(&self) -> Option<u64> {
        self.world.as_ref()?;
        self.player_control
            .as_ref()
            .map(RuntimePlayerControl::active_mover)
    }

    /// Returns the entering player's current native client-control bit.
    #[must_use]
    pub fn player_control_enabled(&self) -> Option<bool> {
        self.world.as_ref()?;
        self.player_control
            .as_ref()
            .map(RuntimePlayerControl::player_enabled)
    }

    pub(super) fn take_player_control_event(&mut self) -> Option<PlayerControlEvent> {
        self.world.as_ref()?;
        self.player_control.as_mut()?.take_event()
    }

    pub(super) fn apply_local_movement(
        &mut self,
        identity: solarity_ecs::WorldObjectIdentity,
        transform: solarity_ecs::WorldTransform,
        movement: solarity_ecs::WorldMovementState,
        stand_state: u8,
    ) -> Result<(), RuntimeGameplayError> {
        self.world
            .as_mut()
            .ok_or(RuntimeGameplayError::TaskEnded)?
            .update_local_movement(identity, transform, movement)?;
        self.world
            .as_mut()
            .ok_or(RuntimeGameplayError::TaskEnded)?
            .set_local_player_stand_state(stand_state);
        Ok(())
    }

    pub(super) fn send_player_movement(
        &self,
        output: super::player_movement::PlayerMovementOutput,
    ) -> Result<bool, RuntimeGameplayError> {
        use super::player_movement::PlayerMovementOutput as Output;
        let command = match output {
            Output::Movement(message) => return self.send_movement(message),
            Output::SkippedTime { guid, milliseconds } => {
                WorldWriterCommand::MovementTimeSkipped { guid, milliseconds }
            }
            Output::StandState(state) => WorldWriterCommand::StandState(state),
            Output::ActiveMover(guid) => WorldWriterCommand::ActiveMover(guid),
            Output::AreaTrigger {
                heartbeat,
                trigger_id,
            } => {
                return self.send_area_trigger(heartbeat, trigger_id);
            }
        };
        match self
            .active
            .as_ref()
            .ok_or(RuntimeGameplayError::TaskEnded)?
            .commands
            .try_send(command)
        {
            Ok(()) => Ok(true),
            Err(TrySendError::Full(_)) => Ok(false),
            Err(TrySendError::Closed(_)) => Err(RuntimeGameplayError::TaskEnded),
        }
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
        self.game_object_templates.clear();
        if let Some(active) = self.active.take() {
            active.task.abort();
        }
        self.world = None;
        self.player_control = None;
        self.realm_clock = None;
        self.action_buttons = None;
        self.unhandled_packets.clear();
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
                                crate::platform::client_milliseconds(),
                            )
                            .await?;
                    }
                    WorldWriterCommand::WorldportAcknowledgement => {
                        writer.send_worldport_acknowledgement().await?;
                    }
                    WorldWriterCommand::Movement(message) => {
                        writer.send_movement(&message).await?;
                    }
                    WorldWriterCommand::MovementTimeSkipped { guid, milliseconds } => {
                        writer.send_movement_time_skipped(guid, milliseconds).await?;
                    }
                    WorldWriterCommand::StandState(state) => {
                        writer.send_stand_state(state).await?;
                    }
                    WorldWriterCommand::ActiveMover(guid) => {
                        writer.send_active_mover(guid).await?;
                    }
                    WorldWriterCommand::GameObjectQuery { entry, guid } => {
                        writer.send_game_object_query(entry, guid).await?;
                    }
                    WorldWriterCommand::AreaTrigger { heartbeat, trigger_id } => {
                        writer.send_movement(&heartbeat).await?;
                        writer.send_area_trigger(trigger_id).await?;
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
    MovementTimeSkipped {
        guid: u64,
        milliseconds: u32,
    },
    StandState(u32),
    ActiveMover(u64),
    GameObjectQuery {
        entry: u32,
        guid: u64,
    },
    AreaTrigger {
        heartbeat: WorldMovementMessage,
        trigger_id: u32,
    },
}

// Each argument borrows an independently owned session service for this dispatch.
#[allow(clippy::too_many_arguments)]
fn dispatch_setup_packet<S>(
    gameplay: &mut GameplaySession<S>,
    packet: WorldServerPacket,
    realm_clock: &mut Option<RealmClock>,
    action_buttons: &mut Option<WorldActionButtons>,
    player_control: &mut RuntimePlayerControl,
    unhandled: &mut VecDeque<WorldServerPacket>,
    path_distance_tolerance: f32,
    notify: &mut GameObjectObserver<'_>,
) -> Result<(), RuntimeGameplayError> {
    let timestamp_ms = crate::platform::client_milliseconds();
    if let Some(message) = packet.remote_movement()? {
        if message
            .context
            .transport
            .is_some_and(|parent| parent.guid != 0)
        {
            return retain_unhandled(unhandled, packet);
        }
        crate::application::player_movement::remote::receive(
            gameplay.world_mut(),
            message,
            timestamp_ms,
            player_control.active_mover(),
        );
        return Ok(());
    }
    if let Some(message) = packet.monster_move()? {
        if message.transport.is_some_and(|parent| parent.guid != 0) {
            return retain_unhandled(unhandled, packet);
        }
        crate::application::player_movement::remote::receive_path(
            gameplay.world_mut(),
            message,
            timestamp_ms,
            path_distance_tolerance,
        );
        return Ok(());
    }
    if apply_world_state_packet(gameplay.world_mut(), &packet)? {
        return Ok(());
    }
    if let Some(update) = packet.client_control_update()? {
        player_control.receive(gameplay.world(), update, timestamp_ms);
        return Ok(());
    }
    if let Some(state) = packet.stand_state_update()? {
        if gameplay.world().local_player_stand_state()? != state {
            player_control.stand_state(state, timestamp_ms);
        }
        gameplay.world_mut().set_local_player_stand_state(state);
        return Ok(());
    }
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
        apply_object_updates_with(
            gameplay.world_mut(),
            &updates,
            timestamp_ms,
            &mut |world, identity, event| notify(world, identity, event),
        )?;
        player_control.synchronize(gameplay.world(), timestamp_ms);
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

// Each argument borrows an independently owned session service for this dispatch.
#[allow(clippy::too_many_arguments)]
fn dispatch_world_packet(
    world: &mut ActiveWorld,
    packet: WorldServerPacket,
    realm_clock: &mut Option<RealmClock>,
    action_buttons: &mut Option<WorldActionButtons>,
    player_control: &mut RuntimePlayerControl,
    unhandled: &mut VecDeque<WorldServerPacket>,
    path_distance_tolerance: f32,
    notify: &mut GameObjectObserver<'_>,
    timestamp_ms: u32,
) -> Result<bool, RuntimeGameplayError> {
    if let Some(message) = packet.remote_movement()? {
        if message
            .context
            .transport
            .is_some_and(|parent| parent.guid != 0)
        {
            retain_unhandled(unhandled, packet)?;
            return Ok(false);
        }
        return Ok(crate::application::player_movement::remote::receive(
            world,
            message,
            timestamp_ms,
            player_control.active_mover(),
        ));
    }
    if let Some(message) = packet.monster_move()? {
        if message.transport.is_some_and(|parent| parent.guid != 0) {
            retain_unhandled(unhandled, packet)?;
            return Ok(false);
        }
        return Ok(crate::application::player_movement::remote::receive_path(
            world,
            message,
            timestamp_ms,
            path_distance_tolerance,
        ));
    }
    if apply_world_state_packet(world, &packet)? {
        return Ok(false);
    }
    if let Some(update) = packet.client_control_update()? {
        player_control.receive(world, update, timestamp_ms);
        return Ok(false);
    }
    if let Some(state) = packet.stand_state_update()? {
        if world.local_player_stand_state()? != state {
            player_control.stand_state(state, timestamp_ms);
        }
        world.set_local_player_stand_state(state);
        return Ok(true);
    }
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
        apply_object_updates_with(
            world,
            &updates,
            timestamp_ms,
            &mut |world, identity, event| notify(world, identity, event),
        )?;
        player_control.synchronize(world, timestamp_ms);
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

fn apply_world_state_packet(
    world: &mut ActiveWorld,
    packet: &WorldServerPacket,
) -> Result<bool, RuntimeGameplayError> {
    let Some(update) = packet.world_state_update()? else {
        return Ok(false);
    };
    let states = world.world_state_values_mut();
    match update {
        solarity_network::WorldStateUpdate::Initialize { location, values } => {
            states.initialize(location, &values)
        }
        solarity_network::WorldStateUpdate::Value { field, value } => {
            states.set(field, value);
        }
    }
    Ok(true)
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
