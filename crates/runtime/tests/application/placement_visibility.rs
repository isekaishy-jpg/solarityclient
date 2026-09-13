//! Native callback ordering survives cached ancestry and forward vehicle parents.

use super::M2PlacementVisibility;

/// Replays the recovered native traversal for both ordinary and vehicle parents.
#[test]
fn callback_subtrees_match_original_scene_traversal() -> Result<(), Box<dyn std::error::Error>> {
    let mut cases = 0;
    for row in include_str!("../fixtures/native_model_scene_order.txt").lines() {
        if row.starts_with('#') || row.is_empty() {
            continue;
        }
        let (input, output) = row.split_once('|').ok_or("scene row")?;
        let input = input
            .split_whitespace()
            .map(str::parse::<i32>)
            .collect::<Result<Vec<_>, _>>()?;
        let expected = output
            .split_whitespace()
            .map(str::parse::<usize>)
            .collect::<Result<Vec<_>, _>>()?;
        let mut visibility = M2PlacementVisibility {
            dynamic_indices: input[5..].iter().map(|index| *index as usize).collect(),
            light_parents: input[..5]
                .iter()
                .map(|parent| usize::try_from(*parent).ok())
                .collect(),
            ..Default::default()
        };
        visibility.rebuild_scene_order();
        assert_eq!(visibility.dynamic_scene_indices(), expected, "{row}");
        // The same graph can be supplied by vehicle attachment publication.
        let parents = visibility
            .light_parents
            .iter()
            .enumerate()
            .filter_map(|(index, parent)| parent.map(|parent| (index, parent)))
            .collect();
        visibility.light_parents.fill(None);
        visibility.set_vehicle_parents(&parents);
        assert_eq!(visibility.dynamic_scene_indices(), expected, "{row}");
        cases += 1;
    }
    assert_eq!(cases, 480);
    Ok(())
}
