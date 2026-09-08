//! Authoritative aura records enter the unit generation that owns their slots.

use solarity_ecs::{ActiveWorld, ObjectKind, UnitAura, UnitAuras};
use solarity_network::WorldUnitAuraUpdate;

pub(super) fn apply(world: &mut ActiveWorld, update: WorldUnitAuraUpdate, receipt_ms: u32) -> bool {
    if !matches!(
        world.object_kind(update.guid),
        Some(ObjectKind::Unit | ObjectKind::Player)
    ) {
        return false;
    }
    let Some(entity) = world.entity_by_guid(update.guid) else {
        return false;
    };
    if world.storage().get::<&UnitAuras>(entity).is_err() {
        world
            .storage_mut()
            .add_component(entity, (UnitAuras::default(),));
    }
    let Ok(mut auras) = world.storage().get::<&mut UnitAuras>(entity) else {
        return false;
    };
    if update.replace {
        auras.clear();
    }
    for aura in update.auras {
        let (duration_ms, end_ms) = aura.duration.map_or((0, 0), |(maximum, remaining)| {
            (maximum, receipt_ms.wrapping_add(remaining).max(1))
        });
        auras.set(
            aura.slot,
            UnitAura {
                spell: aura.spell,
                flags: aura.flags,
                level: aura.level,
                applications: aura.applications,
                caster: aura.caster,
                duration_ms,
                end_ms,
            },
        );
    }
    true
}
