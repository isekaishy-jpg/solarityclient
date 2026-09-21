//! Warm admission must not contend on the accounting mutex without a byte change.
use super::*;
use crate::{CpuBuffer, CpuScratch, CpuStoragePlan};
use std::error::Error;

#[test]
fn warmed_connected_storage_performs_no_ledger_transactions() -> Result<(), Box<dyn Error>> {
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(128, 0, 0));
    let mut output = CpuBuffer::<u64>::default();
    let mut scratch = CpuScratch::<usize>::default();
    output.reserve(&budget, CpuStorageClass::Frame, CpuStorageKind::Result, 8)?;
    scratch.reserve(&budget, CpuStorageClass::Frame, 4)?;
    output.extend_from_slice(&[17, 23])?;
    let mut memory = budget.reserve(CpuStorageClass::Frame, CpuStorageKind::Result, 8)?;
    let hold = budget.reserve(CpuStorageClass::Frame, CpuStorageKind::Scratch, 24)?;
    let identity = memory.allocation_id();
    let address = output.as_ptr();
    LEDGER_LOCKS.with(|locks| locks.set(0));
    for _ in 0..1000 {
        let mut reservation = budget.reserve_working_set(CpuStorageClass::Frame, 0)?;
        assert_eq!(
            reservation.memory.id, 0,
            "funding scopes do not issue backing identities"
        );
        output.reserve_reserved(&mut reservation, CpuStorageKind::Result, 8)?;
        scratch.reserve_reserved(&mut reservation, 4)?;
        memory.resize_reserved(&mut reservation, 8)?;
    }
    assert_eq!(LEDGER_LOCKS.with(std::cell::Cell::get), 0);
    assert_eq!(memory.allocation_id(), identity);
    assert_eq!(output.as_ptr(), address);
    assert_eq!(&*output, &[17, 23]);
    assert_eq!(budget.snapshot().used(CpuStorageClass::Frame), 128);
    drop((output, scratch, memory, hold));
    assert_eq!(budget.snapshot().used(CpuStorageClass::Frame), 0);
    Ok(())
}

#[test]
fn empty_identity_transfer_keeps_identity_without_touching_either_ledger()
-> Result<(), Box<dyn Error>> {
    let first = CpuStorageBudget::new(CpuStoragePlan::new(0, 0, 0));
    let second = CpuStorageBudget::new(CpuStoragePlan::new(0, 0, 0));
    LEDGER_LOCKS.with(|locks| locks.set(0));
    let mut memory = first.reserve(CpuStorageClass::Frame, CpuStorageKind::Result, 0)?;
    let identity = memory.allocation_id();
    memory.transfer(&second, CpuStorageClass::Required, CpuStorageKind::Metadata)?;
    memory.resize(0)?;
    assert_eq!(memory.allocation_id(), identity);
    assert_ne!(identity, 0);
    drop(memory);
    assert_eq!(LEDGER_LOCKS.with(std::cell::Cell::get), 0);
    assert_eq!(first.snapshot().used(CpuStorageClass::Frame), 0);
    assert_eq!(second.snapshot().used(CpuStorageClass::Required), 0);
    Ok(())
}
