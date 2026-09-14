//! Visible descendants retain ancestor callbacks across sparse frame reuse.

use super::{ReceiverFrame, RuntimeTerrainFrameError};
use glam::Vec3;

#[test]
fn receiver_demand_keeps_forward_ancestors_and_drops_previous_frame_consumers()
-> Result<(), RuntimeTerrainFrameError> {
    let mut frame = ReceiverFrame::default();
    for (index, parent) in [(1, Some(90)), (90, Some(3)), (3, None), (8, None)] {
        frame.record(index, parent, Vec3::ZERO, None)?;
    }
    frame.require(1);
    let selected: Vec<_> = frame
        .touched
        .iter()
        .copied()
        .filter(|i| frame.needed[*i])
        .collect();
    assert_eq!(
        selected,
        [1, 90, 3],
        "visible child retains both forward and earlier ancestors"
    );
    assert!(matches!(
        frame.resolved(8),
        Err(RuntimeTerrainFrameError::MissingM2Receiver { placement: 8 })
    ));
    frame.clear();
    frame.record(8, None, Vec3::X, None)?;
    frame.require(8);
    assert_eq!(frame.touched, [8]);
    for old in [1, 90, 3] {
        assert!(!frame.needed[old]);
        assert!(frame.requests[old].is_none());
    }
    frame.remap[8] = Some(0);
    assert_eq!(frame.resolved(8)?, 0);
    frame.clear();
    assert!(
        frame.resolved(8).is_err(),
        "old compact scene IDs cannot cross frame boundaries"
    );
    Ok(())
}
