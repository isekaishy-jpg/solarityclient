//! Compact ordering must reproduce the former stable sort of full draw records.

use super::stable_scene_order;
use solarity_cpu::FixedWriter;

/// A large payload exposes accidental movement or identity loss across cycles.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Draw {
    order: u32,
    identity: usize,
    payload: [u64; 64],
}

#[test]
fn compact_permutation_matches_stable_sort_with_duplicate_scene_orders() {
    for count in [0, 1, 2, 3, 7, 64, 257, 1024] {
        let mut draws: Vec<_> = (0..count)
            .map(|identity| Draw {
                order: ((identity * 31 + identity / 7) % 19) as u32,
                identity,
                payload: [identity as u64; 64],
            })
            .collect();
        let mut expected = draws.clone();
        expected.sort_by_key(|draw| draw.order);
        let mut indices = Vec::with_capacity(count);
        let original_capacity = indices.capacity();
        assert!(
            stable_scene_order(&mut draws, &mut FixedWriter::new(&mut indices), |draw| draw
                .order)
            .is_ok()
        );
        assert_eq!(draws, expected);
        assert_eq!(indices.capacity(), original_capacity);
        // Reuse one allocation for a different permutation and tie order.
        draws.reverse();
        expected = draws.clone();
        expected.sort_by_key(|draw| draw.order);
        assert!(
            stable_scene_order(&mut draws, &mut FixedWriter::new(&mut indices), |draw| draw
                .order)
            .is_ok()
        );
        assert_eq!(draws, expected);
        assert_eq!(indices.capacity(), original_capacity);
    }
}

#[test]
fn insufficient_ordering_scratch_preserves_all_draws() {
    let mut draws = [3_u32, 1, 2];
    let mut indices = Vec::new();
    assert!(
        stable_scene_order(&mut draws, &mut FixedWriter::new(&mut indices), |draw| {
            *draw
        })
        .is_err()
    );
    assert_eq!(draws, [3, 1, 2]);
    assert!(indices.is_empty());
}
