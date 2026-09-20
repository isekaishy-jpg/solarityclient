//! Stable owned payloads preserve their address and charge across real worker transfer.

use solarity_cpu::{
    CpuError, CpuExecutionPlan, CpuExecutor, CpuOwnedCell, CpuPoolConfig, CpuStorageBudget,
    CpuStorageClass as Class, CpuStorageKind as Kind, CpuStoragePlan, FrameBatch, FrameBatchPlan,
};
use std::{cell::Cell, error::Error, num::NonZeroUsize};

#[test]
fn failed_admission_does_not_construct_or_take_inputs() {
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(7, 0, 0));
    let called = Cell::new(false);
    let result = CpuOwnedCell::new_with(&budget, Class::Frame, Kind::Scratch, || {
        called.set(true);
        17_u64
    });
    assert!(matches!(result, Err(CpuError::StorageAtCapacity { .. })));
    assert!(!called.get());
    assert_eq!(budget.snapshot().used(Class::Frame), 0);
}

#[test]
fn budget_transfer_preserves_address_and_refusal_preserves_ownership() -> Result<(), Box<dyn Error>>
{
    let source = CpuStorageBudget::new(CpuStoragePlan::new(8, 0, 0));
    let target = CpuStorageBudget::new(CpuStoragePlan::new(0, 8, 0));
    let mut cell = CpuOwnedCell::new_with(&source, Class::Frame, Kind::Scratch, || 17_u64)?;
    let address = std::ptr::from_ref(cell.value());
    assert!(cell.transfer(&target, Class::Frame, Kind::Scratch).is_err());
    assert_eq!(source.snapshot().used(Class::Frame), 8);
    assert_eq!(target.snapshot().used(Class::Required), 0);
    assert_eq!(*cell.value(), 17);
    cell.transfer(&target, Class::Required, Kind::Result)?;
    assert_eq!(std::ptr::from_ref(cell.value()), address);
    assert_eq!(source.snapshot().used(Class::Frame), 0);
    assert_eq!(target.snapshot().bytes(Class::Required, Kind::Result), 8);
    drop(cell);
    assert_eq!(target.snapshot().used(Class::Required), 0);
    Ok(())
}

#[test]
fn admitted_cell_stays_at_one_address_through_execution_consumption_and_reuse()
-> Result<(), Box<dyn Error>> {
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(0, 1, 1, 1)?,
        NonZeroUsize::MIN,
        CpuStoragePlan::new(1 << 20, 1 << 20, 0),
    ))?;
    let bytes = std::mem::size_of::<[usize; 512]>();
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(bytes, 0, 0));
    let mut cell = CpuOwnedCell::new_with(&budget, Class::Frame, Kind::Scratch, || [0_usize; 512])?;
    let address = std::ptr::from_ref(cell.value()).addr();
    cell.value_mut()[0] = address;
    let mut batch = FrameBatch::new(|cell: &mut CpuOwnedCell<[usize; 512]>| {
        assert_eq!(std::ptr::from_ref(cell.value()).addr(), cell.value()[0]);
        cell.value_mut()[1] += 1;
    });
    let mut returned = vec![cell];
    for count in 1..=8 {
        batch.begin(&cpu, FrameBatchPlan::new(1, 0))?;
        let mut owned = returned.pop();
        let handle = batch.push(&mut owned)?;
        assert!(owned.is_none());
        batch.close();
        batch.with_result(&handle, |cell| {
            assert_eq!(std::ptr::from_ref(cell.value()).addr(), address);
            assert_eq!(cell.value()[1], count);
        })?;
        batch.reclaim(&mut returned)?;
        assert_eq!(std::ptr::from_ref(returned[0].value()).addr(), address);
        assert_eq!(budget.snapshot().bytes(Class::Frame, Kind::Scratch), bytes);
    }
    drop(returned);
    assert_eq!(budget.snapshot().used(Class::Frame), 0);
    cpu.shutdown()?;
    Ok(())
}
