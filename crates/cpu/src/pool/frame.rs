//! Joined frame computation has no path to the archive/job injection queue.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Instant;

use rayon::ThreadPool;
use rayon::prelude::{IntoParallelRefMutIterator, ParallelIterator};

use super::CpuError;

/// Ends all worker borrows before returning, including when an item panics.
pub(super) fn execute<T, F>(
    pool: Option<&ThreadPool>,
    items: &mut [T],
    operation: F,
) -> Result<(), CpuError>
where
    T: Send,
    F: Fn(&mut T) + Send + Sync,
{
    let _profile = solarity_profiling::profile_cycles!("cpu.pool.executor.for_each");
    solarity_profiling::profile_value!("cpu.batch.items", items.len());
    catch_unwind(AssertUnwindSafe(|| {
        let Some(pool) = pool else {
            for item in items {
                operation(item);
            }
            return;
        };
        let epoch = solarity_profiling::generation();
        let queued = (epoch != 0).then(Instant::now);
        let trace = solarity_profiling::TraceContext::capture();
        pool.install(|| {
            let _trace = trace.enter();
            if let Some(queued) = queued {
                static QUEUE: solarity_profiling::Site =
                    solarity_profiling::Site::new("cpu.batch.dispatch_wait", false);
                QUEUE.cpu_duration(epoch, "", queued.elapsed());
            }
            let _execution = solarity_profiling::profile_cycles!("cpu.batch.execution");
            let item_trace = solarity_profiling::TraceContext::capture();
            items.par_iter_mut().for_each(|item| {
                let _trace = item_trace.enter();
                let _item = solarity_profiling::detail_profile!("cpu.batch.worker_item");
                operation(item);
            });
        });
    }))
    .map_err(|_| CpuError::TaskPanicked)
}
