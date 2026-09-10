//! Watched unit fields use the final raw image and the last saved packet mirror.

use std::collections::{HashMap, HashSet};

use solarity_ecs::{ActiveWorld, ObjectFields, ObjectKind, WorldObjectIdentity};
use solarity_network::{WorldObjectUpdate, WorldObjectUpdateBatch};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::application) enum UnitFieldNotification {
    Initialize,
    PlayerBytes2 { changed: u8 },
    Health { previous: u32 },
    MaximumHealth,
    PlayerFlags { previous: u32 },
}

#[derive(Clone, Copy, Default)]
pub(super) struct UnitFieldImage {
    health: u32,
    maximum: u32,
    player_flags: u32,
    player_bytes_2: u32,
}

impl UnitFieldImage {
    pub(super) fn read(world: &ActiveWorld, guid: u64) -> Option<Self> {
        if !matches!(
            world.object_kind(guid),
            Some(ObjectKind::Unit | ObjectKind::Player)
        ) {
            return None;
        }
        let entity = world.entity_by_guid(guid)?;
        let fields = world.storage().get::<&ObjectFields>(entity).ok()?;
        Some(Self {
            health: fields.get(24),
            maximum: fields.get(32),
            player_flags: fields.get(150),
            player_bytes_2: fields.get(1229),
        })
    }
}

#[derive(Default)]
pub(super) struct UnitFieldMirrors {
    previous: HashMap<WorldObjectIdentity, UnitFieldImage>,
    created: HashSet<WorldObjectIdentity>,
}

impl UnitFieldMirrors {
    pub(super) fn record(
        &mut self,
        world: &ActiveWorld,
        guid: u64,
        previous: Option<UnitFieldImage>,
        created: bool,
    ) {
        if UnitFieldImage::read(world, guid).is_none() {
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

    pub(super) fn dispatch(
        self,
        world: &mut ActiveWorld,
        batch: &WorldObjectUpdateBatch,
        notify: &mut impl FnMut(&ActiveWorld, WorldObjectIdentity, UnitFieldNotification),
    ) {
        let mut created = self.created;
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
            if create && guid == world.local_player_guid().unwrap_or(0) {
                notify(world, identity, UnitFieldNotification::Initialize);
            }
            if create && created.remove(&identity) {
                continue;
            }
            let Some(current) = UnitFieldImage::read(world, guid) else {
                continue;
            };
            let touched = |word| create || fields.iter().any(|field| field.index() == word);
            if touched(24) && previous.health != current.health {
                if (previous.health as i32) > 0 && (current.health as i32) <= 0 {
                    world.set_unit_attack_target(guid, 0);
                }
                if let Some(entity) = world.entity_by_guid(guid) {
                    world.storage_mut().add_component(
                        entity,
                        (solarity_ecs::UnitHealthPrediction::new(
                            current.health as i32,
                        ),),
                    );
                }
                notify(
                    world,
                    identity,
                    UnitFieldNotification::Health {
                        previous: previous.health,
                    },
                );
            }
            if touched(32) && previous.maximum != current.maximum {
                notify(world, identity, UnitFieldNotification::MaximumHealth);
            }
            if world.object_kind(guid) == Some(ObjectKind::Player)
                && touched(150)
                && previous.player_flags != current.player_flags
            {
                notify(
                    world,
                    identity,
                    UnitFieldNotification::PlayerFlags {
                        previous: previous.player_flags,
                    },
                );
            }
            if world.object_kind(guid) == Some(ObjectKind::Player)
                && touched(1229)
                && (previous.player_bytes_2 ^ current.player_bytes_2) & 0xff000000 != 0
            {
                notify(
                    world,
                    identity,
                    UnitFieldNotification::PlayerBytes2 {
                        changed: ((previous.player_bytes_2 ^ current.player_bytes_2) >> 24) as u8,
                    },
                );
            }
        }
    }
}
