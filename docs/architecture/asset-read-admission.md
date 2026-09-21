# Encoded source admission and ownership

This connects the CPU storage budget to encoded source allocations. It does not
complete decoded-cache residency, decoder scratch accounting, or the CPU cutover.

`AssetReadBudget` chooses Required storage for required/retirement service and
Speculative storage for speculative reads. MPQ metadata is inspected before the
backend allocates returned input bytes. Path-based reads remain intact because
the pinned wow-mpq index-only API invents a filename and changes encryption-key
selection. Lookup precedence and missing-source behavior remain unchanged.

`AssetBytes` retains the reservation across `AssetRead::into_bytes`, immutable
shared ownership, database retention, FreeType face ownership, and encoded sound
retention. Slice access cannot grow the buffer or detach its reservation.
`AssetText` validates UTF-8 without reallocating and carries the same reservation.
Lua source clones share that immutable owner. Invalid UTF-8 releases the bytes
before releasing admission. Native decoder errors and rejected output-capacity
reconciliation also release their source ownership.

`AssetStore::with_read_budget` scopes the policy to an operation. The handle
variant installs/restores policy without holding a mutable archive-reader borrow
across consumer code. Nested operations and unwind restore the previous policy.
Escaping a RefCell borrow from the handle operation is a programming error.

Connected production paths include primary/nested appearance M2 sources and
appearance textures; GameObject M2 preparation; terrain map/tile, texture,
liquid, ground-detail and shared M2/WMO turns; selected sound reads; world UI
source loading; sky sources/textures; unit-effect source loading; and configured
speculative Glue textures. Shared WMO retirement retains the budget and samples
its effective service demand at each step. Shared-source read failures publish
through the existing producer to every waiter.

The MPQ preflight covers declared returned source size, reserving the larger of
stored and decoded sizes because raw entries may return stored buffers directly.
Actual returned capacity is reconciled before publication. This is not a hard
bound on a third-party codec's temporary allocations or malformed expansion.
Loose AddOn files use metadata preflight and the same returned-capacity check;
the existing loose-first AddOn namespace is unchanged. A concurrent loose-file
size change is reconciled after the read.

Remaining work includes decoded resource vectors, backend temporary storage,
main-owned startup reads, other retained caches and complete working-set
admission/trim policy. This change does not establish a live FPS improvement.

## Verification

Workspace/all-target/all-feature compilation, formatting, and full workspace
Clippy with warnings denied pass. All five real archive ownership/admission tests
pass. The previously overflowing application startup test also passes with the
ordinary test stack. The full workspace suite passes 1,692 tests with 0 failures and 33 existing
ignored tests across 104 suite summaries, including doc tests. No compiler or
linker warnings were emitted.
Logs are in ignored `target/admission-*.{stdout,stderr}.log`.

The combined source also includes the separately documented
[startup ownership correction](cpu-startup-stack.md).
