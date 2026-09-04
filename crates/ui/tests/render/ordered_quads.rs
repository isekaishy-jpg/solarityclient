//! Exact painter order is unchanged when glyph runs replace a full-quad sort.

use super::{UiGlyphDrawRun, UiOrderedQuadSlot, UiOrderedQuadSlots, UiQuadOrder};

/// Textures can fall inside a glyph run's sequence range or tie a glyph's key.
#[test]
fn ordered_runs_match_stable_quad_sort_with_interleaved_texture_sequences() {
    let textures = vec![
        order(2, 12),
        order(0, 0),
        order(2, 11),
        order(2, 12),
        order(4, 3),
    ];
    let runs = vec![
        UiGlyphDrawRun {
            next_order: order(2, 10),
            range: 0..5,
        },
        UiGlyphDrawRun {
            next_order: order(1, 5),
            range: 5..8,
        },
        UiGlyphDrawRun {
            next_order: order(2, 18),
            range: 8..12,
        },
    ];
    let mut reference = textures
        .iter()
        .copied()
        .enumerate()
        .map(|(index, key)| (key, UiOrderedQuadSlot::Texture(index)))
        .collect::<Vec<_>>();
    for run in &runs {
        for (offset, index) in run.range.clone().enumerate() {
            let mut key = run.next_order;
            key.3 += offset;
            reference.push((key, UiOrderedQuadSlot::Glyph(index)));
        }
    }
    reference.sort_by_key(|(key, _)| *key);
    let mut actual = UiOrderedQuadSlots::from_runs(textures, runs);
    let count = reference.len();
    for (index, (_, expected)) in reference.into_iter().enumerate() {
        assert_eq!(actual.len(), count - index);
        assert_eq!(actual.size_hint(), (count - index, Some(count - index)));
        assert_eq!(actual.next(), Some(expected));
    }
    assert_eq!(actual.len(), 0);
    assert_eq!(actual.next(), None);
    assert_eq!(actual.next(), None);
}

/// Missing source classes and an empty scene preserve the exact mesh count contract.
#[test]
fn ordered_runs_support_empty_texture_only_and_glyph_only_meshes() {
    let mut empty = UiOrderedQuadSlots::from_runs(Vec::new(), Vec::new());
    assert_eq!(empty.len(), 0);
    assert_eq!(empty.next(), None);
    assert_eq!(
        UiOrderedQuadSlots::from_runs(vec![order(4, 0), order(1, 1)], Vec::new())
            .collect::<Vec<_>>(),
        [UiOrderedQuadSlot::Texture(1), UiOrderedQuadSlot::Texture(0)]
    );
    assert_eq!(
        UiOrderedQuadSlots::from_runs(
            Vec::new(),
            vec![
                UiGlyphDrawRun {
                    next_order: order(4, 0),
                    range: 0..2
                },
                UiGlyphDrawRun {
                    next_order: order(1, 2),
                    range: 2..3
                },
            ]
        )
        .collect::<Vec<_>>(),
        [
            UiOrderedQuadSlot::Glyph(2),
            UiOrderedQuadSlot::Glyph(0),
            UiOrderedQuadSlot::Glyph(1)
        ]
    );
}

/// Ownerless packets exercise object and source-sequence tie breakers directly.
fn order(owner: usize, sequence: usize) -> UiQuadOrder {
    (true, None, owner, sequence)
}
