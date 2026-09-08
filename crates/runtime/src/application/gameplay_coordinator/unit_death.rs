//! Replicated unit-death combat records captured before player life callbacks.

use solarity_asset::CharacterFactionCatalog;
use solarity_ecs::{ActiveWorld, ObjectKind, WorldObjectIdentity};

use super::creature_cache::CreatureTemplateCache;
use super::environmental_damage::{RuntimeCombatLogClock, unit_combat_log_flags};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::application) struct RuntimeUnitDeathSnapshot {
    pub identity: WorldObjectIdentity,
    pub name: Option<String>,
    pub flags: u32,
    pub event: &'static str,
    pub timestamp_ms: u32,
    pub clock: RuntimeCombatLogClock,
    pub player_ui: Option<(
        super::player_ui::RuntimePlayerHealthSnapshot,
        solarity_ui::UiPlayerReleaseTimer,
    )>,
}

impl RuntimeUnitDeathSnapshot {
    pub(super) fn admit(
        world: &ActiveWorld,
        identity: WorldObjectIdentity,
        creatures: Option<&CreatureTemplateCache>,
        factions: Option<&CharacterFactionCatalog>,
        timestamp_ms: u32,
        clock: RuntimeCombatLogClock,
    ) -> Option<Self> {
        if world.object_identity(identity.guid()) != Some(identity) {
            return None;
        }
        let template = creatures
            .and_then(|cache| cache.template(identity))
            .filter(|template| {
                world
                    .object_presentation(identity.guid())
                    .is_some_and(|presentation| presentation.entry_id() == template.entry())
            });
        match world.object_kind(identity.guid())? {
            ObjectKind::Player => {}
            // 718A90 suppresses the log while the creature template is absent.
            ObjectKind::Unit
                if template
                    .as_ref()
                    .is_some_and(|template| template.flags() & 0x400 == 0) => {}
            _ => return None,
        }
        let name = if world.local_player_guid().ok() == Some(identity.guid()) {
            world
                .local_player_identity()
                .map(|player| player.name().to_owned())
        } else {
            template.as_ref().and_then(|template| {
                let bytes = &template.strings()[0];
                (!bytes.is_empty()).then(|| String::from_utf8_lossy(bytes).into_owned())
            })
        };
        Some(Self {
            identity,
            name,
            flags: unit_combat_log_flags(world, identity.guid(), factions)?,
            event: if template
                .as_ref()
                .is_some_and(|template| template.creature_type() == 13)
            {
                "UNIT_DISSIPATES"
            } else {
                "UNIT_DIED"
            },
            timestamp_ms,
            clock,
            player_ui: None,
        })
    }
}
