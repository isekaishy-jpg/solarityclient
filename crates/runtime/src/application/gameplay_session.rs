//! Composition boundary between an accepted network login and ECS ownership.

use glam::Vec3;
use solarity_ecs::{ActiveWorld, WorldBootstrap, WorldMapId};
use solarity_network::InWorldSession;

/// Live world transport paired with the ECS state it authoritatively seeded.
pub struct GameplaySession<S> {
    network: InWorldSession<S>,
    world: ActiveWorld,
}

impl<S> GameplaySession<S> {
    /// Transfers a verified selected-character result into ECS ownership.
    #[must_use]
    pub fn enter(network: InWorldSession<S>) -> Self {
        let location = network.location();
        let bootstrap = WorldBootstrap::new(
            WorldMapId::new(location.map_id()),
            network.character_guid(),
            network.character_name(),
            Vec3::new(location.x(), location.y(), location.z()),
            location.orientation(),
        );
        Self {
            network,
            world: ActiveWorld::enter(bootstrap),
        }
    }

    /// Returns the encrypted active-world network session.
    #[must_use]
    pub const fn network(&self) -> &InWorldSession<S> {
        &self.network
    }

    /// Returns mutable access to the active-world network session.
    #[must_use]
    pub const fn network_mut(&mut self) -> &mut InWorldSession<S> {
        &mut self.network
    }

    /// Returns the ECS-owned active world.
    #[must_use]
    pub const fn world(&self) -> &ActiveWorld {
        &self.world
    }

    /// Returns mutable access to the ECS-owned active world.
    #[must_use]
    pub const fn world_mut(&mut self) -> &mut ActiveWorld {
        &mut self.world
    }
}
