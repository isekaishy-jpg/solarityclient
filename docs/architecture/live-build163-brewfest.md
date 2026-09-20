# Build 163 live Brewfest capture

The user supplied `capture-1789904174970-1` and
`solarity-20260920-073543.log` on 2026-09-20, then clarified that the area outside
Orgrimmar previously ran at about 130–150 FPS, before Brewfest vendors arrived.
The area also contains many creatures and critters. This is workload context,
not a matched before/after benchmark.

The user further clarified that a combined 200–300 players, NPCs, creatures and
critters is normal for a live-server area, and can be low for a busy server.
Treat that range as a representative population requirement, including movement
and camera changes, rather than an exceptional stress case. Added population
explains why a cost becomes visible; it does not make the slowdown acceptable.
The existing 192-NPC synthetic sweep is a useful isolated scaling probe, not a
substitute for mixed population, attachments, animation and live movement.

## Capture identity and limits

- Build 163, source `1b9bf3fb8504ace34a251ac920b9fa5cf76af156`, four CPU workers
  and two network workers. The package's expected dirty marker is documented in
  [the build record](testing-build163-services.md).
- Duration 69.851 seconds; 6,141 ordinary completed frame samples and 49 detail
  frames. Two dropped samples, one dropped trace row, zero dropped event rows
  and zero capacity overflows. Counts crossing capture boundaries differ by one.
- Capture overhead is included. Detail frames average 13.340 ms versus ordinary
  frames at 11.254 ms, so detail timings cannot be substituted for ordinary
  frame costs. The previous capture includes Glue/loading and different world
  views; its whole-run average is not a valid regression baseline.
- Source work continued while the user tested. A CPU/asset check was performed
  during this development interval; its exact overlap with the capture was not
  recorded. The full workspace validation began after capture completion.
  This capture is useful for locating work, not isolating a build-only delta.
- The new typed-product code was uncommitted and was not in the tested binary.
  Do not attribute this capture to that later implementation.

## Recorded costs

Ordinary main-thread totals divided by completed ordinary frames:

| Work | Mean ms per frame |
| --- | ---: |
| Whole frame | 11.254 |
| Session/world service | 1.834 |
| M2 initial admission | 1.009 |
| M2 placement admission, including all resumes | 2.702 |
| M2 publication, including all resumes | 1.721 |
| Vulkan CPU work | 2.102 |

The three M2 intervals total 5.432 ms. They are distinct entry, placement and
publication scopes; their child scopes are not additional costs. The broader
`World scene preparation` scope includes Vulkan presentation and must not be
added to it. Command recording accounts for 1.418 ms within Vulkan CPU work.
Remote movement accounts for 0.529 ms within session/world service.

The 49 GPU timestamp samples average 3.270 ms, including 1.013 ms shadows,
0.802 ms terrain and 0.551 ms M2/transparent effects. GPU activity overlaps CPU
work; these timings are not extra frame costs.

OS thread CPU counters give a separate signal: over the 68.777-second interval
between first and last resource samples, main accumulated 66.297 CPU seconds
(0.964 of one core). The four CPU workers accumulated 5.219, 5.125, 5.344 and
5.797 CPU seconds, respectively (0.075–0.084 cores each). This supports main-thread
work as the principal throughput limit in this capture, rather than four busy
workers holding up a mostly idle coordinator. It does not identify individual
JobContext overhead without a controlled comparison.

Sampled dispatcher lock means are below 0.23 microseconds on every recorded
thread. Worker batch-lock means are 0.30–0.66 microseconds, with isolated tails
up to 1.069 ms. Frame queue residence averages about 20–23 microseconds per
dequeued runner. These samples do not support replacing the dispatcher as the
first response to the multi-millisecond frame cost; queue timestamps end at
dequeue and do not measure the entire dependent critical path.

## Workload and next boundary

Ordinary frames average 433 dynamic placements, 743 visible M2 draw packets,
5,922 bone transforms and 354 light receivers. Detail samples visit about 2,041
placements, early-reject 1,172, and produce 668 environment-shadow owners.
All 40.3 average worker-prepared root palettes were consumed; zero sampled root
palettes were discarded. These are not counts of NPCs: placements include
attachments, scenery and other owners.

Costs move with the view. At 30–40 seconds, ordinary frames average 12.298 ms
and 1,027 M2 draws; at 50–60 seconds they average 10.091 ms and 427 draws.
The final interval rises to 12.638 ms despite fewer dynamic placements. This is
not a stationary leak test or proof that draw count alone explains the cost.

The log ends in `UiQuitRequested`, with no error entries and one earlier warning
about the existing stock failure texture for an absent optional Tauren hair
replacement. Several Vulkan capacity growths occur during movement. The capture
does not show a CPU storage-admission failure.

The remaining cutover target is the ordered main-thread model preparation and
publication boundary, followed by draw submission scaling. It must preserve
stock callback/RNG, attachment and effect ordering while making independent
work worker-owned. Simply finishing service JobContext plumbing or adding more
workers does not remove this measured main-thread work. The earlier
[isolated population sweep](npc-density-cutover-measurements.md) separately
reproduces about 4.5 ms of population-dependent cost; neither study establishes
a fix or a matched live regression delta.
