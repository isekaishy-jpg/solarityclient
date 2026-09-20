//! Typed products retain their generation and payload through dependency execution.

use solarity_cpu::{
    CpuError, CpuExecutionPlan, CpuExecutor, CpuPoolConfig, CpuService, CpuStorageBudget,
    CpuStorageClass, CpuStoragePlan, JobOutcome, LoadBatch, ProductOutcome, SharedProduct,
};
use std::{
    error::Error,
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

/// Destruction is observable without requiring the output itself to implement Clone.
struct Value {
    number: usize,
    drops: Arc<AtomicUsize>,
}
impl Drop for Value {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

/// A single flexible worker proves pending products do not occupy an execution lane.
fn cpu() -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(0, 1, 1, 1)?,
        NonZeroUsize::new(8).ok_or("capacity")?,
        CpuStoragePlan::new(1 << 20, 1 << 20, 1 << 20),
    ))?)
}

/// Every reader pins the same immutable output; no producer or executor owns its last lifetime.
#[test]
fn product_fanout_releases_dependents_after_payload_and_survives_executor_shutdown()
-> Result<(), Box<dyn Error>> {
    let mut cpu = cpu()?;
    let budget = cpu.storage().clone();
    let baseline = budget.snapshot().used(CpuStorageClass::Required);
    let (publisher, product) =
        SharedProduct::<Value, &'static str>::new(2, &budget, CpuStorageClass::Required)?;
    let ready = product.readiness();
    let drops = Arc::new(AtomicUsize::new(0));
    let run = |input: &mut (SharedProduct<Value, &'static str>, usize)| match input.0.poll() {
        Some(ProductOutcome::Succeeded(value)) => {
            input.1 = value.number;
            JobOutcome::Succeeded
        }
        _ => JobOutcome::Failed,
    };
    let mut first = LoadBatch::new(CpuService::Required, run);
    let mut second = LoadBatch::new(CpuService::Required, run);
    let mut left = vec![(product.clone(), 0)];
    let mut right = vec![(product.clone(), 0)];
    first.start_after(&cpu, &mut left, std::slice::from_ref(&ready))?;
    second.start_after(&cpu, &mut right, std::slice::from_ref(&ready))?;
    assert_eq!(cpu.try_submit(|| 31)?.join()?, 31);
    assert!(product.poll().is_none());
    publisher.publish(Ok(Value {
        number: 42,
        drops: Arc::clone(&drops),
    }));
    first.reclaim(&mut left)?;
    second.reclaim(&mut right)?;
    assert_eq!((left[0].1, right[0].1), (42, 42));
    let (Some(ProductOutcome::Succeeded(a)), Some(ProductOutcome::Succeeded(b))) =
        (left[0].0.poll(), right[0].0.poll())
    else {
        return Err("missing product".into());
    };
    assert!(std::ptr::eq(a, b));
    drop(first);
    drop(second);
    drop(left);
    drop(right);
    cpu.shutdown()?;
    assert!(budget.snapshot().used(CpuStorageClass::Required) > baseline);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drop(product);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), baseline);
    assert!(ready.outcome().is_err());
    Ok(())
}

/// Domain failures preserve their typed error, whereas abandoned producers cancel successors.
#[test]
fn failed_and_abandoned_products_return_unexecuted_inputs_and_retain_terminal_state()
-> Result<(), Box<dyn Error>> {
    let cpu = cpu()?;
    for abandon in [false, true] {
        let (publisher, product) =
            SharedProduct::<usize, usize>::new(1, cpu.storage(), CpuStorageClass::Required)?;
        let ready = product.readiness();
        let mut load = LoadBatch::new(CpuService::Required, |value: &mut usize| {
            *value = 0;
            JobOutcome::Succeeded
        });
        let mut inputs = vec![91];
        load.start_after(&cpu, &mut inputs, std::slice::from_ref(&ready))?;
        if abandon {
            drop(publisher);
        } else {
            publisher.publish(Err(17));
        }
        assert!(load.reclaim(&mut inputs).is_err());
        assert_eq!(inputs, [91]);
        assert_eq!(
            ready.outcome()?,
            Some(if abandon {
                JobOutcome::Cancelled
            } else {
                JobOutcome::Failed
            })
        );
        if abandon {
            assert!(matches!(product.poll(), Some(ProductOutcome::Abandoned)));
        } else {
            assert!(matches!(product.poll(), Some(ProductOutcome::Failed(&17))));
        }
    }
    Ok(())
}

/// A rejected dependency capacity must release the previously admitted fixed result cell.
#[test]
fn product_admission_rolls_back_storage_and_old_generation_never_addresses_new_payload()
-> Result<(), Box<dyn Error>> {
    let budget = CpuStorageBudget::new(CpuStoragePlan::new(1 << 20, 1 << 20, 0));
    assert!(matches!(
        SharedProduct::<u32, u32>::new(1, &budget, CpuStorageClass::Speculative),
        Err(CpuError::StorageAtCapacity { .. })
    ));
    assert!(
        SharedProduct::<u32, u32>::new(usize::MAX, &budget, CpuStorageClass::Required).is_err()
    );
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), 0);
    let (old_producer, old) =
        SharedProduct::<u32, u32>::new(0, &budget, CpuStorageClass::Required)?;
    let old_ready = old.readiness();
    old_producer.publish(Ok(5));
    let (new_producer, new) =
        SharedProduct::<u32, u32>::new(0, &budget, CpuStorageClass::Required)?;
    new_producer.publish(Ok(9));
    assert!(matches!(old.poll(), Some(ProductOutcome::Succeeded(&5))));
    assert!(matches!(new.poll(), Some(ProductOutcome::Succeeded(&9))));
    drop(old);
    assert!(old_ready.outcome().is_err());
    assert!(matches!(new.poll(), Some(ProductOutcome::Succeeded(&9))));
    drop(new);
    assert_eq!(budget.snapshot().used(CpuStorageClass::Required), 0);
    Ok(())
}
