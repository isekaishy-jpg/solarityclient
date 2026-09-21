//! Retained handoff storage preserves ownership under reuse, pressure and unwind.
use super::*;
use solarity_cpu::CpuStoragePlan;
use std::error::Error;
#[path = "allocations.rs"]
mod allocations;

#[test]
fn warm_loan_checkout_reuses_slot_and_placeholder_without_allocation() -> Result<(), Box<dyn Error>>
{
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(0, 65536, 0));
    let mut pool = PreparationPool::default();
    let first = pool.slot::<Vec<u32>, usize>(Some(&budget))?;
    let baseline = budget.snapshot().used(CpuStorageClass::Required);
    let hold = budget.reserve(
        CpuStorageClass::Required,
        CpuStorageKind::Scratch,
        65536 - baseline,
    )?;
    let mut state = vec![0u32];
    let identity = state.as_ptr();
    let (result, calls) = allocations::count(|| -> Result<(), FontError> {
        for _ in 0..1000 {
            let slot = pool.slot::<Vec<u32>, usize>(Some(&budget))?;
            assert!(Arc::ptr_eq(&first, &slot));
            let loan = Loan::checkout(slot, &mut state)?;
            loan.shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0[0] += 1;
            drop(loan);
        }
        Ok(())
    });
    result?;
    assert_eq!(calls, 0);
    assert_eq!(state, [1000]);
    assert_eq!(state.as_ptr(), identity);
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), 65536);
    drop((hold, first, pool));
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), 0);
    Ok(())
}

#[test]
fn refused_and_panicked_loans_leave_reusable_state_and_exact_admission()
-> Result<(), Box<dyn Error>> {
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(0, 65536, 0));
    let mut pool = PreparationPool::default();
    let hold = budget.reserve(CpuStorageClass::Required, CpuStorageKind::Scratch, 65536)?;
    assert!(pool.slot::<Vec<u32>, ()>(Some(&budget)).is_err());
    assert!(pool.slots.is_empty());
    drop(hold);
    let slot = pool.slot::<Vec<u32>, ()>(Some(&budget))?;
    let mut state = vec![3];
    let identity = state.as_ptr();
    {
        let loan = Loan::checkout(slot.clone(), &mut state)?;
        let mut other = vec![9];
        assert!(Loan::checkout(slot.clone(), &mut other).is_err());
        assert_eq!(other, [9]);
        drop(loan);
    }
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _loan = Loan::checkout(slot.clone(), &mut state)
                .unwrap_or_else(|_| unreachable!("idle slot"));
            let mut borrowed = slot
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            borrowed.0[0] = 11;
            panic!("worker mutation");
        }))
        .is_err()
    );
    assert_eq!(state, [11]);
    assert_eq!(state.as_ptr(), identity);
    drop(Loan::checkout(slot.clone(), &mut state)?);
    drop((slot, pool));
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), 0);
    Ok(())
}

struct OutputDrop(Box<dyn FnMut() + Send>);
impl Drop for OutputDrop {
    fn drop(&mut self) {
        (self.0)();
    }
}
#[test]
fn host_failure_discards_completed_output_outside_slot_lock_before_reuse()
-> Result<(), Box<dyn Error>> {
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(0, 65536, 0));
    let mut pool = PreparationPool::default();
    let slot = pool.slot::<Vec<u32>, OutputDrop>(Some(&budget))?;
    let dropped = Arc::new(AtomicBool::new(false));
    let mut state = vec![1];
    let loan = Loan::checkout(slot.clone(), &mut state)?;
    {
        let check = slot.clone();
        let dropped = dropped.clone();
        slot.state.lock().map_err(|_| "slot lock")?.1 = Some(OutputDrop(Box::new(move || {
            assert!(
                check.state.try_lock().is_ok(),
                "result destruction must not hold the loan lock"
            );
            dropped.store(true, Ordering::Release);
        })));
    }
    // A host error did not consume the typed result; returning the loan must retire it.
    drop(loan);
    assert!(dropped.load(Ordering::Acquire));
    assert!(slot.state.lock().map_err(|_| "slot lock")?.1.is_none());
    drop(Loan::checkout(slot.clone(), &mut state)?);
    drop((slot, pool));
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), 0);
    Ok(())
}

#[test]
fn trimmed_placeholders_drop_after_font_cache_unlock() -> Result<(), Box<dyn Error>> {
    use crate::test_support::Fixture;
    use solarity_asset::{ArchiveCatalog, ClientDataRoot, Locale};
    #[derive(Default)]
    struct Placeholder(Option<Box<dyn FnMut() + Send>>);
    impl Drop for Placeholder {
        fn drop(&mut self) {
            if let Some(drop) = &mut self.0 {
                drop();
            }
        }
    }
    let fixture = Fixture::new(&[])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let namespace = catalog.namespace();
    let mut store = AssetStore::mount(catalog)?;
    let cache = Arc::new(Mutex::new(super::super::state::FontCache::default()));
    let mut pool = PreparationPool::default();
    let slot = pool.slot::<Placeholder, ()>(None)?;
    let checked = Arc::new(AtomicBool::new(false));
    {
        let cache = cache.clone();
        let checked = checked.clone();
        slot.state.lock().map_err(|_| "slot lock")?.0.0 = Some(Box::new(move || {
            assert!(
                cache.try_lock().is_ok(),
                "font cache must be unlocked before typed retirement"
            );
            checked.store(true, Ordering::Release);
        }));
    }
    drop(slot);
    super::super::work::FontWork {
        cache,
        namespace,
        request: Request::Trim(pool),
    }
    .run(&mut store)?;
    assert!(checked.load(Ordering::Acquire));
    Ok(())
}
