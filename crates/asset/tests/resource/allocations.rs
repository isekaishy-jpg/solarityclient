//! Thread-local allocation measurement excludes other concurrently running tests.

#![allow(unsafe_code)]

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

/// Test instrumentation adds no allocation or synchronization of its own.
struct Counter;
#[global_allocator]
static ALLOCATOR: Counter = Counter;
thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static CALLS: Cell<usize> = const { Cell::new(0) };
}

/// Thread teardown may allocate after its TLS is unavailable; no measurement is active then.
fn record() {
    if ACTIVE.try_with(Cell::get).unwrap_or(false) {
        let _recorded = CALLS.try_with(|count| count.set(count.get() + 1));
    }
}

// SAFETY: Arguments, pointers and allocation ownership are forwarded to System unchanged.
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record();
        // SAFETY: The caller supplies the layout required by GlobalAlloc.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record();
        // SAFETY: The zeroed allocation contract is preserved by forwarding.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record();
        // SAFETY: The original live allocation and requested size are unchanged.
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: The caller supplies the original allocation and layout.
        unsafe { System.dealloc(ptr, layout) }
    }
}

/// Always closes measurement before error reporting or another test can run here.
struct Window;
impl Drop for Window {
    fn drop(&mut self) {
        ACTIVE.set(false);
    }
}

/// Measures only the synchronous operation on this thread, including its final drops.
pub(super) fn count<T>(operation: impl FnOnce() -> T) -> (T, usize) {
    CALLS.set(0);
    ACTIVE.set(true);
    let window = Window;
    let result = operation();
    drop(window);
    (result, CALLS.get())
}
