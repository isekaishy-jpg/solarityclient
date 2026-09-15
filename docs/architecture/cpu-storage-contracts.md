# CPU execution storage contracts

This is the implemented storage boundary for the direct cutover, not completion
of the [CPU and cache architecture](cpu-cutover-status.md). Stock gameplay,
arithmetic, callback selection and ordered publication retain their existing
contracts; execution capacity is modern application policy.

`CpuPoolConfig` requires a `CpuStoragePlan`. The executor owns a shared ledger
with independent frame, required-load and speculative allowances. Limits are
logical buffer-capacity bytes, not OS working set, allocator bookkeeping,
thread stacks, Arc headers or total client memory. A snapshot reports category
usage and simultaneous peak reservations consistently under one lock.

Runtime defaults are 128/256/64 MiB respectively. Optional `--cpu-frame-bytes`,
`--cpu-required-bytes` and `--cpu-speculative-bytes` override those values with
nonnegative byte counts. Task-count admission remains separate. A zero allowance
permits no positive-byte reservation; startup must still reserve its required
queue/registry capacity. An infeasible policy is an error, not a reason to execute
the phase synchronously or silently omit inputs.

Frame job cells, node/edge metadata, ready/propagation rings, dependency tokens,
subscriber slots, graph topology, dispatch queues and shutdown registry now own
capacity reservations. Independent graph templates allocate no topology arrays.
The typed batch preadmits metadata before it drains any caller inputs. A failure
can retain successfully grown empty metadata, but that capacity remains charged
and inputs remain with the caller. No unbounded overflow queue is provided.

Retained buffers reserve a full replacement while their old allocation remains
charged. Allocation failure leaves old contents intact. After a successful
allocation its actual element capacity is reconciled before moving values.
The old allocation is freed before its charge is released. Clearing or reclaiming
a buffer releases values, not capacity; disposal releases both. Warm reuse of an
unchanged budget and capacity performs no ledger transaction or allocation.

`ByteReservation` is the unique charge for an allocation. Domain users must
reserve before allocating/transferring mandatory inputs, account actual capacity,
and free storage before reducing or dropping a charge. Nested allocations in a
job or result need their own reservation; `size_of::<T>()` covers only the element
slots. The wrappers enforce capacity ownership for their own buffers, not an
arbitrary domain object graph. Remaining M2/asset/cache adoption is required work.

`CpuResultPage<T>` owns writable contiguous output, with explicit fallible growth
and nonallocating append. `freeze` publishes an immutable `CpuResultLease<T>`;
clones share the same allocation identity and charge. Only the final consumer can
reclaim writable storage. Its page capacity remains charged through reclaim and
clear. Freeze currently allocates the shared owner header; it is not the warmed
frame-graph allocation-free contract or a completed reusable page-owner pool.

An exclusively owned allocation can transfer between categories or budgets
without copying its contents or creating a second allocation identity. Rejected
transfers retain source ownership. Cross-ledger transactions acquire locks by
ledger address order and invoke no user code. Each buffer transfer is atomic;
rebinding a multi-buffer phase may transfer some retained buffers before a later
reservation fails, with each still charged exactly once. Domain work does not run
under an accounting lock. Retained reservations keep their ledger alive after
executor shutdown.

The existing F10 detail path samples the ledger once and publishes class totals,
category bytes, peaks and limits as `cpu.storage.*`. Ordinary frame execution does
not sample it. These counters must not be compared directly to process RSS or
reported as a complete domain memory census.
