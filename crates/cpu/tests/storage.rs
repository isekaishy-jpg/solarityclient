//! Byte admission, peak growth and shared result lifetimes through public APIs.

use solarity_cpu::{
    CompletionPort, CpuError, CpuExecutor, CpuPoolConfig, CpuResultPage, CpuStorageBudget,
    CpuStorageClass as Class, CpuStorageKind as Kind, CpuStoragePlan, FrameBatch, FrameBatchPlan,
};
use std::{error::Error, num::NonZeroUsize};

/// Minimal pool leaves ample frame metadata space for pressure-controlled tests.
fn executor() -> Result<CpuExecutor, CpuError> {
    CpuExecutor::new(CpuPoolConfig::new(
        NonZeroUsize::MIN,
        NonZeroUsize::MIN,
        CpuStoragePlan::new(1 << 20, 1 << 20, 0),
    ))
}

#[test]
fn classes_are_isolated_and_failed_transfer_preserves_the_original_charge()
-> Result<(), Box<dyn Error>> {
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(16, 8, 4));
    let mut memory = budget.reserve(Class::Frame, Kind::Scratch, 16)?;
    let id = memory.allocation_id();
    let speculative = budget.reserve(Class::Speculative, Kind::Result, 4)?;
    assert!(matches!(
        budget.reserve(Class::Speculative, Kind::Result, 1),
        Err(CpuError::StorageAtCapacity { available: 0, .. })
    ));
    assert!(
        memory
            .transfer(&budget, Class::Required, Kind::Result)
            .is_err()
    );
    assert_eq!(memory.allocation_id(), id);
    assert_eq!(budget.snapshot().bytes(Class::Frame, Kind::Scratch), 16);
    memory.transfer(&budget, Class::Frame, Kind::Result)?;
    assert_eq!(budget.snapshot().bytes(Class::Frame, Kind::Scratch), 0);
    assert_eq!(budget.snapshot().bytes(Class::Frame, Kind::Result), 16);
    assert_eq!(budget.snapshot().peak(Class::Frame), 16);
    drop((memory, speculative));
    assert_eq!(budget.snapshot().used(Class::Frame), 0);
    assert_eq!(budget.snapshot().used(Class::Speculative), 0);
    Ok(())
}

#[test]
fn replacement_growth_reserves_old_and_new_capacity_before_moving_values()
-> Result<(), Box<dyn Error>> {
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(23, 0, 0));
    let mut page = CpuResultPage::<u64>::new(&budget, Class::Frame, 1)?;
    page.try_push(7).map_err(|_| "page full")?;
    let id = page.allocation_id();
    assert!(matches!(
        page.reserve(2),
        Err(CpuError::StorageAtCapacity {
            requested: 16,
            available: 15,
            ..
        })
    ));
    assert_eq!(&*page, &[7]);
    assert_eq!(page.allocation_id(), id);
    assert_eq!(budget.snapshot().used(Class::Frame), 8);
    let larger = CpuStorageBudget::new(CpuStoragePlan::new(24, 0, 0));
    page.transfer(&larger, Class::Frame)?;
    assert_eq!(budget.snapshot().used(Class::Frame), 0);
    assert_eq!(page.allocation_id(), id);
    page.reserve(2)?;
    assert_ne!(page.allocation_id(), id);
    assert_eq!(larger.snapshot().peak(Class::Frame), 24);
    assert_eq!(larger.snapshot().used(Class::Frame), 16);
    assert_eq!(&*page, &[7]);
    page.clear();
    assert_eq!(larger.snapshot().used(Class::Frame), 16);
    drop(page);
    assert_eq!(larger.snapshot().used(Class::Frame), 0);
    Ok(())
}

#[test]
fn immutable_consumers_share_one_charge_until_the_last_lease_leaves() -> Result<(), Box<dyn Error>>
{
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(16, 0, 0));
    let mut page = CpuResultPage::<u64>::new(&budget, Class::Frame, 2)?;
    page.try_push(11).map_err(|_| "page full")?;
    let first = page.freeze();
    let second = first.clone();
    assert_eq!(first.allocation_id(), second.allocation_id());
    assert_eq!(budget.snapshot().used(Class::Frame), 16);
    let first = first
        .try_reclaim()
        .err()
        .ok_or("other consumer must prevent mutation")?;
    drop(first);
    assert_eq!(&*second, &[11]);
    let mut page = second
        .try_reclaim()
        .map_err(|_| "last consumer must reclaim")?;
    page.clear();
    assert_eq!(budget.snapshot().used(Class::Frame), 16);
    drop(page);
    assert_eq!(budget.snapshot().used(Class::Frame), 0);
    Ok(())
}

