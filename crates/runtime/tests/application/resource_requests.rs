//! Shared producers preserve demand, independent lifetimes and bounded CPU ownership.

use std::{
    error::Error,
    num::NonZeroUsize,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
};

use solarity_cpu::{CpuError, CpuExecutor, CpuPoolConfig, CpuService, CpuStoragePlan};

use super::ResourceRequests;

/// One flexible lane makes service ordering deterministic without clocks or sleeps.
fn pool(capacity: usize) -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        NonZeroUsize::MIN,
        NonZeroUsize::new(capacity).ok_or("positive test capacity required")?,
        CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?)
}

/// A required FIFO marker observes earlier producer publication on this lane.
fn finish(
    cpu: &CpuExecutor,
    requests: &mut ResourceRequests<u32, u32, u32, CpuError>,
) -> Result<(), CpuError> {
    cpu.try_submit(|| ())?.join()?;
    assert!(requests.poll().is_empty());
    Ok(())
}

/// Multiple subscribers and late joins observe the exact same immutable allocation.
#[test]
fn shared_request_runs_once_and_releases_only_the_registered_generation()
-> Result<(), Box<dyn Error>> {
    let mut cpu = pool(8)?;
    let mut requests = ResourceRequests::new();
    let calls = Arc::new(AtomicUsize::new(0));
    requests.request(7, 100, CpuService::Required);
    requests.request(7, 101, CpuService::Required);
    let counter = Arc::clone(&calls);
    assert!(requests.start(&7, &cpu, || move || {
        counter.fetch_add(1, Ordering::Relaxed);
        Ok(Arc::new(42))
    })?);
    assert!(!requests.start(&7, &cpu, || || panic!(
        "joined request must not create another producer"
    ))?);
    finish(&cpu, &mut requests)?;
    let first = requests.ready(&7, &100).ok_or("request is not ready")??;
    let second = requests.ready(&7, &101).ok_or("request is not ready")??;
    assert!(Arc::ptr_eq(&first, &second));
    assert!(requests.cancel(&7, &100).is_none());
    assert!(requests.ready(&7, &100).is_none());
    requests.request(7, 102, CpuService::Required);
    assert!(Arc::ptr_eq(
        &first,
        &requests.ready(&7, &102).ok_or("request is not ready")??
    ));
    assert!(requests.cancel(&7, &101).is_none());
    assert!(requests.cancel(&7, &102).is_some());
    assert!(requests.entries.is_empty());
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    cpu.shutdown()?;
    Ok(())
}

/// A cancelled required consumer withdraws urgency without cancelling a speculative peer.
#[test]
fn strongest_live_demand_promotes_and_withdraws_the_same_producer() -> Result<(), Box<dyn Error>> {
    for withdraw in [false, true] {
        let mut cpu = pool(8)?;
        let (release, wait) = mpsc::channel();
        let blocker = cpu.try_submit(move || wait.recv())?;
        let output = Arc::new(Mutex::new(Vec::new()));
        let mut requests: ResourceRequests<u32, u32, u32, CpuError> = ResourceRequests::new();
        for key in [1, 2] {
            requests.request(key, 10, CpuService::Speculative);
            let output = Arc::clone(&output);
            requests.start(&key, &cpu, || {
                move || {
                    output
                        .lock()
                        .unwrap_or_else(|_| unreachable!("test recorder must remain unpoisoned"))
                        .push(key);
                    Ok(Arc::new(key))
                }
            })?;
        }
        requests.request(2, 11, CpuService::Required);
        if withdraw {
            requests.cancel(&2, &11);
        }
        release.send(())?;
        blocker.join()??;
        cpu.shutdown()?;
        assert!(requests.poll().is_empty());
        assert_eq!(
            *output
                .lock()
                .unwrap_or_else(|_| unreachable!("test recorder must remain unpoisoned")),
            if withdraw { vec![1, 2] } else { vec![2, 1] }
        );
        assert!(requests.ready(&2, &10).is_some());
        assert_eq!(requests.ready(&2, &11).is_some(), !withdraw);
    }
    Ok(())
}

/// Failed requests fan out once; releasing current consumers leaves no negative cache.
#[test]
fn shared_failure_and_worker_panic_do_not_poison_future_requests() -> Result<(), Box<dyn Error>> {
    let mut cpu = pool(8)?;
    let mut requests: ResourceRequests<u32, u32, u32, CpuError> = ResourceRequests::new();
    for panic in [false, true] {
        requests.request(1, 10, CpuService::Required);
        requests.request(1, 11, CpuService::Required);
        requests.start(&1, &cpu, || {
            move || {
                if panic {
                    panic!("controlled producer failure")
                } else {
                    Err(CpuError::CompletionLost)
                }
            }
        })?;
        finish(&cpu, &mut requests)?;
        let first = requests
            .ready(&1, &10)
            .ok_or("request is not ready")?
            .err()
            .ok_or("expected producer failure")?;
        let second = requests
            .ready(&1, &11)
            .ok_or("request is not ready")?
            .err()
            .ok_or("expected producer failure")?;
        assert!(Arc::ptr_eq(&first, &second));
        if panic {
            assert!(matches!(*first, CpuError::TaskPanicked));
        }
        requests.cancel(&1, &10);
        requests.cancel(&1, &11);
        assert!(requests.entries.is_empty());
    }
    requests.request(1, 12, CpuService::Required);
    assert!(requests.start(&1, &cpu, || || Ok(Arc::new(99)))?);
    finish(&cpu, &mut requests)?;
    assert_eq!(*requests.ready(&1, &12).ok_or("request is not ready")??, 99);
    assert!(requests.ready(&1, &11).is_none());
    requests.drain();
    cpu.shutdown()?;
    Ok(())
}

/// Refusal keeps subscriptions; abandoned accepted work drains with exact ownership.
#[test]
fn capacity_refusal_reacquisition_and_abandoned_shutdown_keep_the_producer_owned()
-> Result<(), Box<dyn Error>> {
    let mut cpu = pool(1)?;
    let (release, wait) = mpsc::channel();
    let blocker = cpu.try_submit(move || wait.recv())?;
    let mut requests: ResourceRequests<u32, u32, u32, CpuError> = ResourceRequests::new();
    requests.request(1, 10, CpuService::Required);
    let mut input = Some(Arc::new(7));
    assert!(matches!(
        requests.start(&1, &cpu, || {
            let value = input
                .take()
                .unwrap_or_else(|| unreachable!("fixture input has one owner"));
            move || Ok(value)
        }),
        Err(CpuError::AtCapacity { .. })
    ));
    assert!(
        input.is_some(),
        "refusal must not invoke the producer factory"
    );
    release.send(())?;
    blocker.join()??;
    requests.start(&1, &cpu, || || Ok(Arc::new(7)))?;
    requests.cancel(&1, &10);
    requests.request(1, 11, CpuService::Required);
    assert!(!requests.start(&1, &cpu, || || panic!(
        "reacquisition must join its existing producer"
    ))?);
    requests.cancel(&1, &11);
    cpu.shutdown()?;
    let retired = requests.poll();
    assert_eq!(retired.len(), 1);
    let Ok(value) = &retired[0] else {
        return Err("abandoned producer failed".into());
    };
    assert_eq!(**value, 7);
    assert!(requests.entries.is_empty());
    assert!(!requests.has_running());
    Ok(())
}
