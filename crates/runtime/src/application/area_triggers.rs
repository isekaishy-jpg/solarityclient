//! Periodic AreaTrigger entry ownership and ordered movement notification.

use std::ops::Range;

use solarity_asset::AreaTriggerCatalog;
use solarity_ecs::{WorldMovementState, WorldTransform};
use solarity_network::{
    ObjectMovementContext, ObjectMovementFall, ObjectMovementTransport, WorldMovementKind,
    WorldMovementMessage,
};
use solarity_systems::AreaTriggerVolume;

use super::RuntimeGameplayCoordinator;
use super::player_movement::{RuntimePlayerMovement, RuntimePlayerMovementError};

struct Entry {
    id: u32,
    map_id: u32,
    volume: AreaTriggerVolume,
}

pub(super) struct RuntimeAreaTriggers {
    entries: Vec<Entry>,
    map_id: Option<u32>,
    map_entries: Range<usize>,
    active: Option<usize>,
    check_at: u32,
}

impl RuntimeAreaTriggers {
    pub(super) fn new(catalog: &AreaTriggerCatalog) -> Self {
        Self {
            entries: catalog
                .entries()
                .iter()
                .map(|entry| Entry {
                    id: entry.id(),
                    map_id: entry.map_id(),
                    volume: AreaTriggerVolume::new(entry.map_id(), entry.position(), entry.shape()),
                })
                .collect(),
            map_id: None,
            map_entries: 0..0,
            active: None,
            check_at: 0,
        }
    }

    /// 6D2F70 resets the active row and schedules the next 100 ms callback.
    pub(super) fn enter_map(&mut self, map_id: u32, now: u32) {
        self.map_id = Some(map_id);
        self.active = None;
        self.check_at = now.wrapping_add(100);
        let start = self.entries.iter().position(|entry| entry.map_id >= map_id);
        self.map_entries = start
            .filter(|start| self.entries[*start].map_id == map_id)
            .map_or(0..0, |start| {
                let length = self.entries[start..]
                    .iter()
                    .take_while(|entry| entry.map_id <= map_id)
                    .count();
                start..start + length
            });
    }

    pub(super) fn disconnect(&mut self) {
        self.map_id = None;
        self.map_entries = 0..0;
        self.active = None;
    }

    pub(super) fn service(
        &mut self,
        gameplay: &RuntimeGameplayCoordinator,
        movement: &mut RuntimePlayerMovement,
        now: u32,
    ) -> Result<(), RuntimePlayerMovementError> {
        let Some(world) = gameplay.world() else {
            self.disconnect();
            return Ok(());
        };
        if self.map_id.is_some() && (now.wrapping_sub(self.check_at) as i32) < 0 {
            return Ok(());
        }
        let player = world.local_player_guid()?;
        let position = world
            .unit_vitals(player)
            .filter(|vitals| (vitals.health() as i32) > 0)
            .map(|_| {
                world
                    .local_player_transform()
                    .map(|transform| transform.position().to_array())
            })
            .transpose()?;
        let active_mover = gameplay.active_mover_guid().and_then(|guid| {
            Some((
                guid,
                world.object_transform(guid)?,
                world.movement_state(guid)?,
            ))
        });
        if let Some(trigger_id) = self.poll(
            now,
            world.map_id().value(),
            position,
            active_mover.map(|mover| mover.0),
        ) && let Some((guid, transform, state)) = active_mover
        {
            let heartbeat = heartbeat(guid, transform, state)?;
            movement.queue_area_trigger(heartbeat, trigger_id);
            movement.flush_output(gameplay)?;
            tracing::debug!(
                trigger_id,
                map_id = world.map_id().value(),
                "entered area trigger"
            );
        }
        Ok(())
    }

    fn poll(
        &mut self,
        now: u32,
        map_id: u32,
        position: Option<[f32; 3]>,
        active_mover: Option<u64>,
    ) -> Option<u32> {
        if self.map_id.is_none() {
            self.enter_map(map_id, now);
            return None;
        }
        if (now.wrapping_sub(self.check_at) as i32) < 0 {
            return None;
        }
        self.check_at = now.wrapping_add(100);
        let position = position?;
        if self.map_id != Some(map_id) {
            self.enter_map(map_id, now);
        }
        if let Some(active) = self.active {
            if self.entries[active].volume.contains(map_id, position) {
                return None;
            }
            self.active = None;
        }
        active_mover?;
        let index = self
            .map_entries
            .clone()
            .find(|index| self.entries[*index].volume.contains(map_id, position))?;
        self.active = Some(index);
        Some(self.entries[index].id)
    }
}

