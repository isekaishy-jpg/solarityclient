//! Attachment indexing preserves the ordered build-168 parent-consumer contract.

use super::attachments::{
    ItemRequests, ItemSamples, MountedOwners, RiderSamples, VisualRequests, VisualSamples,
};
use glam::{Mat4, Vec3};
use solarity_rendering::CharacterAttachmentPoint::{HandLeft, HandRight, Shield};

#[test]
fn owner_requests_preserve_order_duplicates_and_replace_departed_membership() {
    let original = [(7, HandLeft), (9, Shield), (7, HandRight), (7, HandLeft)];
    let mut items: ItemRequests = original.into_iter().collect();
    assert_eq!(items.len(), original.len());
    for owner in [7, 9, 11] {
        let expected: Vec<_> = original
            .iter()
            .copied()
            .filter(|&(guid, _)| guid == owner)
            .collect();
        assert_eq!(items.for_owner(owner), expected);
    }
    items.replace([(9, HandRight)]);
    assert_eq!(items.len(), 1);
    assert!(items.for_owner(7).is_empty());
    assert_eq!(items.for_owner(9), [(9, HandRight)]);
    items.replace([]);
    assert_eq!(items.len(), 0);
    assert!(items.for_owner(9).is_empty());

    let original = [
        (7, HandLeft, 3),
        (7, HandRight, 1),
        (7, HandLeft, 2),
        (7, HandLeft, 3),
    ];
    let visuals: VisualRequests = original.into_iter().collect();
    assert_eq!(visuals.len(), 4);
    assert_eq!(
        visuals.for_owner((7, HandLeft)),
        [original[0], original[2], original[3]]
    );
    assert_eq!(visuals.for_owner((7, HandRight)), [original[1]]);
    assert!(visuals.for_owner((9, HandLeft)).is_empty());
}

#[test]
fn indexed_parent_selection_matches_first_sample_and_any_hidden_rider() {
    let first = Mat4::from_translation(Vec3::X);
    let later = Mat4::from_translation(Vec3::Y);
    let original = [
        (7, Some(first)),
        (9, None),
        (7, None),
        (7, Some(later)),
        (9, Some(later)),
    ];
    let mut riders = RiderSamples::default();
    riders.reserve(original.len());
    for sample in original {
        riders.push(sample);
    }
    assert_eq!(&*riders, &original);
    for owner in [7, 9, 11] {
        assert_eq!(
            riders.first(owner),
            original
                .iter()
                .find_map(|&(guid, sample)| (guid == owner).then_some(sample))
        );
        assert_eq!(
            riders.any_hidden(owner),
            original
                .iter()
                .any(|&(guid, sample)| guid == owner && sample.is_none())
        );
    }
    assert_eq!(riders.first(7), Some(Some(first)));
    assert_eq!(riders.first(9), Some(None));
    assert_eq!(riders.first(11), None);
    riders.clear();
    riders.push((7, Some(later)));
    assert_eq!(riders.first(7), Some(Some(later)));
    assert!(!riders.any_hidden(7));
    assert_eq!(riders.first(9), None);
    assert!(!riders.any_hidden(9));
}

#[test]
fn item_and_visual_keys_keep_points_and_generations_separate() {
    let mut items = ItemSamples::default();
    items.push((7, HandRight, None));
    items.push((7, HandLeft, Some(Mat4::IDENTITY)));
    items.push((9, HandRight, Some(Mat4::IDENTITY)));
    items.push((7, HandRight, Some(Mat4::IDENTITY)));
    assert_eq!(items.len(), 4);
    assert_eq!(items.first((7, HandRight)), Some(None));
    assert_eq!(items.first((7, HandLeft)), Some(Some(Mat4::IDENTITY)));
    assert_eq!(items.first((9, HandRight)), Some(Some(Mat4::IDENTITY)));
    assert_eq!(items.first((9, HandLeft)), None);
    let mut visuals = VisualSamples::default();
    visuals.push((7, HandRight, 0, Some(Mat4::IDENTITY)));
    visuals.push((7, HandRight, 1, None));
    visuals.push((7, HandLeft, 1, Some(Mat4::IDENTITY)));
    assert_eq!(visuals.first((7, HandRight, 1)), Some(None));
    assert_eq!(visuals.first((7, HandLeft, 1)), Some(Some(Mat4::IDENTITY)));
    assert_eq!(visuals.first((9, HandRight, 0)), None);
    visuals.clear();
    items.clear();
    assert_eq!(visuals.first((7, HandRight, 0)), None);
    assert_eq!(items.first((7, HandRight)), None);
}

#[test]
fn mount_membership_retains_duplicate_count_and_clears_departed_owners() {
    let mut mounts = MountedOwners::default();
    mounts.replace([7, 9, 7]);
    assert_eq!(mounts.len(), 3);
    assert!(mounts.contains(&7));
    assert!(mounts.contains(&9));
    assert!(!mounts.contains(&11));
    mounts.replace([11]);
    assert_eq!(mounts.len(), 1);
    assert!(!mounts.contains(&7));
    assert!(!mounts.contains(&9));
    assert!(mounts.contains(&11));
}
