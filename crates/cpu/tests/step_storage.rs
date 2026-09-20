//! A yielded service keeps its task allocation and queue metadata across resumes.

#![allow(unsafe_code)]

use solarity_cpu::{CpuExecutor, CpuPoolConfig};
use std::alloc::{GlobalAlloc, Layout, System};
use std::error::Error;
use std::num::NonZeroUsize;
use std::ops::ControlFlow;
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
fn one_thousand_service_resumes_allocate_no_task_or_queue_storage() -> Result<(), Box<dyn Error>> {
    let cpu = CpuExecutor::new(CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::MIN;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        NonZeroUsize::MIN,
        solarity_cpu::CpuStoragePlan::new(1 << 20, 1 << 20, 1 << 20),
    ))?;
    let window = CountWindow;
    let mut iteration = 0;
    let task = cpu.try_reserve()?.submit_steps(move || {
        iteration += 1;
        if iteration == 64 {
            ALLOCATIONS.store(0, Ordering::SeqCst);
            COUNTING.store(true, Ordering::SeqCst);
        }
        if iteration == 1064 {
            COUNTING.store(false, Ordering::SeqCst);
            ControlFlow::Break(iteration)
        } else {
            ControlFlow::Continue(())
        }
    });
    let completed = task.join()?;
    drop(window);
    assert_eq!(completed, 1064);
    assert_eq!(ALLOCATIONS.load(Ordering::SeqCst), 0);
    Ok(())
}
