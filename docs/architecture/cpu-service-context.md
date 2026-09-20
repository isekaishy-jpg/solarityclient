# Contextual services and terrain withdrawal

Finite service tasks now receive the same `JobContext` contract as frame and
loading kernels. `submit_with_context` and `submit_steps_with_context` preserve
one admitted service identity through every resume. Context supplies inherited
diagnostics, cooperative withdrawal and scoped preadmitted scratch; it exposes
no runtime state or dependency wait. Existing non-contextual calls adapt to this
boundary without changing their completion behavior.

`CpuTask::cancel` requests withdrawal while retaining its result. Dropping the
handle also requests withdrawal, but executor ownership and shutdown draining
remain unconditional. A domain must choose a safe stopping boundary. Required
destruction and cache maintenance deliberately ignore withdrawal and finish;
they use context diagnostics without making cleanup optional.

The shared control allocation is charged as Required metadata before callers
transfer inputs. Failed metadata admission returns the task slot. The charge
survives until producer and consumer release the control. Its allocation identity
provides service provenance; this is not the missing typed cross-domain product
contract. Closure captures, result channels and nested payloads still require
their own accounting. No allocation is added per service resume.

## Terrain consumer

Terrain preparation checks withdrawal before each existing whole-operation
boundary: mounting, map manifest, low detail, content, and tile preparation.
Superseded primary/prewarm requests, retired world generations and streamed
requests outside current demand lose publication eligibility and request
withdrawal together. Returning to the same tile cannot revive a retired task.

A withdrawn task retires partial preparation on its worker, collects unused
cache entries and returns the mounted archive bank with no resident result.
The coordinator can reuse that bank for current demand. It never publishes a
partial tile or records withdrawal as an asset failure. Legitimate preworld
prewarm remains retained. Current requests retain existing first-error behavior.

This extends the existing generation and demand contract. Stock `7831A0` /
`780860` installs camera demand before `7B6B00` services loading; the existing
streaming publication gate remains authoritative. It changes obsolete work,
not selected terrain, animation, camera sampling, callbacks or RNG order.

Worker preparation and task completion/control now have folder modules with
separate ownership and execution responsibilities.

## Validation and limits

Formatting and full workspace Clippy (all targets/features, warnings denied)
pass. Full workspace tests: 1,617 passed, zero failed, 33 existing ignored.

CPU regressions cover stable identity across resumes, cancellation with owned
input return, dropped-handle cleanup draining, and refused/unused control
admission. The existing 1,000-resume allocation regression still passes.
The terrain fixture withdraws before execution and at four subsequent boundaries,
verifies the same mounted bank returns without decoding a missing later asset,
then successfully prepares another tile with that bank.

This does not bound the duration of an indivisible decode, nested WMO preparation
or cache collection. Broad service adoption, worker-local scratch policy, typed
products and complete working-set accounting remain cutover requirements. No
live FPS gain is inferred from these ownership tests.

## NPC-density report

The user's 2026-09-20 Brewfest run reports lower FPS around dense NPC populations.
`solarity-20260920-065404.log` identifies Build 162 and a normal UI-requested exit.
It records several resource capacity expansions, including M2 draw demand growing
from 1,931 to 2,068 and a later bone demand of 9,375. These are different frame
samples; they neither count NPCs nor establish a sustained CPU/GPU bottleneck.
No new F10 capture accompanies that run.

Code inspection confirms root poses already run as independent worker inputs,
selected by camera or shadow demand after ordered callbacks. Attached models
have later transform dependencies. The older capture `1789889511517-1` reports
zero unconsumed root palettes in its sampled frames, but predates this report.
It cannot rule out different behavior in the dense Brewfest scene. A subsequent
[isolated population sweep](npc-density-cutover-measurements.md) reproduces about
4.5 ms of added frame cost at 192 authored NPCs, spread across preparation,
publication and rendering. A matching live trace is still needed; no NPC hotspot
fix or measured saving is claimed by this checkpoint.
