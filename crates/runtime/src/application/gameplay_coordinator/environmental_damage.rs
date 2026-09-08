//! Environmental impact snapshots at the original packet admission boundary.

use solarity_asset::CharacterFactionCatalog;
use solarity_ecs::{
    ActiveWorld, ObjectFields, ObjectKind, UnitHealthPrediction, UnitVitals, WorldObjectIdentity,
};
use solarity_network::WorldEnvironmentalDamage;
use solarity_systems::{
    CombatLogObjectClassification, EnvironmentalDamageKind, combat_log_object_flags,
    faction_template_reaction, is_player_guid, predict_unit_health,
};

/// The native combat-log timestamp anchor survives individual packet delivery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::application) struct RuntimeCombatLogClock {
    pub unix_seconds: i32,
    pub milliseconds: u32,
}

impl Default for RuntimeCombatLogClock {
    fn default() -> Self {
        Self {
            unix_seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |time| time.as_secs() as i32),
            milliseconds: crate::platform::client_milliseconds(),
        }
    }
}

impl RuntimeCombatLogClock {
    pub(in crate::application) fn timestamp(self, milliseconds: u32) -> f64 {
        f64::from(self.unix_seconds)
            + f64::from(milliseconds.wrapping_sub(self.milliseconds)) * 0.001
    }
}

/// Values belong to the affected unit generation and the packet's position in time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::application) struct RuntimeEnvironmentalDamageSnapshot {
    pub identity: WorldObjectIdentity,
    pub packet: WorldEnvironmentalDamage,
    pub kind: EnvironmentalDamageKind,
    pub name: Option<String>,
    pub flags: u32,
    pub local_player: bool,
    pub timestamp_ms: u32,
    pub clock: RuntimeCombatLogClock,
    pub attack_target_guid: u64,
    pub template_flags: Option<u32>,
}

impl RuntimeEnvironmentalDamageSnapshot {
    pub(in crate::application) fn has_combat_event(&self) -> bool {
        self.packet.amount != 0 || self.packet.absorbed != 0 || self.packet.resisted != 0
    }

    pub(super) fn admit(
        world: &mut ActiveWorld,
        packet: WorldEnvironmentalDamage,
        name: Option<String>,
        factions: Option<&CharacterFactionCatalog>,
        timestamp_ms: u32,
        clock: RuntimeCombatLogClock,
    ) -> Option<Self> {
        if !matches!(
            world.object_kind(packet.guid),
            Some(ObjectKind::Unit | ObjectKind::Player)
        ) {
            return None;
        }
        let kind = EnvironmentalDamageKind::from_value(packet.kind)?;
        let identity = world.object_identity(packet.guid)?;
        let local_guid = world.local_player_guid().ok()?;
        let entity = world.entity_by_guid(packet.guid)?;
        let flags = unit_combat_log_flags(world, packet.guid, factions)?;
        let snapshot = Self {
            identity,
            packet,
            kind,
            name,
            flags,
            local_player: packet.guid == local_guid,
            timestamp_ms,
            clock,
            attack_target_guid: world.unit_attack_target(packet.guid),
            template_flags: None,
        };
        let vitals = world
            .storage()
            .get::<&UnitVitals>(entity)
            .map(|vitals| **vitals)
            .ok();
        if snapshot.has_combat_event()
            && let Some(vitals) = vitals
        {
            let maximum = vitals.max_health() as i32;
            let current = world
                .unit_health_prediction(packet.guid)
                .map_or(vitals.health() as i32, UnitHealthPrediction::health);
            let secondary = world
                .unit_flags(packet.guid)
                .map_or(0, |flags| flags.secondary());
            let health =
                predict_unit_health(current, maximum, secondary, packet.amount.wrapping_neg());
            world
                .storage_mut()
                .add_component(entity, (UnitHealthPrediction::new(health),));
        }
        Some(snapshot)
    }
}

