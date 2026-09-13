//! Adjacent geometry shares submission only when its resolved scene is equal.

use super::RecordContext;
use crate::M2PreparedDraw;

/// Different receivers can resolve to the same exterior lights and fog. Compare
/// those immutable results only after geometry compatibility; never merge across
/// an actual lighting boundary or change the scene's logical receiver indices.
pub(super) fn compatible(
    context: &RecordContext<'_>,
    left: M2PreparedDraw,
    right: M2PreparedDraw,
) -> bool {
    if left.scene_index() == right.scene_index() {
        return left.can_instance_with(right);
    }
    if !left.can_instance_with(right.with_scene_index(left.scene_index())) {
        return false;
    }
    let (Some(left), Some(right)) = (left.scene_index(), right.scene_index()) else {
        return false;
    };
    let scenes = context.scene.m2_instance_scenes();
    match (scenes.get(left as usize), scenes.get(right as usize)) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}
