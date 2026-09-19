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
arbitrary domain object graph. M2 model-local output and copied overrides now use this boundary. Live simulation
and pose allocations, final frame streams, assets and caches remain required work.

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

`CpuBuffer<T>` provides an exclusively owned domain buffer whose charge survives
worker transfer and draining. Its `FixedWriter` can append, roll back and borrow
initialized slices, but cannot grow the allocation. The `OutputBuffer` contract
lets the particle/ribbon builders share their existing generation logic between
ordinary owned mesh construction and fixed frame output. Ordinary Vec destinations
reserve explicitly; admitted frame destinations reject an insufficient range.

World and login M2 presentation now use the application executor. Each geometry
job reserves mesh records (including both liquid passes), particle/ribbon records,
vertex/index streams, particle sort indices and copied bone overrides before
placement simulation state moves into that job. This is incremental per-model
admission; it does not yet reserve all domain storage of the connected frame as
one transaction. A later failure drains earlier admitted jobs through the existing
ordered cleanup path.

A shared model source caches the existing authored particle rate/lifetime bound
once. Reserving against the larger of that bound and a placement's current pool
covers first update and animated growth without scanning animation tracks in each
frame. This may retain more CPU output capacity than the current live particle
count; it is charged against the explicit frame allowance. It does not pre-grow
or otherwise modify world simulation pools, twinkle identities or RNG state, and
the final GPU capacity report remains based on actual stock simulation capacity.

Particle sorting uses an in-place sort with presentation-index tie-breaking to
retain the previous equal-depth order without a temporary stable-sort allocation.
Other allocation sources inside the job (including simulation growth) still need
integration; fixed output does not imply an allocation-free entire M2 kernel.

Shared M2 request tables and the new `CpuServiceDemand`/`CpuServiceInterest`
registrations currently use ordinary owned map/Arc storage. These request and
consumer metadata allocations are not included in the reported byte ledger.
They still require resource-request budget admission and accounting along with
source payloads; the shared scheduling-control handle does not imply that their
memory has been accounted for. CPU owns only the service counters/control here,
while assets own request keys, decoded payloads, outcomes and retention policy.

Terrain continuations retain one admitted task and private archive/cache bank
across their service turns. Tile preparation keeps the same boxed continuation
through per-texture and per-doodad yields; it does not reserve another CPU job
or allocate a replacement continuation per resource. Its surface/query inputs,
placement cursor metadata and decoded payloads remain ordinary domain allocations,
not newly covered byte-ledger storage. Partially prepared tiles stay owned by the
task until failure or complete publication through the existing coordinator gate.

The epoch registry's bounded entry storage includes its weak liveness identities.
Each batch creates one separate atomic identity cell alongside its cold
synchronization owner and reuses it across activations. Registry pruning can
release only that metadata under its lock, never a domain-bearing batch owner.
As with other cold synchronization owners, the cell is outside the logical
buffer-capacity ledger; this change does not expand the ledger into a heap census.

Background loading phases share the ordinary task admission limit and reserve
node/input/readiness metadata in their fixed required or speculative byte class.
Each phase has at most one background runner; promotion reclassifies its current
service identity without moving its existing allocation charges. Frame and load
shutdown registries are separately bounded. Priority and completion capacity
covers both sets of admitted phases. Loading allocates a new small service
identity per activation so an old control cannot reclassify a later activation;
this does not change warmed frame activation allocation behavior.

Each shared M2 graph consumer reserves a one-slot completion port against its
CPU byte class. The shared request's weak listener list, bridge Arc, source
payload, mounted bank and buffers nested in a GameObject input remain ordinary
domain allocations. Listener entries are removed on source publication and dead
entries are pruned on registration. No global fixed model fan-out cap is assumed.