pub(super) fn unit_combat_log_flags(
    world: &ActiveWorld,
    guid: u64,
    factions: Option<&CharacterFactionCatalog>,
) -> Option<u32> {
    let local_guid = world.local_player_guid().ok()?;
    let fields = unit_fields(world, guid)?;
    let mut controller = owner(&fields).unwrap_or(guid);
    let charmed_or_summoned = field_guid(&fields, 12) != 0 || field_guid(&fields, 14) != 0;
    drop(fields);
    if controller != guid
        && !is_player_guid(controller)
        && let Some(next) = unit_fields(world, controller).and_then(|fields| owner(&fields))
    {
        controller = next;
    }
    Some(combat_log_object_flags(CombatLogObjectClassification {
        guid,
        controller_guid: controller,
        local_player_guid: local_guid,
        charmed_or_summoned,
        reaction: directed_reaction(world, guid, local_guid, factions),
        // Resident providers are presently ungrouped and unselected.
        party_member: false,
        raid_member: false,
        in_group: false,
        target: false,
        focus: false,
        role_mask: 0,
        raid_marker: None,
    }))
}

fn field_guid(fields: &ObjectFields, index: u16) -> u64 {
    u64::from(fields.get(index)) | (u64::from(fields.get(index + 1)) << 32)
}

fn owner(fields: &ObjectFields) -> Option<u64> {
    let charm = field_guid(fields, 12);
    let owner = if charm != 0 {
        charm
    } else {
        field_guid(fields, 16)
    };
    (owner != 0).then_some(owner)
}

fn unit_fields(
    world: &ActiveWorld,
    guid: u64,
) -> Option<shipyard::advanced::get_component::Ref<'_, &ObjectFields>> {
    if !matches!(
        world.object_kind(guid),
        Some(ObjectKind::Unit | ObjectKind::Player)
    ) {
        return None;
    }
    world
        .storage()
        .get::<&ObjectFields>(world.entity_by_guid(guid)?)
        .ok()
}

fn player_controller(world: &ActiveWorld, guid: u64) -> Option<u64> {
    let fields = unit_fields(world, guid)?;
    let candidate = owner(&fields).unwrap_or(guid);
    if world.object_kind(candidate) == Some(ObjectKind::Player) {
        return Some(candidate);
    }
    let fields = unit_fields(world, candidate)?;
    let candidate = owner(&fields)?;
    (world.object_kind(candidate) == Some(ObjectKind::Player)).then_some(candidate)
}

/// Raw unit relationships and table fallback before group/reputation overrides.
fn directed_reaction(
    world: &ActiveWorld,
    source: u64,
    target: u64,
    factions: Option<&CharacterFactionCatalog>,
) -> Option<i32> {
    if source == target {
        return Some(4);
    }
    let a = unit_fields(world, source)?;
    let b = unit_fields(world, target)?;
    if a.get(59) & 8 != 0 && b.get(59) & 8 != 0 {
        if let (Some(a_owner), Some(b_owner)) = (
            player_controller(world, source),
            player_controller(world, target),
        ) {
            let a_player = unit_fields(world, a_owner)?;
            let b_player = unit_fields(world, b_owner)?;
            if a_player.get(156) != 0
                && b_player.get(156) != 0
                && field_guid(&a_player, 148) == field_guid(&b_player, 148)
            {
                return Some(if a_player.get(156) == b_player.get(156) {
                    4
                } else {
                    1
                });
            }
            if a_owner == b_owner {
                return Some(4);
            }
        }
        if a.get(122) & 0x400 != 0 && b.get(122) & 0x400 != 0 {
            return Some(1);
        }
    }
    Some(
        factions
            .and_then(|catalog| {
                Some((*catalog.template(a.get(55))?, *catalog.template(b.get(55))?))
            })
            .map_or(3, |(a, b)| faction_template_reaction(a, b)),
    )
}