#[test]
fn cross_budget_rejection_never_loses_values_or_their_source_charge() -> Result<(), Box<dyn Error>>
{
    let source = CpuStorageBudget::new(CpuStoragePlan::new(8, 0, 0));
    let destination = CpuStorageBudget::new(CpuStoragePlan::new(0, 7, 0));
    let mut page = CpuResultPage::<u64>::new(&source, Class::Frame, 1)?;
    page.try_push(3).map_err(|_| "page full")?;
    assert!(page.transfer(&destination, Class::Required).is_err());
    assert_eq!(&*page, &[3]);
    assert_eq!(source.snapshot().used(Class::Frame), 8);
    assert_eq!(destination.snapshot().used(Class::Required), 0);
    Ok(())
}

#[test]
fn frame_pressure_rejects_before_input_transfer_and_warm_storage_stays_charged()
-> Result<(), Box<dyn Error>> {
    let cpu = executor()?;
    let baseline = cpu.storage().snapshot().used(Class::Frame);
    let remaining = cpu.storage().snapshot().limit(Class::Frame) - baseline;
    let pressure = cpu
        .storage()
        .reserve(Class::Frame, Kind::Scratch, remaining)?;
    let mut batch = FrameBatch::new(|value: &mut u64| *value += 1);
    let mut inputs = vec![5];
    assert!(matches!(
        batch.start(&cpu, &mut inputs),
        Err(CpuError::StorageAtCapacity { .. })
    ));
    assert_eq!(inputs, [5]);
    drop(pressure);
    batch.start(&cpu, &mut inputs)?;
    batch.reclaim(&mut inputs)?;
    assert_eq!(inputs, [6]);
    let warm = cpu.storage().snapshot().used(Class::Frame);
    assert!(warm > baseline);
    batch.start(&cpu, &mut inputs)?;
    batch.reclaim(&mut inputs)?;
    assert_eq!(cpu.storage().snapshot().used(Class::Frame), warm);
    drop(batch);
    assert_eq!(cpu.storage().snapshot().used(Class::Frame), baseline);
    Ok(())
}

#[test]
fn reusing_a_batch_on_another_executor_moves_retained_capacity() -> Result<(), Box<dyn Error>> {
    let first = executor()?;
    let second = executor()?;
    let baseline = first.storage().snapshot().used(Class::Frame);
    let mut batch = FrameBatch::new(|_: &mut u64| {});
    let mut inputs = vec![1, 2, 3];
    batch.start(&first, &mut inputs)?;
    batch.reclaim(&mut inputs)?;
    let retained = first.storage().snapshot().used(Class::Frame) - baseline;
    batch.start(&second, &mut inputs)?;
    batch.reclaim(&mut inputs)?;
    assert_eq!(first.storage().snapshot().used(Class::Frame), baseline);
    assert_eq!(
        second.storage().snapshot().used(Class::Frame),
        baseline + retained
    );
    drop(batch);
    assert_eq!(second.storage().snapshot().used(Class::Frame), baseline);
    Ok(())
}

#[test]
fn shutdown_cancels_gates_but_keeps_input_and_port_storage_until_disposal()
-> Result<(), Box<dyn Error>> {
    let mut cpu = executor()?;
    let budget = cpu.storage().clone();
    let port = CompletionPort::new(1, &budget, Class::Frame)?;
    let mut batch = FrameBatch::new(|_: &mut u64| {});
    batch.begin_when(&cpu, FrameBatchPlan::new(1, 0), &port.readiness())?;
    let mut input = Some(12);
    batch.push(&mut input)?;
    cpu.shutdown()?;
    drop(cpu);
    assert!(budget.snapshot().used(Class::Frame) > 0);
    let mut returned = Vec::new();
    let _outcome = batch.reclaim(&mut returned);
    assert_eq!(returned, [12]);
    drop((batch, port));
    assert_eq!(budget.snapshot().used(Class::Frame), 0);
    Ok(())
}

#[test]
fn startup_byte_rejection_happens_before_workers_are_created() {
    assert!(matches!(
        CpuExecutor::new(CpuPoolConfig::new(
            NonZeroUsize::MIN,
            NonZeroUsize::MIN,
            CpuStoragePlan::new(0, 0, 0),
        )),
        Err(CpuError::StorageAtCapacity { .. })
    ));
}
