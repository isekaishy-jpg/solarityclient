//! Byte admission, peak growth and shared result lifetimes through public APIs.

use solarity_cpu::{
    CompletionPort, CpuError, CpuExecutor, CpuPoolConfig, CpuResultPage, CpuStorageBudget,
    CpuStorageClass as Class, CpuStorageKind as Kind, CpuStoragePlan, FrameBatch, FrameBatchPlan,
};
use std::{error::Error, num::NonZeroUsize};

/// Minimal pool leaves ample frame metadata space for pressure-controlled tests.
fn executor() -> Result<CpuExecutor, CpuError> {
    CpuExecutor::new(CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::MIN;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
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
    let mut cpu = executor()?;
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
    // Reclaim returns inputs before the runner drops its final Core reference.
    // Shutdown joins that epilogue; batch drop intentionally does not park a worker.
    cpu.shutdown()?;
    assert_eq!(cpu.storage().snapshot().used(Class::Frame), baseline);
    Ok(())
}

#[test]
fn reusing_a_batch_on_another_executor_moves_retained_capacity() -> Result<(), Box<dyn Error>> {
    let mut first = executor()?;
    let mut second = executor()?;
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
    // Either executor can still hold the shared owner in its terminal epilogue.
    first.shutdown()?;
    second.shutdown()?;
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
            {
                let total: std::num::NonZeroUsize = NonZeroUsize::MIN;
                solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                    .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
            },
            NonZeroUsize::MIN,
            CpuStoragePlan::new(0, 0, 0),
        )),
        Err(CpuError::StorageAtCapacity { .. })
    ));
}

#[test]
fn domain_writers_cannot_grow_and_draining_retains_their_charge() -> Result<(), Box<dyn Error>> {
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(16, 0, 0));
    let mut buffer = solarity_cpu::CpuBuffer::<u64>::default();
    buffer.reserve(&budget, Class::Frame, Kind::Result, 2)?;
    buffer.extend_from_slice(&[5, 7])?;
    assert!(matches!(
        buffer.writer().push(9),
        Err(CpuError::OutputCapacity {
            requested: 1,
            available: 0
        })
    ));
    assert!(matches!(
        buffer.writer().require(1),
        Err(CpuError::OutputCapacity { .. })
    ));
    assert_eq!(&*buffer, &[5, 7]);
    assert_eq!(budget.snapshot().used(Class::Frame), 16);
    assert_eq!(buffer.drain().collect::<Vec<_>>(), [5, 7]);
    assert_eq!(budget.snapshot().used(Class::Frame), 16);
    buffer.push(11)?;
    buffer.clear();
    assert_eq!(budget.snapshot().used(Class::Frame), 16);
    drop(buffer);
    assert_eq!(budget.snapshot().used(Class::Frame), 0);
    Ok(())
}

#[test]
fn connected_working_set_keeps_headroom_protected_until_all_allocations_are_funded()
-> Result<(), Box<dyn Error>> {
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(128, 0, 0));
    let mut working_set = budget.reserve_working_set(Class::Frame, 96)?;
    let competitor = budget.reserve(Class::Frame, Kind::Scratch, 32)?;
    assert!(budget.reserve(Class::Frame, Kind::Result, 1).is_err());
    let mut output = working_set.reserve(Kind::Result, 64)?;
    let scratch = working_set.reserve(Kind::Scratch, 16)?;
    assert_eq!(working_set.remaining(), 16);
    assert_eq!(budget.snapshot().used(Class::Frame), 128);
    let identity = output.allocation_id();
    output.resize_reserved(&mut working_set, 72)?;
    assert_eq!(output.allocation_id(), identity);
    assert_eq!(working_set.remaining(), 8);
    assert!(output.resize_reserved(&mut working_set, 81).is_err());
    assert_eq!(output.bytes(), 72);
    assert_eq!(working_set.remaining(), 8);
    assert!(budget.reserve(Class::Frame, Kind::Result, 1).is_err());
    drop(working_set);
    assert_eq!(budget.snapshot().used(Class::Frame), 120);
    drop((output, scratch, competitor));
    assert_eq!(budget.snapshot().used(Class::Frame), 0);
    Ok(())
}

