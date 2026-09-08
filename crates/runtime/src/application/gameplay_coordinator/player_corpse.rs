//! Corpse query location, frame range latch and absent-transport query clock.

use std::collections::VecDeque;

use glam::{Mat4, Vec3};
use solarity_ecs::ActiveWorld;
use solarity_network::WorldPlayerCorpseUpdate;
use solarity_ui::UiPlayerCorpseState;

#[cfg(test)]
#[path = "../../../tests/application/player_corpse.rs"]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CorpseQuery {
    Location,
    Transport(u32),
}

pub(super) struct RuntimePlayerCorpse {
    pub ui: UiPlayerCorpseState,
    position: Vec3,
    transport: u32,
    fallback: Mat4,
    query_deadline_seconds: u32,
    pub marker: [f32; 2],
    pub queries: VecDeque<CorpseQuery>,
    placement: solarity_systems::GameObjectPlacementResolver,
}

impl Default for RuntimePlayerCorpse {
    fn default() -> Self {
        Self {
            ui: UiPlayerCorpseState::default(),
            position: Vec3::ZERO,
            transport: 0,
            fallback: Mat4::IDENTITY,
            query_deadline_seconds: 0,
            marker: [0.0; 2],
            queries: VecDeque::new(),
            placement: Default::default(),
        }
    }
}

impl RuntimePlayerCorpse {
    /// 524A30 runs on world entry and each ghost-bit transition.
    pub fn clear(&mut self, present_ghost: bool, arena: bool) -> Option<&'static str> {
        self.clear_location();
        if present_ghost && !arena {
            self.queries.push_back(CorpseQuery::Location);
        }
        self.set_range(false, arena)
    }

    fn clear_location(&mut self) {
        self.ui.maps = (u32::MAX, u32::MAX);
        self.position = Vec3::ZERO;
        self.transport = 0;
        self.marker = [0.0; 2];
        // Native 523DB0 does not reset the absent-transport clock or matrix.
    }

    pub fn receive(
        &mut self,
        world: &ActiveWorld,
        update: WorldPlayerCorpseUpdate,
        arena: bool,
        now_seconds: u32,
    ) -> Option<&'static str> {
        match update {
            WorldPlayerCorpseUpdate::Missing => {
                self.clear_location();
                return self.set_range(false, arena);
            }
            WorldPlayerCorpseUpdate::Location {
                map,
                position,
                display_map,
                transport,
            } => {
                if !super::player_ui::RuntimePlayerHealthSnapshot::from_world(world)
                    .is_some_and(|v| v.ghost)
                {
                    return None;
                }
                self.ui.maps = (map, display_map);
                self.position = Vec3::from_array(position);
                self.transport = transport;
                let position = self.resolve(world, now_seconds);
                self.marker = if world.map_id().value() == map {
                    [position.x, position.y]
                } else {
                    [0.0; 2]
                };
            }
            WorldPlayerCorpseUpdate::Transport {
                position,
                orientation,
            } => {
                // 4C3290 rounds FSINCOS to f32 before filling the matrix.
                let (sin, cos) = f64::from(orientation).sin_cos();
                let (sin, cos) = (sin as f32, cos as f32);
                self.fallback = Mat4::from_cols_array(&[
                    cos,
                    sin,
                    0.0,
                    0.0,
                    -sin,
                    cos,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    1.0,
                    0.0,
                    position[0],
                    position[1],
                    position[2],
                    1.0,
                ]);
            }
        }
        None
    }

    pub fn advance(
        &mut self,
        world: &ActiveWorld,
        arena: bool,
        now_seconds: u32,
    ) -> Option<&'static str> {
        self.ui.guid = world.local_corpse_guid();
        let in_range = if (self.ui.maps.0 as i32) >= 0
            && super::player_ui::RuntimePlayerHealthSnapshot::from_world(world).is_some()
            && let Ok(player) = world.local_player_transform()
        {
            let corpse = self.resolve(world, now_seconds);
            if world.map_id().value() == self.ui.maps.0 {
                self.marker = [corpse.x, corpse.y];
            }
            Self::in_range(player.position(), corpse)
        } else {
            false
        };
        self.set_range(in_range, arena)
    }

    fn in_range(player: Vec3, corpse: Vec3) -> bool {
        // 51F640 retains the differences and squared sum in the x87 registers.
        let p = player.to_array().map(f64::from);
        let c = corpse.to_array().map(f64::from);
        let [x, y, z] = std::array::from_fn(|i| p[i] - c[i]);
        (x * x + y * y) + z * z <= 1600.0
    }

    fn set_range(&mut self, in_range: bool, arena: bool) -> Option<&'static str> {
        if self.ui.in_range == in_range {
            return None;
        }
        self.ui.in_range = in_range;
        if arena {
            None
        } else if in_range {
            self.ui.range_event()
        } else {
            Some("CORPSE_OUT_OF_RANGE")
        }
    }

    fn transport_guid(&self) -> u64 {
        if self.transport == 0 {
            return 0;
        }
        let high = ((self.transport as i32 >> 31) as u32) | 0x1fc00000;
        (u64::from(high) << 32) | u64::from(self.transport)
    }

    fn resolve(&mut self, world: &ActiveWorld, now_seconds: u32) -> Vec3 {
        let guid = self.transport_guid();
        let resident = world
            .entity_by_guid(guid)
            .map(|_| self.placement.resolve(world, guid).ok().map(|p| p.matrix()));
        self.resolve_pose(resident, now_seconds)
    }

    fn resolve_pose(&mut self, resident: Option<Option<Mat4>>, now_seconds: u32) -> Vec3 {
        if self.transport == 0 {
            return self.position;
        }
        let matrix = match resident {
            Some(Some(matrix)) => {
                self.query_deadline_seconds = 0;
                matrix
            }
            Some(None) => return self.position,
            None => {
                if self.query_deadline_seconds == 0
                    || now_seconds.wrapping_sub(self.query_deadline_seconds) as i32 >= 0
                {
                    self.queries
                        .push_back(CorpseQuery::Transport(self.transport));
                    self.query_deadline_seconds = now_seconds.wrapping_add(30);
                }
                self.fallback
            }
        };
        let m = matrix.to_cols_array().map(f64::from);
        let [x, y, z] = self.position.to_array().map(f64::from);
        Vec3::from_array(std::array::from_fn(|row| {
            (m[12 + row] + (z * m[8 + row] + y * m[4 + row] + x * m[row])) as f32
        }))
    }
}

pub(super) fn seconds() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as u32
}
