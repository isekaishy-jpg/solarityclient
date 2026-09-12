//! Broad-phase equivalence for disjoint, overlapping and boundary-touching roots.

use super::RootIndex;
use glam::Vec3;

#[test]
fn root_candidates_preserve_all_native_box_candidates() {
    let mut entries = (0..4096)
        .map(|index| {
            let position = Vec3::new((index % 64) as f32 * 10., (index / 64) as f32 * 10., 0.);
            (index, [position, position + Vec3::new(12., 12., 4.)])
        })
        .collect::<Vec<_>>();
    let original = entries.clone();
    let mut nodes = Vec::new();
    RootIndex::build(&mut entries, &mut nodes);
    let index = RootIndex {
        revision: Some(1),
        nodes,
    };
    for bounds in [
        [Vec3::ZERO, Vec3::ZERO],
        [Vec3::new(12., 12., 4.); 2],
        [Vec3::new(305., 100., -1000.), Vec3::new(305., 100., 200.)],
        [Vec3::splat(-20.), Vec3::splat(-10.)],
        [Vec3::ZERO, Vec3::splat(1000.)],
    ] {
        let mut actual = Vec::new();
        index.collect(0, bounds, &mut actual);
        actual.sort_unstable();
        let expected = original
            .iter()
            .filter(|(_, root)| {
                (0..3).all(|axis| {
                    bounds[1][axis] >= root[0][axis] && bounds[0][axis] <= root[1][axis]
                })
            })
            .map(|(index, _)| *index)
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }
}
