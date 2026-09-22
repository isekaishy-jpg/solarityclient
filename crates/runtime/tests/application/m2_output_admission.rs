//! Renderer-vector adoption and replacement obey CPU old-plus-new accounting.

use super::{replacement_credit, required_bytes};
use solarity_cpu::{ByteReservation, CpuError};

fn reserve<T>(
    values: &mut Vec<T>,
    charge: &mut Option<ByteReservation>,
    budget: &CpuStorageBudget,
    additional: usize,
) -> Result<(), CpuError> {
    let mut reservation = budget.reserve_working_set(
        Class::Frame,
        required_bytes(values, charge, budget, additional)?,
    )?;
    super::reserve(values, charge, &mut reservation, additional)
}
use solarity_cpu::{CpuStorageBudget, CpuStorageClass as Class, CpuStoragePlan};
use std::error::Error;

#[test]
fn adoption_and_refused_growth_keep_values_and_the_original_charge() -> Result<(), Box<dyn Error>> {
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(96, 0, 0));
    let mut values = Vec::with_capacity(4);
    values.extend([10_u64, 20, 30, 40]);
    let mut charge = None;
    reserve(&mut values, &mut charge, &budget, 0)?;
    let old_bytes = values.capacity() * size_of::<u64>();
    let old_identity = charge.as_ref().ok_or("charge")?.allocation_id();
    assert_eq!(budget.snapshot().used(Class::Frame), old_bytes);
    // A 12-element replacement plus the retained four-element buffer cannot fit.
    assert!(reserve(&mut values, &mut charge, &budget, 8).is_err());
    assert_eq!(values, [10, 20, 30, 40]);
    assert_eq!(values.capacity() * size_of::<u64>(), old_bytes);
    assert_eq!(
        charge.as_ref().ok_or("charge")?.allocation_id(),
        old_identity
    );
    assert_eq!(budget.snapshot().used(Class::Frame), old_bytes);
    let larger = CpuStorageBudget::new(CpuStoragePlan::new(256, 0, 0));
    reserve(&mut values, &mut charge, &larger, 8)?;
    assert_eq!(values, [10, 20, 30, 40]);
    assert!(values.capacity() >= 12);
    assert_eq!(budget.snapshot().used(Class::Frame), 0);
    assert_eq!(
        larger.snapshot().used(Class::Frame),
        values.capacity() * size_of::<u64>()
    );
    assert_ne!(
        charge.as_ref().ok_or("charge")?.allocation_id(),
        old_identity
    );
    drop(values);
    drop(charge);
    assert_eq!(larger.snapshot().used(Class::Frame), 0);
    Ok(())
}

#[test]
fn refused_executor_transfer_keeps_the_previous_storage_owner() -> Result<(), Box<dyn Error>> {
    let original = CpuStorageBudget::new(CpuStoragePlan::new(256, 0, 0));
    let refused = CpuStorageBudget::new(CpuStoragePlan::new(0, 0, 0));
    let mut values = vec![7_u64, 8, 9];
    let mut charge = None;
    reserve(&mut values, &mut charge, &original, 0)?;
    let identity = charge.as_ref().ok_or("charge")?.allocation_id();
    assert!(reserve(&mut values, &mut charge, &refused, 0).is_err());
    assert_eq!(values, [7, 8, 9]);
    assert_eq!(charge.as_ref().ok_or("charge")?.allocation_id(), identity);
    assert_eq!(refused.snapshot().used(Class::Frame), 0);
    assert_eq!(
        original.snapshot().used(Class::Frame),
        values.capacity() * size_of::<u64>()
    );
    Ok(())
}

