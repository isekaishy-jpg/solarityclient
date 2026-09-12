//! Immutable parent order and finger ancestry derived at model admission.

use super::M2Bone;

/// These facts depend only on the already validated, cycle-free bone table.
#[derive(Debug)]
pub(super) struct BoneHierarchy {
    pub(super) order: Vec<usize>,
    pub(super) fingers: Vec<Option<u16>>,
    pub(super) identity_pose: bool,
}

impl BoneHierarchy {
    /// Visits each parent edge once; arbitrary file ordering requires no repeated walks.
    pub(super) fn prepare(bones: &[M2Bone]) -> Self {
        let mut order = Vec::with_capacity(bones.len());
        let mut visited = vec![false; bones.len()];
        let mut path = Vec::new();
        let mut fingers = vec![None; bones.len()];
        for start in 0..bones.len() {
            let mut current = Some(start);
            while let Some(index) = current {
                if visited[index] {
                    break;
                }
                visited[index] = true;
                path.push(index);
                current = bones[index].parent().map(usize::from);
            }
            while let Some(index) = path.pop() {
                order.push(index);
                fingers[index] = match bones[index].key_bone_id() {
                    key @ 8..=17 => Some(key as u16),
                    _ => bones[index]
                        .parent()
                        .and_then(|parent| fingers[usize::from(parent)]),
                };
            }
        }
        let identity_pose = bones.iter().all(|bone| {
            bone.flags() & 0x78 == 0
                && bone
                    .translation()
                    .channels()
                    .iter()
                    .all(|channel| channel.timestamps_ms().is_empty())
                && bone
                    .rotation()
                    .channels()
                    .iter()
                    .all(|channel| channel.timestamps_ms().is_empty())
                && bone
                    .scale()
                    .channels()
                    .iter()
                    .all(|channel| channel.timestamps_ms().is_empty())
        });
        Self {
            order,
            fingers,
            identity_pose,
        }
    }
}
