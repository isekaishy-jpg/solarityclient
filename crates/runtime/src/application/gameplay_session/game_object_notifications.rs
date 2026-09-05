//! Packet mirror comparison and native ascending-word GameObject notifications.

#[cfg(test)]
#[path = "../../../tests/application/game_object_notifications.rs"]
mod tests;

use std::collections::{HashMap, HashSet};

use solarity_ecs::{ActiveWorld, GameObjectPresentation, ObjectKind, WorldObjectIdentity};
use solarity_network::{WorldObjectUpdate, WorldObjectUpdateBatch};

use crate::application::game_object_behavior::GameObjectNotification;

#[derive(Default)]
pub(super) struct GameObjectUpdateMirrors {
    previous: HashMap<WorldObjectIdentity, GameObjectPresentation>,
    created: HashSet<WorldObjectIdentity>,
}

impl GameObjectUpdateMirrors {
    /// Each raw values block saves the watched old ranges before overwriting them.
    pub(super) fn record(
        &mut self,
        world: &ActiveWorld,
        guid: u64,
        previous: Option<GameObjectPresentation>,
        created: bool,
    ) {
        if world.object_kind(guid) != Some(ObjectKind::GameObject) {
            return;
        }
        let Some(identity) = world.object_identity(guid) else {
            return;
        };
        self.previous.insert(identity, previous.unwrap_or_default());
        if created {
            self.created.insert(identity);
        }
    }

    pub(super) fn dispatch<E>(
        mut self,
        world: &ActiveWorld,
        batch: &WorldObjectUpdateBatch,
        notify: &mut impl FnMut(
            &ActiveWorld,
            WorldObjectIdentity,
            GameObjectNotification,
        ) -> Result<(), E>,
    ) -> Result<(), E> {
        for update in batch.updates() {
            let (guid, fields, create) = match update {
                WorldObjectUpdate::Values { guid, fields } => (*guid, fields, false),
                WorldObjectUpdate::Create { guid, fields, .. } => (*guid, fields, true),
                _ => continue,
            };
            let Some(identity) = world.object_identity(guid) else {
                continue;
            };
            let Some(previous) = self.previous.get(&identity) else {
                continue;
            };
            if create && self.created.remove(&identity) {
                notify(world, identity, GameObjectNotification::Initialize)?;
                continue;
            }
            // Existing creates use native 4D5550's full-refresh notification arm.
            let touched = |word| create || fields.iter().any(|field| field.index() == word);
            if touched(9)
                && world
                    .game_object_presentation(guid)
                    .is_some_and(|fields| fields.flags() != previous.flags())
            {
                notify(
                    world,
                    identity,
                    GameObjectNotification::Flags {
                        previous: previous.flags(),
                    },
                )?;
            }
            // Compare live fields here: preceding handlers may have consumed a seek.
            if touched(14)
                && world.game_object_presentation(guid).is_some_and(|fields| {
                    fields.sequence_progress() != previous.sequence_progress()
                })
            {
                notify(world, identity, GameObjectNotification::Progress)?;
            }
            // State has the native always-notify registration bit; its behavior
            // performs the additional cached-state comparison itself.
            if touched(17) {
                notify(world, identity, GameObjectNotification::State)?;
            }
        }
        Ok(())
    }
}
