# Typed immutable CPU products

`SharedProduct<T, E>` binds an immutable result and its dependency readiness to
one owned generation. Previously, the M2 loading bridge held a completion port
separately from the request slot which supplied its payload. The bridge now
publishes a typed product containing the original `ResourceLease<DecodedM2Model>`
or `M2LoadError`.

## Ownership and ordering

- Admission reserves the fixed result cell and bounded readiness subscriptions
  before domain input transfer. Failure returns the preceding reservation.
- One non-cloneable `ProductPublisher` stores the durable value before releasing
  dependent jobs. Success/failure comes from that value; consumers cannot reset
  the port or signal an outcome separately from the payload.
- Dropping an unpublished producer records abandonment and cancels dependents.
  A published domain error remains available even if scheduler failure skips
  the dependent operation and returns its inputs untouched.
- Cloned product leases pin the same value without cloning its contents or
  allocating another cell. Polling borrows through `OnceLock`, without a result
  mutex. A readiness token alone does not pin the payload.
- A product can outlive its executor and original source request. Last-owner
  release retires its payload and fixed storage charge. A new product cannot
  replace the value addressed by an old readiness token.

The implementation is split into `completion/product/{mod,publisher}.rs`.
This is a shared-result primitive, not a replacement scheduler or asset cache.

## Connected consumer

`M2LoadRequest::dependency` admits one product per consumer edge. Registration
still locks listeners before observing source completion; source publication
stores the result before taking listeners. Publication occurs outside source
and listener locks. Dropping one edge does not cancel the common decode or other
consumers. The edge retains its `CpuServiceInterest`, preserving existing
source urgency independently of result-slot lifetime.

Appearance and GameObject dependent jobs already consume this API, so the new
product is on their production loading path. Their source identity, decode
errors, ordering and stock fallback decisions remain the existing asset-layer
contracts. No gameplay behavior is added by the CPU primitive.

## Validation and limits

CPU coverage exercises fan-out on a single worker, unrelated progress while
waiting, common non-cloneable payload identity, executor-independent lifetime,
failure/abandonment, preserved unexecuted inputs, admission rollback and distinct
old/new generations. Asset coverage includes late subscribers, dependency drop,
registration/publication races, and payload access after request and executor
retirement; pending abandonment also completes after dropping the request.

Validation on 2026-09-20: full workspace Clippy with all targets/features and
warnings denied passed; full workspace tests passed 1,620 tests, zero failures,
33 existing ignored across 99 suites. Formatting passed. Logs are retained in
ignored `target/typed-product-final-*`. The only source edit after compilation
began was the completion module's descriptive leading comment.

Allocations nested in `T` or `E` remain the domain's responsibility. The model
lease retains existing source accounting; this change does not account all
request listeners, decoded buffers or caches. Other source domains and their
cross-resource graphs still need adoption. It does not move the main-thread M2
placement traversal or Vulkan command recording onto workers, and it is not a
claimed live frame-time improvement.
