//! Native combat-log flags and faction-template reaction reductions.

use solarity_asset::FactionTemplateDefinition;

/// Facts resolved by the object, relationship, group and selection owners.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CombatLogObjectClassification {
    /// Original event participant identity.
    pub guid: u64,
    /// Participant identity after the native owner/charm resolution.
    pub controller_guid: u64,
    /// Controlled player's identity.
    pub local_player_guid: u64,
    /// A unit has a charmer or summoner, distinguishing pets from guardians.
    pub charmed_or_summoned: bool,
    /// Directed unit reaction to the local player, absent without both units.
    pub reaction: Option<i32>,
    /// The controller appears in the party provider.
    pub party_member: bool,
    /// The controller appears in the raid provider.
    pub raid_member: bool,
    /// Native group/raid state is active.
    pub in_group: bool,
    /// The original participant is the selected target.
    pub target: bool,
    /// The original participant is the selected focus.
    pub focus: bool,
    /// Native group role mask, with tank bit 2 and healer bit 4.
    pub role_mask: u32,
    /// Native raid marker index, if present.
    pub raid_marker: Option<u8>,
}

/// Classifies the player GUID family using the original high-word masks.
#[must_use]
pub const fn is_player_guid(guid: u64) -> bool {
    let high = (guid >> 32) as u32;
    high & 0xf0000000 == 0 && (guid as u32 != 0 || high & 0xf07fffff != 0)
}

/// Reduces resolved facts using `74DCB0`'s flags and branch precedence.
#[must_use]
pub fn combat_log_object_flags(input: CombatLogObjectClassification) -> u32 {
    if input.guid == 0 {
        return 0x80000000;
    }
    let family = (input.guid >> 32) as u32 & 0xf0f00000;
    let mut flags =
        if is_player_guid(input.guid) || matches!(family, 0xf0300000 | 0xf0400000 | 0xf0500000) {
            if input.controller_guid != input.guid {
                if input.charmed_or_summoned {
                    0x1000
                } else {
                    0x2000
                }
            } else if is_player_guid(input.guid) {
                0x400
            } else if family == 0xf0400000 {
                0x1000
            } else {
                0x800
            }
        } else {
            0x4000
        };
    flags |= if is_player_guid(input.controller_guid) {
        0x100
    } else {
        0x200
    };
    if input.controller_guid == input.local_player_guid {
        flags |= 0x11;
    } else {
        if input.reaction.is_some_and(|reaction| reaction <= 1) {
            flags |= 0x40;
        } else {
            if input.reaction.is_some_and(|reaction| reaction > 3) {
                flags |= 0x10;
            }
            if input.in_group {
                if input.party_member {
                    flags |= 0x12;
                } else if input.raid_member {
                    flags |= 0x14;
                }
            }
        }
        if flags & 0xf == 0 {
            flags |= 8;
        }
        if flags & 0xf0 == 0 {
            flags |= 0x20;
        }
    }
    if input.target {
        flags |= 0x10000;
    }
    if input.focus {
        flags |= 0x20000;
    }
    if flags & 0x400 != 0
        && (flags & 6 != 0 || (input.controller_guid == input.local_player_guid && input.in_group))
    {
        if input.role_mask & 2 != 0 {
            flags |= 0x40000;
        } else if input.role_mask & 4 != 0 {
            flags |= 0x80000;
        }
    }
    if input.in_group
        && let Some(index) = input.raid_marker.filter(|index| *index < 8)
    {
        flags |= 0x100000 << index;
    }
    flags
}

/// Native `715440` directed reaction before player/reputation overrides.
#[must_use]
pub fn faction_template_reaction(
    source: FactionTemplateDefinition,
    target: FactionTemplateDefinition,
) -> i32 {
    let listed = |values: &[u32; 4], id| {
        values
            .iter()
            .take_while(|value| **value != 0)
            .any(|value| *value == id)
    };
    if source.enemy_mask() & target.group_mask() != 0
        || listed(source.enemies(), target.faction_id())
    {
        return 1;
    }
    if source.friend_mask() & target.group_mask() != 0
        || listed(source.friends(), target.faction_id())
        || target.friend_mask() & source.group_mask() != 0
        || listed(target.friends(), source.faction_id())
    {
        return 4;
    }
    if source.flags() & 0x2000 != 0 { 1 } else { 3 }
}