#[test]
fn connected_output_refusal_preserves_every_buffer_before_any_growth() -> Result<(), Box<dyn Error>>
{
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(150, 0, 0));
    let mut first = vec![1u64, 2, 3, 4];
    let mut second = vec![5u64, 6, 7, 8];
    let mut first_charge = None;
    let mut second_charge = None;
    reserve(&mut first, &mut first_charge, &budget, 0)?;
    reserve(&mut second, &mut second_charge, &budget, 0)?;
    let identities = (first.as_ptr(), second.as_ptr());
    let capacities = (first.capacity(), second.capacity());
    let mut working_set = solarity_cpu::CpuStorageWorkingSet::default();
    working_set.include(
        required_bytes(&first, &first_charge, &budget, 4)?,
        replacement_credit(&first, 4)?,
    )?;
    working_set.include(
        required_bytes(&second, &second_charge, &budget, 4)?,
        replacement_credit(&second, 4)?,
    )?;
    let required = working_set.bytes();
    // Each individual replacement fits; their connected working set does not.
    assert!(budget.reserve_working_set(Class::Frame, required).is_err());
    assert_eq!((first.as_ptr(), second.as_ptr()), identities);
    assert_eq!((first.capacity(), second.capacity()), capacities);
    assert_eq!(first, [1, 2, 3, 4]);
    assert_eq!(second, [5, 6, 7, 8]);
    assert_eq!(budget.snapshot().used(Class::Frame), 64);
    let larger = CpuStorageBudget::new(CpuStoragePlan::new(256, 0, 0));
    let mut working_set = solarity_cpu::CpuStorageWorkingSet::default();
    working_set.include(
        required_bytes(&first, &first_charge, &larger, 4)?,
        replacement_credit(&first, 4)?,
    )?;
    working_set.include(
        required_bytes(&second, &second_charge, &larger, 4)?,
        replacement_credit(&second, 4)?,
    )?;
    let required = working_set.bytes();
    let mut admitted = larger.reserve_working_set(Class::Frame, required)?;
    let competing = larger.reserve(
        Class::Frame,
        solarity_cpu::CpuStorageKind::Scratch,
        256 - required,
    )?;
    super::reserve(&mut first, &mut first_charge, &mut admitted, 4)?;
    super::reserve(&mut second, &mut second_charge, &mut admitted, 4)?;
    assert_eq!(budget.snapshot().used(Class::Frame), 0);
    assert_eq!(admitted.remaining(), 32);
    assert_eq!(first, [1, 2, 3, 4]);
    assert_eq!(second, [5, 6, 7, 8]);
    drop((
        competing,
        admitted,
        first,
        second,
        first_charge,
        second_charge,
    ));
    assert_eq!(larger.snapshot().used(Class::Frame), 0);
    Ok(())
}

#[test]
fn finalization_refuses_scheduler_and_aggregate_outputs_as_one_phase() -> Result<(), Box<dyn Error>>
{
    use super::super::{Finalization, FinalizationJob};
    use solarity_cpu::{
        CpuExecutionPlan, CpuExecutor, CpuOwnedCell, CpuPoolConfig, CpuStorageKind as Kind,
    };
    use std::num::NonZeroUsize;
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(0, 1, 1, 1)?,
        NonZeroUsize::MIN,
        CpuStoragePlan::new(1 << 20, 1 << 20, 0),
    ))?;
    let budget = cpu.storage().clone();
    let baseline = budget.snapshot().used(Class::Frame);
    let mut phase = Finalization::default();
    phase
        .retained
        .reserve(&budget, Class::Frame, Kind::Metadata, 1)?;
    phase.retained.push(CpuOwnedCell::new_with(
        &budget,
        Class::Frame,
        Kind::Scratch,
        || FinalizationJob {
            first_pass: Some(solarity_rendering::M2TransparentPass::One),
            ..Default::default()
        },
    )?)?;
    phase.owns_inputs = true;
    let counts = super::OutputCounts {
        visible_draws: 4,
        particle_vertices: 12,
        ribbon_vertices: 8,
        ..Default::default()
    };
    let job = phase.retained[0].value();
    let outputs = job
        .memory
        .reservation_bytes(&job.streams, &budget, &job.sorting, counts)?;
    let pressure = budget.reserve(
        Class::Frame,
        Kind::Scratch,
        budget.snapshot().limit(Class::Frame) - budget.snapshot().used(Class::Frame) - outputs,
    )?;
    let held = budget.snapshot().used(Class::Frame);
    assert!(matches!(
        phase.start(&cpu, counts),
        Err(CpuError::StorageAtCapacity { .. })
    ));
    assert!(!phase.submitted);
    assert!(phase.owns_inputs);
    assert_eq!(phase.retained.len(), 1);
    let job = phase.retained[0].value();
    assert_eq!(job.streams.visible_draws.capacity(), 0);
    assert_eq!(job.streams.particle_vertices.capacity(), 0);
    assert_eq!(job.streams.ribbon_vertices.capacity(), 0);
    assert_eq!(budget.snapshot().used(Class::Frame), held);
    drop(pressure);
    phase.start(&cpu, counts)?;
    phase.pending.reclaim_into(&mut phase.retained.writer())?;
    phase.submitted = false;
    let job = phase.retained[0].value();
    assert!(job.result.as_ref().is_some_and(Result::is_ok));
    let addresses = (
        job.streams.visible_draws.as_ptr(),
        job.streams.particle_vertices.as_ptr(),
        job.streams.ribbon_vertices.as_ptr(),
    );
    let warm = budget.snapshot().used(Class::Frame);
    let pressure = budget.reserve(
        Class::Frame,
        Kind::Scratch,
        budget.snapshot().limit(Class::Frame) - warm,
    )?;
    phase.start(&cpu, counts)?;
    phase.pending.reclaim_into(&mut phase.retained.writer())?;
    let job = phase.retained[0].value();
    assert_eq!(
        addresses,
        (
            job.streams.visible_draws.as_ptr(),
            job.streams.particle_vertices.as_ptr(),
            job.streams.ribbon_vertices.as_ptr()
        )
    );
    drop((phase, pressure));
    cpu.shutdown()?;
    assert_eq!(budget.snapshot().used(Class::Frame), baseline);
    Ok(())
}