#[test]
fn funded_transfer_preserves_allocation_identity_and_rejects_underestimated_adoption()
-> Result<(), Box<dyn Error>> {
    let original = CpuStorageBudget::new(CpuStoragePlan::new(32, 0, 0));
    let destination = CpuStorageBudget::new(CpuStoragePlan::new(64, 0, 0));
    let mut allocation = original.reserve(Class::Frame, Kind::Result, 32)?;
    let identity = allocation.allocation_id();
    let mut too_small = destination.reserve_working_set(Class::Frame, 31)?;
    assert!(
        allocation
            .transfer_reserved(&mut too_small, Kind::Result)
            .is_err()
    );
    assert_eq!(allocation.allocation_id(), identity);
    assert_eq!(original.snapshot().used(Class::Frame), 32);
    assert_eq!(too_small.remaining(), 31);
    drop(too_small);
    let mut admitted = destination.reserve_working_set(
        Class::Frame,
        allocation.admission_bytes(&destination, Class::Frame),
    )?;
    allocation.transfer_reserved(&mut admitted, Kind::Result)?;
    assert_eq!(allocation.allocation_id(), identity);
    assert_eq!(original.snapshot().used(Class::Frame), 0);
    assert_eq!(admitted.remaining(), 0);
    assert_eq!(allocation.admission_bytes(&destination, Class::Frame), 0);
    assert_eq!(destination.snapshot().used(Class::Frame), 32);
    drop((allocation, admitted));
    assert_eq!(destination.snapshot().used(Class::Frame), 0);
    Ok(())
}

#[test]
fn connected_buffers_grow_from_reserved_capacity_and_warm_reuse_needs_no_headroom()
-> Result<(), Box<dyn Error>> {
    use solarity_cpu::{CpuBuffer, CpuScratch};
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(128, 0, 0));
    let mut output = CpuBuffer::<u64>::default();
    let mut sorting = CpuScratch::<usize>::default();
    let bytes = output.reservation_bytes(&budget, Class::Frame, 8)?
        + sorting.reservation_bytes(&budget, Class::Frame, 4)?;
    let mut admitted = budget.reserve_working_set(Class::Frame, bytes)?;
    let competitor = budget.reserve(Class::Frame, Kind::Scratch, 128 - bytes)?;
    output.reserve_reserved(&mut admitted, Kind::Result, 8)?;
    sorting.reserve_reserved(&mut admitted, 4)?;
    output.extend_from_slice(&[17, 23])?;
    assert_eq!(admitted.remaining(), 0);
    let identity = output.as_ptr();
    assert_eq!(output.reservation_bytes(&budget, Class::Frame, 8)?, 0);
    assert_eq!(sorting.reservation_bytes(&budget, Class::Frame, 4)?, 0);
    let mut warm = budget.reserve_working_set(Class::Frame, 0)?;
    output.reserve_reserved(&mut warm, Kind::Result, 8)?;
    sorting.reserve_reserved(&mut warm, 4)?;
    assert_eq!(output.as_ptr(), identity);
    assert_eq!(&*output, &[17, 23]);
    assert_eq!(budget.snapshot().used(Class::Frame), 128);
    drop((output, sorting, admitted, warm, competitor));
    assert_eq!(budget.snapshot().used(Class::Frame), 0);
    Ok(())
}

#[test]
fn sequential_replacements_recycle_old_capacity_without_overreserving_the_phase()
-> Result<(), Box<dyn Error>> {
    use solarity_cpu::{CpuBuffer, CpuStorageWorkingSet};
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(160, 0, 0));
    let mut first = CpuBuffer::<u64>::default();
    let mut second = CpuBuffer::<u64>::default();
    first.reserve(&budget, Class::Frame, Kind::Result, 4)?;
    second.reserve(&budget, Class::Frame, Kind::Result, 4)?;
    first.extend_from_slice(&[1, 2])?;
    second.extend_from_slice(&[3, 4])?;
    let mut plan = CpuStorageWorkingSet::default();
    plan.include(
        first.reservation_bytes(&budget, Class::Frame, 8)?,
        first.replacement_credit(8),
    )?;
    plan.include(
        second.reservation_bytes(&budget, Class::Frame, 8)?,
        second.replacement_credit(8),
    )?;
    assert_eq!(
        plan.bytes(),
        96,
        "reuse needs less than the sum of both replacement allocations"
    );
    let mut admitted = budget.reserve_working_set(Class::Frame, plan.bytes())?;
    first.reserve_reserved(&mut admitted, Kind::Result, 8)?;
    assert!(
        budget.reserve(Class::Frame, Kind::Scratch, 1).is_err(),
        "retired capacity stays protected for the second output"
    );
    second.reserve_reserved(&mut admitted, Kind::Result, 8)?;
    assert_eq!(&*first, &[1, 2]);
    assert_eq!(&*second, &[3, 4]);
    assert_eq!(admitted.remaining(), 32);
    drop(admitted);
    assert_eq!(budget.snapshot().used(Class::Frame), 128);
    drop((first, second));
    assert_eq!(budget.snapshot().used(Class::Frame), 0);
    Ok(())
}

