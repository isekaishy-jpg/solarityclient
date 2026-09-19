//! Steady-state graph activation reuses metadata rather than allocating per frame.

#![allow(unsafe_code)]

use solarity_cpu::{
    CompletionPort, CpuExecutor, CpuPoolConfig, FrameBatch, FrameGraphTemplate, FramePriority,
    JobOutcome,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::error::Error;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// One-test executable counts both coordinator and worker allocations after warmup.
struct AllocationCounter;
static COUNTING: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
#[global_allocator]
static ALLOCATOR: AllocationCounter = AllocationCounter;

/// Records allocator entries without allocating or acquiring another lock.
fn record() {
    if COUNTING.load(Ordering::Relaxed) {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    }
}

// SAFETY: This wrapper preserves every System allocator argument, result and
// ownership contract; the additional atomics never access allocated storage.
unsafe impl GlobalAlloc for AllocationCounter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record();
        // SAFETY: GlobalAlloc callers provide the original valid layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record();
        // SAFETY: Forwarding preserves the requested zeroed allocation contract.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record();
        // SAFETY: The pointer/layout/size contract is unchanged by this wrapper.
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: The original allocation and layout are forwarded unchanged.
        unsafe { System.dealloc(ptr, layout) }
    }
}

/// Stops measurement before diagnostics even if an operation returns an error.
struct CountWindow;
impl Drop for CountWindow {
    fn drop(&mut self) {
        COUNTING.store(false, Ordering::SeqCst);
    }
}

#[test]
fn warmed_graphs_and_external_fan_in_allocate_no_activation_metadata() -> Result<(), Box<dyn Error>>
{
    let cpu = CpuExecutor::new(CpuPoolConfig::new(
        NonZeroUsize::new(2).ok_or("workers")?,
        NonZeroUsize::new(8).ok_or("capacity")?,
        solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?;
    let mut first = CompletionPort::new(1, cpu.storage(), solarity_cpu::CpuStorageClass::Frame)?;
    let mut second = CompletionPort::new(1, cpu.storage(), solarity_cpu::CpuStorageClass::Frame)?;
    let template =
        FrameGraphTemplate::with_dependencies(cpu.storage(), &[&[], &[0], &[0], &[1, 2]])?
            .with_priority(FramePriority::Prerequisite);
    let mut batch = FrameBatch::new(|value: &mut usize| *value += 1);
    let mut jobs = vec![0; 4];
    let costs = [1, 400, 100, 5]
        .map(|micros| solarity_cpu::JobCost::measured(std::time::Duration::from_micros(micros)));
    let mut window = None;
    for iteration in 0..1064 {
        if iteration == 64 {
            ALLOCATIONS.store(0, Ordering::SeqCst);
            COUNTING.store(true, Ordering::SeqCst);
            window = Some(CountWindow);
        }
        first.producer()?.complete(JobOutcome::Succeeded)?;
        let mut producer = second.producer()?;
        batch.start_costed_graph(
            &cpu,
            &template,
            &mut jobs,
            &[first.readiness(), second.readiness()],
            &costs,
        )?;
        producer.complete(JobOutcome::Succeeded)?;
        batch.reclaim(&mut jobs)?;
        first.restart(1, cpu.storage(), solarity_cpu::CpuStorageClass::Frame)?;
        second.restart(1, cpu.storage(), solarity_cpu::CpuStorageClass::Frame)?;
    }
    drop(window);
    assert_eq!(jobs, [1064; 4]);
    assert_eq!(ALLOCATIONS.load(Ordering::SeqCst), 0);
    Ok(())
}
