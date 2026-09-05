//! Main-thread transfer callbacks, separate from loading-card completion.

use solarity_network::{WorldLocation, WorldTransfer, WorldTransferTransport};
use thiserror::Error;

/// A transfer reaches a stock presentation dependency not yet implemented.
#[derive(Debug, Error)]
pub enum RuntimeWorldTransferError {
    /// The matching transport requires stock's animated world-map loading card.
    #[error("transport {entry} requires an unsupported dynamic loading card")]
    DynamicTransportCard {
        /// Template entry supplied by the pending-transfer packet.
        entry: u32,
    },
}

/// One destination-map replacement whose network completion is still pending.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RuntimeWorldReplacement {
    location: WorldLocation,
    acknowledge: bool,
}

impl RuntimeWorldReplacement {
    /// Returns the server-authored destination consumed by this callback.
    #[must_use]
    pub const fn location(self) -> WorldLocation {
        self.location
    }

    /// Returns whether map completion must send `MSG_MOVE_WORLDPORT_ACK`.
    #[must_use]
    pub const fn requires_acknowledgement(self) -> bool {
        self.acknowledge
    }
}

/// Immediate presentation or map-ownership work requested by a packet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RuntimeWorldTransferEffect {
    /// The packet only changed deferred callback state or verified the same map.
    None,
    /// Open a card while retaining the published world until NEW_WORLD arrives.
    OpenCard {
        /// Destination Map.dbc identifier.
        map_id: u32,
        /// Ship/spline context for the stock dynamic card selector.
        transport: Option<WorldTransferTransport>,
    },
    /// Dismiss the pending card and present the stock rejection message.
    Abort {
        /// Map whose transfer failed.
        map_id: u32,
        /// Exact server rejection code.
        reason: u8,
        /// Optional expansion, difficulty, or unique-message index.
        argument: Option<u8>,
    },
    /// A different-map verify-world invokes the replacement callback immediately.
    Replace(RuntimeWorldReplacement),
    /// Stock rejects unknown destination map rows before scheduling a callback.
    InvalidMap(u32),
}

/// Owns stock's shared destination slot and zero-delay callback queue.
///
/// `0x00403D10` overwrites one destination slot but schedules a callback for
/// every valid NEW_WORLD. Two packets drained together therefore cause two
/// replacements of the latest destination, each with its own acknowledgement.
/// Map loading is asynchronous here; packet dispatch pauses until the active
/// callback completes, preserving stock's synchronous replacement boundary.
#[derive(Default)]
pub struct RuntimeWorldTransferCoordinator {
    destination: Option<WorldLocation>,
    scheduled_callbacks: usize,
    loading: Option<RuntimeWorldReplacement>,
    awaiting_destination: bool,
    entering_world: bool,
}

impl RuntimeWorldTransferCoordinator {
    /// Creates a transfer owner with no pending card or callback.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies a decoded packet against the current map and exact DBC membership.
    ///
    /// The caller drains packets only while [`Self::is_loading_map`] is false.
    pub fn receive(
        &mut self,
        transfer: WorldTransfer,
        current_map_id: u32,
        destination_exists: bool,
    ) -> RuntimeWorldTransferEffect {
        match transfer {
            WorldTransfer::Pending { map_id, transport } => {
                self.awaiting_destination = true;
                RuntimeWorldTransferEffect::OpenCard { map_id, transport }
            }
            WorldTransfer::Aborted {
                map_id,
                reason,
                argument,
            } => {
                self.awaiting_destination = false;
                RuntimeWorldTransferEffect::Abort {
                    map_id,
                    reason,
                    argument,
                }
            }
            WorldTransfer::VerifyWorld(location) if location.map_id() == current_map_id => {
                RuntimeWorldTransferEffect::None
            }
            WorldTransfer::NewWorld(location) | WorldTransfer::VerifyWorld(location) => {
                if !destination_exists {
                    return RuntimeWorldTransferEffect::InvalidMap(location.map_id());
                }
                self.destination = Some(location);
                if matches!(transfer, WorldTransfer::NewWorld(_)) {
                    self.scheduled_callbacks += 1;
                    RuntimeWorldTransferEffect::None
                } else {
                    let replacement = self.start_replacement(location, false);
                    RuntimeWorldTransferEffect::Replace(replacement)
                }
            }
        }
    }

    /// Starts the next zero-delay callback after the current packet batch.
    pub fn take_deferred_replacement(&mut self) -> Option<RuntimeWorldReplacement> {
        if self.loading.is_some() || self.scheduled_callbacks == 0 {
            return None;
        }
        let location = self.destination?;
        self.scheduled_callbacks -= 1;
        Some(self.start_replacement(location, true))
    }

    /// Returns the active map load, including its acknowledgement obligation.
    #[must_use]
    pub const fn loading(&self) -> Option<RuntimeWorldReplacement> {
        self.loading
    }

    /// Returns whether application packet dispatch is suspended for map loading.
    #[must_use]
    pub const fn is_loading_map(&self) -> bool {
        self.loading.is_some()
    }

    /// Returns whether a pending card still belongs to the old published world.
    #[must_use]
    pub const fn is_awaiting_destination(&self) -> bool {
        self.awaiting_destination
    }

    /// Returns whether replacement has not yet admitted its new player.
    #[must_use]
    pub const fn is_entering_world(&self) -> bool {
        self.entering_world
    }

    /// Completes the callback after map residency and any required ACK admission.
    ///
    /// This does not release the card: the replacement player still has to arrive.
    pub fn complete_map(&mut self) {
        self.loading = None;
    }

    /// Returns whether pending destination or map work still holds the card.
    #[must_use]
    pub const fn holds_loading_card(&self) -> bool {
        self.awaiting_destination || self.loading.is_some() || self.scheduled_callbacks != 0
    }

    /// Takes the enter-world notification once destination player readiness passes.
    pub fn complete_player(&mut self) -> bool {
        if self.holds_loading_card() || !self.entering_world {
            return false;
        }
        self.entering_world = false;
        true
    }

    /// Clears callback and presentation obligations when the session ends.
    pub fn disconnect(&mut self) {
        *self = Self::default();
    }

    /// Marks a callback active before the application releases old-world owners.
    fn start_replacement(
        &mut self,
        location: WorldLocation,
        acknowledge: bool,
    ) -> RuntimeWorldReplacement {
        let replacement = RuntimeWorldReplacement {
            location,
            acknowledge,
        };
        self.loading = Some(replacement);
        self.awaiting_destination = false;
        self.entering_world = true;
        replacement
    }
}
