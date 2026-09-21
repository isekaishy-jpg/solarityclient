# Scoped source scheduling demand

The CPU cutover needs one admitted loader to own several successive sources.
A source's consumers must be able to promote the producer currently running
inside that loader without replacing the loader's own demand. The source must
also stop influencing later stages once it publishes.

`CpuServiceControl::scoped_demand` now returns a producer-owned scope. Its control
can bind to the source's existing aggregate demand. The effective queue class is
the strongest of the containing task's owner and its live scopes. All controls
for an admitted task or loading epoch share that state, including controls
obtained separately before and after dispatch. Dropping a scope removes only
its contribution; stale consumer controls cannot reactivate it.

The scheduler's ready queues still read an atomic class. Demand transitions
reconcile queue publication outside identity locks, including propagation along
suspended dependency edges. The change does not introduce a second queue, polling
worker, or additional admitted task. Task identities and scoped controls reserve
and retain metadata charges; failure to reserve a scope leaves demand unchanged.

The shared WMO loader now binds each incremental root/group producer through a
scope. A withdrawn terrain owner still finishes a source needed by another
required consumer. Publication, failure and withdrawal release the scope before
later terrain work. Failure to admit producer priority metadata is published as
a pipeline error to joined source consumers rather than reported as an abandoned
producer. Stock root/group ordering, source data and instance publication are
unchanged.

Formatting, full-workspace Clippy with warnings denied, and the full workspace
suite pass: 1,680 tests passed, zero failed, 33 ignored, including 138 CPU tests
and the real WMO retirement/publication fixture. Doc tests also pass. The final
suite used a separate Cargo target directory; its logs are in ignored
`target/isolated-workspace-test.{stdout,stderr}.log`. The Clippy log is
`target/service-clippy.stderr.log`. Interrupted earlier runs are not evidence of
a full-suite pass.

This work remains isolated in `perf/cpu-service-composition` while another active
run owns appearance/effect integration on `perf/critical-frame-work`. Integration
and the complete combined checks remain outstanding. The change addresses
scheduling-demand composition; it does not claim an FPS gain or completion of
the CPU cutover.
