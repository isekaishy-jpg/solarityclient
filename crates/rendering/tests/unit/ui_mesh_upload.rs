//! Changed-range regressions for retained UI upload storage.

use super::replace_payload;

#[test]
fn retained_payload_limits_updates_to_aligned_changed_bytes() {
    let mut retained = (0_u8..32).collect::<Vec<_>>();
    let mut candidate = retained.clone();
    candidate[10] = 200;
    candidate[13] = 201;

    assert_eq!(replace_payload(&mut retained, &candidate), Some((8, 16)));
    assert_eq!(retained, candidate);
    assert_eq!(replace_payload(&mut retained, &candidate), None);
}

#[test]
fn retained_payload_handles_growth_and_logical_shrink() {
    let mut retained = vec![1_u8; 8];
    let candidate = vec![1_u8; 16];
    assert_eq!(replace_payload(&mut retained, &candidate), Some((8, 16)));
    assert_eq!(retained, candidate);

    assert_eq!(replace_payload(&mut retained, &[1_u8; 4]), None);
    assert_eq!(retained, [1_u8; 4]);
}

#[test]
fn changing_existing_vertices_while_appending_keeps_the_complete_tail() {
    let mut retained = vec![1_u8; 8];
    let candidate = [2_u8; 16];
    assert_eq!(replace_payload(&mut retained, &candidate), Some((0, 16)));
    assert_eq!(retained, candidate);
}
