//! Warming numeric continuation epochs must not allocate frame metadata.

#![allow(unsafe_code)]

use solarity_cpu::{
    CompletionPort, CpuExecutor, CpuPoolConfig, CpuStorageClass, CpuStoragePlan, JobOutcome,
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
fn warmed_main_continuations_allocate_no_metadata() -> Result<(), Box<dyn Error>> {
    let cpu = CpuExecutor::new(CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::MIN;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        NonZeroUsize::MIN,
        CpuStoragePlan::new(64 << 20, 64 << 20, 0),
    ))?;
    let mut first = CompletionPort::new(1, cpu.storage(), CpuStorageClass::Frame)?;
    let mut second = CompletionPort::new(1, cpu.storage(), CpuStorageClass::Frame)?;
    let mut ready = cpu.main_ready_queue();
    let mut window = None;
    for epoch in 0..1064 {
        if epoch == 64 {
            ALLOCATIONS.store(0, Ordering::SeqCst);
            COUNTING.store(true, Ordering::SeqCst);
            window = Some(CountWindow);
        }
        ready.begin(2, 2, cpu.storage(), CpuStorageClass::Frame)?;
        ready.watch(1, &[first.readiness(), second.readiness()])?;
        ready.watch(2, &[])?;
        assert_eq!(ready.take_ready().ok_or("independent operation")?.key(), 2);
        first.producer()?.complete(JobOutcome::Succeeded)?;
        second.producer()?.complete(JobOutcome::Succeeded)?;
        assert_eq!(ready.take_ready().ok_or("dependent operation")?.key(), 1);
        assert!(ready.take_ready().is_none());
        first.restart(1, cpu.storage(), CpuStorageClass::Frame)?;
        second.restart(1, cpu.storage(), CpuStorageClass::Frame)?;
    }
    drop(window);
    assert_eq!(ALLOCATIONS.load(Ordering::SeqCst), 0);
    Ok(())
}
