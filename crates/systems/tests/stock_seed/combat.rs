//! External stock-compatibility tests for `systems/combat` belong here.
use solarity_systems::{
    CombatLogObjectClassification, combat_log_object_flags, faction_template_reaction,
};

#[test]
fn combat_classification_matches_native_owner_reaction_group_and_selection_flags()
-> Result<(), Box<dyn std::error::Error>> {
    for line in include_str!("../fixtures/combat_classification_native.txt").lines() {
        let row = line
            .split_ascii_whitespace()
            .map(str::parse::<i128>)
            .collect::<Result<Vec<_>, _>>()?;
        let input = CombatLogObjectClassification {
            guid: row[0] as u64,
            controller_guid: row[1] as u64,
            local_player_guid: 1,
            charmed_or_summoned: row[2] != 0,
            reaction: (row[3] >= 0).then_some(row[3] as i32),
            party_member: row[4] != 0,
            raid_member: row[5] != 0,
            in_group: row[6] != 0,
            target: row[7] != 0,
            focus: row[8] != 0,
            role_mask: row[9] as u32,
            raid_marker: Some(row[10] as u8),
        };
        assert_eq!(combat_log_object_flags(input), row[11] as u32, "{line}");
    }
    Ok(())
}

#[test]
fn directed_faction_reaction_matches_native_masks_lists_and_termination()
-> Result<(), Box<dyn std::error::Error>> {
    for line in include_str!("../fixtures/combat_classification_native.factions.txt").lines() {
        let row = line
            .split_ascii_whitespace()
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>()?;
        let source = solarity_asset::FactionTemplateDefinition::from_words(row[..14].try_into()?);
        let target = solarity_asset::FactionTemplateDefinition::from_words(row[14..28].try_into()?);
        assert_eq!(
            faction_template_reaction(source, target),
            row[28] as i32,
            "{line}"
        );
    }
    Ok(())
}