/// 724E70(EE) freezes the active mover immediately before the trigger packet.
fn heartbeat(
    guid: u64,
    transform: WorldTransform,
    movement: WorldMovementState,
) -> Result<WorldMovementMessage, RuntimePlayerMovementError> {
    let context = movement.context();
    Ok(WorldMovementMessage::new(
        WorldMovementKind::Heartbeat,
        guid,
        movement.flags(),
        transform.position().to_array(),
        transform.orientation(),
        ObjectMovementContext {
            timestamp_ms: context.timestamp_ms,
            transport: context.transport.map(|parent| ObjectMovementTransport {
                guid: parent.guid,
                position: parent.position.to_array(),
                orientation: parent.orientation,
                time_ms: parent.time_ms,
                seat: parent.seat,
                interpolated_time_ms: parent.interpolated_time_ms,
            }),
            pitch_radians: context.pitch_radians,
            fall_time_ms: context.fall_time_ms,
            falling: context.falling.map(|fall| ObjectMovementFall {
                vertical_speed: fall.vertical_speed,
                direction_sin: fall.direction_sin,
                direction_cos: fall.direction_cos,
                horizontal_speed: fall.horizontal_speed,
            }),
            spline_elevation: context.spline_elevation,
        },
    )?)
}

#[cfg(test)]
mod tests {
    use solarity_asset::AreaTriggerShape;
    use solarity_systems::AreaTriggerVolume;

    use super::{Entry, RuntimeAreaTriggers};

    fn tracker() -> RuntimeAreaTriggers {
        RuntimeAreaTriggers {
            entries: [(90, 1, 0.), (20, 1, 2.), (10, 2, 0.)]
                .map(|(id, map_id, x)| Entry {
                    id,
                    map_id,
                    volume: AreaTriggerVolume::new(
                        map_id,
                        [x, 0., 0.],
                        AreaTriggerShape::Sphere { radius: 2. },
                    ),
                })
                .into(),
            map_id: None,
            map_entries: 0..0,
            active: None,
            check_at: 0,
        }
    }

    #[test]
    fn entry_rearms_only_after_leaving_the_active_volume() {
        let mut tracker = tracker();
        tracker.enter_map(1, 0);
        assert_eq!(tracker.poll(99, 1, Some([1., 0., 0.]), Some(7)), None);
        assert_eq!(tracker.poll(100, 1, Some([1., 0., 0.]), Some(7)), Some(90));
        // The second overlapping row cannot fire while the first is active.
        assert_eq!(tracker.poll(200, 1, Some([1., 0., 0.]), Some(7)), None);
        assert_eq!(tracker.poll(300, 1, Some([3., 0., 0.]), Some(7)), Some(20));
        assert_eq!(tracker.poll(400, 1, Some([10., 0., 0.]), Some(7)), None);
        assert_eq!(tracker.poll(500, 1, Some([0., 0., 0.]), Some(7)), Some(90));
        tracker.enter_map(1, 500);
        assert_eq!(tracker.poll(600, 1, Some([0., 0., 0.]), Some(7)), Some(90));
    }

    #[test]
    fn dead_player_missing_mover_and_clock_wrap_preserve_poll_policy() {
        let mut tracker = tracker();
        tracker.enter_map(1, u32::MAX - 49);
        assert_eq!(tracker.poll(49, 1, Some([0.; 3]), Some(7)), None);
        assert_eq!(tracker.poll(50, 1, None, Some(7)), None);
        assert_eq!(tracker.poll(150, 1, Some([0.; 3]), None), None);
        assert_eq!(tracker.poll(250, 1, Some([0.; 3]), Some(7)), Some(90));
        assert_eq!(tracker.poll(350, 1, None, Some(7)), None);
        assert_eq!(tracker.poll(450, 1, Some([0.; 3]), Some(7)), None);
        assert_eq!(tracker.poll(550, 2, Some([0.; 3]), Some(7)), Some(10));
        tracker.disconnect();
        assert_eq!(tracker.poll(650, 1, Some([0.; 3]), Some(7)), None);
        assert_eq!(tracker.poll(750, 1, Some([0.; 3]), Some(7)), Some(90));
    }
}