#[test]
fn connected_scheduler_and_domain_admission_refuses_before_either_grows()
-> Result<(), Box<dyn Error>> {
    use solarity_cpu::{FrameGraphTemplate, JobOutcome};
    let mut cpu = executor()?;
    let baseline = cpu.storage().snapshot().used(Class::Frame);
    let domain_bytes = 4096;
    let pressure = cpu.storage().reserve(
        Class::Frame,
        Kind::Scratch,
        cpu.storage().snapshot().limit(Class::Frame) - baseline - domain_bytes,
    )?;
    let held = cpu.storage().snapshot().used(Class::Frame);
    let mut batch = FrameBatch::with_context(|value: &mut u64, _| {
        *value += 1;
        JobOutcome::Succeeded
    });
    let template = FrameGraphTemplate::independent(1);
    let mut inputs = vec![5];
    let mut prepared = false;
    let result = batch.start_costed_graph_with_storage(
        &cpu,
        &template,
        &mut inputs,
        &[],
        &[],
        domain_bytes,
        |_, _| {
            prepared = true;
            Ok(())
        },
    );
    assert!(matches!(result, Err(CpuError::StorageAtCapacity { .. })));
    assert!(!prepared);
    assert_eq!(inputs, [5]);
    assert_eq!(cpu.storage().snapshot().used(Class::Frame), held);
    assert!(batch.is_finished());
    drop(pressure);

    let mut output = None;
    batch.start_costed_graph_with_storage(
        &cpu,
        &template,
        &mut inputs,
        &[],
        &[],
        domain_bytes,
        |inputs, fund| {
            assert_eq!(inputs, &[5]);
            let snapshot = cpu.storage().snapshot();
            let competitor = cpu.storage().reserve(
                Class::Frame,
                Kind::Scratch,
                snapshot.limit(Class::Frame) - snapshot.used(Class::Frame),
            )?;
            assert!(
                cpu.storage()
                    .reserve(Class::Frame, Kind::Result, 1)
                    .is_err()
            );
            output = Some(fund.reserve(Kind::Result, domain_bytes)?);
            drop(competitor);
            Ok(())
        },
    )?;
    assert!(inputs.is_empty());
    batch.reclaim(&mut inputs)?;
    assert_eq!(inputs, [6]);
    drop(output);
    let warm = cpu.storage().snapshot().used(Class::Frame);
    let pressure = cpu.storage().reserve(
        Class::Frame,
        Kind::Scratch,
        cpu.storage().snapshot().limit(Class::Frame) - warm,
    )?;
    batch.start(&cpu, &mut inputs)?;
    batch.reclaim(&mut inputs)?;
    assert_eq!(inputs, [7]);
    drop((batch, pressure));
    cpu.shutdown()?;
    assert_eq!(cpu.storage().snapshot().used(Class::Frame), baseline);
    Ok(())
}

#[test]
fn connected_preparation_error_unwind_and_count_change_retire_empty_gated_epochs()
-> Result<(), Box<dyn Error>> {
    use solarity_cpu::{FrameGraphTemplate, FramePriority, JobOutcome};
    use std::panic::{AssertUnwindSafe, catch_unwind};
    let mut cpu = executor()?;
    let mut port = CompletionPort::new(1, cpu.storage(), Class::Frame)?;
    let template = FrameGraphTemplate::independent(1).with_priority(FramePriority::Prerequisite);
    let mut batch = FrameBatch::new(|value: &mut u64| *value += 1);
    let mut inputs = vec![11];
    let result = batch.start_costed_graph_with_storage(
        &cpu,
        &template,
        &mut inputs,
        &[port.readiness()],
        &[],
        16,
        |_, _| Err(CpuError::StorageAllocation),
    );
    assert!(matches!(result, Err(CpuError::StorageAllocation)));
    assert!(matches!(batch.completion(), Err(CpuError::BatchInactive)));
    assert_eq!(inputs, [11]);
    assert!(batch.is_finished());
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let _ = batch.start_costed_graph_with_storage(
                &cpu,
                &template,
                &mut inputs,
                &[port.readiness()],
                &[],
                16,
                |_, _| panic!("preparation unwinds before binding"),
            );
        }))
        .is_err()
    );
    assert!(matches!(batch.completion(), Err(CpuError::BatchInactive)));
    assert_eq!(inputs, [11]);
    let result = batch.start_costed_graph_with_storage(
        &cpu,
        &template,
        &mut inputs,
        &[port.readiness()],
        &[],
        0,
        |inputs, _| {
            inputs.push(17);
            Ok(())
        },
    );
    assert!(matches!(result, Err(CpuError::GraphInputCount)));
    assert_eq!(inputs, [11, 17]);
    // Every failed binding unsubscribed without cancelling the shared producer.
    port.producer()?.complete(JobOutcome::Succeeded)?;
    port.restart(1, cpu.storage(), Class::Frame)?;
    batch.start_graph(
        &cpu,
        &FrameGraphTemplate::independent(2),
        &mut inputs,
        &[port.readiness()],
    )?;
    port.producer()?.complete(JobOutcome::Succeeded)?;
    batch.reclaim(&mut inputs)?;
    assert_eq!(inputs, [12, 18]);
    drop((batch, port));
    cpu.shutdown()?;
    Ok(())
}
