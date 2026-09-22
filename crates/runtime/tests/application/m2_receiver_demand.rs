//! Visible descendants retain ancestor callbacks across sparse frame reuse.

use super::{ReceiverFrame, RuntimeTerrainFrameError};
use glam::Vec3;

#[test]
fn receiver_demand_keeps_forward_ancestors_and_drops_previous_frame_consumers()
-> Result<(), RuntimeTerrainFrameError> {
    let mut frame = ReceiverFrame::default();
    let budget =
        solarity_cpu::CpuStorageBudget::new(solarity_cpu::CpuStoragePlan::new(1 << 20, 0, 0));
    for (index, parent) in [(1, Some(90)), (90, Some(3)), (3, None), (8, None)] {
        frame.record(&budget, index, parent, Vec3::ZERO, None)?;
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
    frame.record(&budget, 8, None, Vec3::X, None)?;
    frame.require(8);
    assert_eq!(&*frame.touched, &[8]);
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

#[test]
fn receiver_metadata_admits_together_and_preserves_recorded_demand_on_refusal()
-> Result<(), RuntimeTerrainFrameError> {
    use solarity_cpu::{CpuStorageBudget, CpuStorageClass as Class, CpuStoragePlan};
    let bytes = 4
        * (size_of::<Option<super::ReceiverRequest>>()
            + size_of::<usize>()
            + size_of::<bool>()
            + size_of::<Option<u32>>());
    let denied = CpuStorageBudget::new(CpuStoragePlan::new(bytes - 1, 0, 0));
    let mut frame = ReceiverFrame::default();
    assert!(frame.prepare_storage(&denied, 4).is_err());
    assert_eq!(frame.requests.capacity(), 0);
    assert_eq!(frame.touched.capacity(), 0);
    assert_eq!(frame.needed.capacity(), 0);
    assert_eq!(frame.remap.capacity(), 0);
    assert_eq!(denied.snapshot().used(Class::Frame), 0);
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(bytes, 0, 0));
    frame.prepare_storage(&budget, 4)?;
    frame.record(&budget, 2, Some(1), Vec3::X, None)?;
    frame.record(&budget, 1, None, Vec3::Y, None)?;
    frame.require(2);
    let addresses = (
        frame.requests.as_ptr(),
        frame.touched.as_ptr(),
        frame.needed.as_ptr(),
        frame.remap.as_ptr(),
    );
    assert!(frame.record(&budget, 4, None, Vec3::ZERO, None).is_err());
    assert_eq!(&*frame.touched, &[2, 1]);
    assert!(frame.needed[1] && frame.needed[2]);
    frame.prepare_storage(&budget, 4)?;
    assert_eq!(
        addresses,
        (
            frame.requests.as_ptr(),
            frame.touched.as_ptr(),
            frame.needed.as_ptr(),
            frame.remap.as_ptr()
        )
    );
    frame.clear();
    assert!(frame.requests.iter().all(Option::is_none));
    assert_eq!(budget.snapshot().used(Class::Frame), bytes);
    drop(frame);
    assert_eq!(budget.snapshot().used(Class::Frame), 0);
    Ok(())
}
