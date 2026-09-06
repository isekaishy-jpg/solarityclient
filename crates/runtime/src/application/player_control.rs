//! Unit client-control bits and the selected movement subject.

use solarity_ecs::{ActiveWorld, ObjectKind, WorldObjectIdentity};
use solarity_network::WorldClientControlUpdate;
use std::collections::{HashMap, VecDeque};

/// Ordered side effects retained until the composition root admits them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PlayerControlEvent {
    StandState {
        state: u8,
        timestamp_ms: u32,
    },
    PlayerControl {
        enabled: bool,
        timestamp_ms: u32,
    },
    ActiveMover {
        previous: u64,
        next: u64,
        timestamp_ms: u32,
    },
}

pub(super) struct RuntimePlayerControl {
    player: WorldObjectIdentity,
    active_mover: u64,
    units: HashMap<WorldObjectIdentity, bool>,
    pending: Option<WorldClientControlUpdate>,
    events: VecDeque<PlayerControlEvent>,
}

impl RuntimePlayerControl {
    pub(super) fn new(player: WorldObjectIdentity) -> Self {
        Self {
            player,
            active_mover: player.guid(),
            units: HashMap::from([(player, true)]),
            pending: None,
            events: VecDeque::new(),
        }
    }

    pub(super) fn active_mover(&self) -> u64 {
        self.active_mover
    }
    pub(super) fn player_enabled(&self) -> bool {
        self.unit_enabled(self.player)
    }
    pub(super) fn unit_enabled(&self, identity: WorldObjectIdentity) -> bool {
        self.units.get(&identity).copied().unwrap_or(false)
    }
    pub(super) fn take_event(&mut self) -> Option<PlayerControlEvent> {
        self.events.pop_front()
    }

    pub(super) fn stand_state(&mut self, state: u8, timestamp_ms: u32) {
        self.events.push_back(PlayerControlEvent::StandState {
            state,
            timestamp_ms,
        });
    }

    pub(super) fn receive(
        &mut self,
        world: &ActiveWorld,
        update: WorldClientControlUpdate,
        timestamp_ms: u32,
    ) {
        if let Some(identity) = unit_identity(world, update.guid) {
            self.apply(identity, update.enabled, timestamp_ms);
        } else if self.pending.is_none_or(|pending| {
            pending.guid == 0 || pending.guid == update.guid || !pending.enabled
        }) || update.enabled
        {
            // 716060 retains one pending subject. An enabled subject is not
            // displaced by a different subject's disable arriving before create.
            self.pending = Some(update);
        }
    }

    pub(super) fn synchronize(&mut self, world: &ActiveWorld, timestamp_ms: u32) {
        self.units
            .retain(|identity, _| world.object_identity(identity.guid()) == Some(*identity));
        if let Some(pending) = self.pending
            && let Some(identity) = unit_identity(world, pending.guid)
        {
            self.pending = None;
            self.apply(identity, pending.enabled, timestamp_ms);
        }
    }

    fn apply(&mut self, identity: WorldObjectIdentity, enabled: bool, timestamp_ms: u32) {
        let previous_enabled = self.unit_enabled(identity);
        self.units.insert(identity, enabled);
        if identity == self.player && previous_enabled != enabled {
            self.events.push_back(PlayerControlEvent::PlayerControl {
                enabled,
                timestamp_ms,
            });
        }
        // 72CCA0 selects the player on control gain. Losing the selected unit
        // falls back to the controlled player, or zero when neither is owned.
        let next = if identity == self.player && enabled {
            self.player.guid()
        } else if identity.guid() == self.active_mover && !enabled {
            if self.player_enabled() {
                self.player.guid()
            } else {
                0
            }
        } else {
            self.active_mover
        };
        if next != self.active_mover {
            self.events.push_back(PlayerControlEvent::ActiveMover {
                previous: self.active_mover,
                next,
                timestamp_ms,
            });
            self.active_mover = next;
        }
    }
}

fn unit_identity(world: &ActiveWorld, guid: u64) -> Option<WorldObjectIdentity> {
    matches!(
        world.object_kind(guid),
        Some(ObjectKind::Unit | ObjectKind::Player)
    )
    .then(|| world.object_identity(guid))
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> ActiveWorld {
        ActiveWorld::enter(solarity_ecs::WorldBootstrap::new(
            solarity_ecs::WorldMapId::new(0),
            8,
            "Control",
            glam::Vec3::ZERO,
            0.,
        ))
    }

    #[test]
    fn unknown_subject_priority_matches_original_pending_slot()
    -> Result<(), Box<dyn std::error::Error>> {
        let world = world();
        let player = world.object_identity(8).ok_or("player")?;
        for line in include_str!("../../tests/fixtures/player-control-pending-native.txt")
            .lines()
            .filter(|line| !line.starts_with('#'))
        {
            let row: Vec<_> = line.split_whitespace().collect();
            let mut control = RuntimePlayerControl::new(player);
            control.pending = Some(WorldClientControlUpdate {
                guid: u64::from_str_radix(row[0], 16)?,
                enabled: row[1].parse::<u8>()? != 0,
            });
            control.receive(
                &world,
                WorldClientControlUpdate {
                    guid: u64::from_str_radix(row[2], 16)?,
                    enabled: row[3].parse::<u8>()? != 0,
                },
                17,
            );
            assert_eq!(
                control.pending,
                Some(WorldClientControlUpdate {
                    guid: u64::from_str_radix(row[4], 16)?,
                    enabled: row[5].parse::<u8>()? != 0,
                }),
                "{line}"
            );
            assert!(control.take_event().is_none());
        }
        Ok(())
    }

    #[test]
    fn deferred_player_loss_and_gain_notify_before_mover_selection()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut world = world();
        let mut control = RuntimePlayerControl::new(world.object_identity(8).ok_or("player")?);
        control.receive(
            &world,
            WorldClientControlUpdate {
                guid: 8,
                enabled: false,
            },
            10,
        );
        assert_eq!(control.active_mover(), 8);
        assert!(control.take_event().is_none());
        world.create_object(8, ObjectKind::Player, None, [])?;
        control.synchronize(&world, 20);
        assert!(!control.player_enabled());
        assert_eq!(
            control.take_event(),
            Some(PlayerControlEvent::PlayerControl {
                enabled: false,
                timestamp_ms: 20
            })
        );
        assert_eq!(
            control.take_event(),
            Some(PlayerControlEvent::ActiveMover {
                previous: 8,
                next: 0,
                timestamp_ms: 20
            })
        );
        control.receive(
            &world,
            WorldClientControlUpdate {
                guid: 8,
                enabled: false,
            },
            21,
        );
        assert!(control.take_event().is_none());
        control.receive(
            &world,
            WorldClientControlUpdate {
                guid: 8,
                enabled: true,
            },
            30,
        );
        assert_eq!(
            control.take_event(),
            Some(PlayerControlEvent::PlayerControl {
                enabled: true,
                timestamp_ms: 30
            })
        );
        assert_eq!(
            control.take_event(),
            Some(PlayerControlEvent::ActiveMover {
                previous: 0,
                next: 8,
                timestamp_ms: 30
            })
        );
        assert!(control.take_event().is_none());
        Ok(())
    }
}
