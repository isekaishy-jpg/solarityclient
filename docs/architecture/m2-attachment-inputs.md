# Retained owner attachment inputs

M2 topology publication already retained requested equipment and visual lists.
Frame consumers nevertheless scanned those scene-wide lists separately for each
body or item: hand-pose capture, CPU bone demand, and attachment publication.
Child admission then scanned the growing rider/item/visual output lists again.
Equipped populations could therefore require quadratic membership work before
the owned geometry jobs reached workers.

## Connected boundary

The folder-backed `m2/attachments` module owns two separate lifetimes:

- Topology-owned request groups provide borrowed slices by body GUID or by
  `(GUID, item attachment point)`. Groups are rebuilt only at existing topology
  publication. Surviving groups reuse storage; departed groups are removed.
  Vehicle ancestry and retirement hand-pose selection use the same interface.
- Frame-owned samples preserve the original ordered records and add parent
  indexes. Publication maintains both through one write API. Clearing the frame
  removes all old samples and hidden flags while retaining capacity. Reservation
  covers the full additional bound after clearing, correcting the former
  `reserve(required - capacity)` use on an empty vector.

Mount membership also has an index, while its count still includes duplicate
records. Request groups preserve duplicate rows and each owner's original order.
Output lookup preserves the first published result, including an explicit hidden
`None`. The separate rider-hidden predicate preserves the existing rule that
**any** hidden rider record suppresses its items. Missing-parent errors remain
distinct from explicitly hidden attachments.

These are changes to access and ownership, not authored animation behavior.
No clocks, RNG calls, visibility decisions, camera ordering, bone sampling,
callbacks, or output order change. Existing stock attachment consumers remain
the behavior authority; focused tests compare the new collections against the
former ordered scans. The serial geometry reference retains linear parent
lookups; it shares the request/publication helpers, and thus is not an independent
oracle for every changed helper.

## Scope and measurement

This removes scene-wide scans from these per-owner inputs. It does not move the
remaining serial callback, attachment sampling or admission work onto workers,
establish a multi-ms gain, or complete working-set byte admission for these
collections. The [complete cutover requirements](cpu-cutover-status.md#still-required-for-the-complete-cutover)
remain active.

The earlier 192-NPC benchmark used zero equipment entries. The new
`solarity-asset` example `inspect_item_attachments` prints exact installed
`Item.dbc`/`ItemDisplayInfo.dbc` definitions for explicitly supplied IDs, so an
equipped workload can be defined from stock data rather than guessed gear.

Installed definitions selected for the comparison are item 50732,
`MainHandWeapon`, display 64530, sheathe 3,
`sword_1h_icecrownraid_d_03.mdx`; and item 18805, `Weapon`, display 33626,
sheathe 3, `Knife_1H_Epic_A_04.mdx`. Both have item visual ID zero. The fixture
supplies these as main/off-hand entries through the existing NPC CLI. These are
facts from the installed archives, not assumed names or pristine-data hashes.

## Validation

Formatting and all-target/all-feature workspace Clippy with warnings denied pass.
The full workspace suite passes 1,638 tests, zero failed, 33 existing ignored,
across 101 suites. Four new cases compare request order, duplicate membership,
first-result versus any-hidden semantics, distinct item/visual keys and frame
replacement with the former scans. Existing equipment, NPC virtual-item,
mount/event ordering, retired hierarchy/GUID reuse, and moving M2 geometry parity
tests also pass. The serial oracle changes only its storage reservation calls;
its parent lookup algorithm remains linear.

Logs are retained in ignored `target/attachment-inputs-{clippy,test}.*.log`.

## Controlled comparison

The controlled equipped fixture uses 300 NPCs with display 6882, main entry 50732,
off-hand entry 18805 and ranged entry zero. Offsets for NPC index `i` are
`(8 + (i % 12) * 1.5, -7 + floor(i / 12) * 1.5, 0)` from Soap at map 1,
`(1515.34, -4417.27, 18.0499)`. Each executable runs 1,024 frames per streaming,
stationary, orbit and pointer phase. Settings are four CPU workers, capacity 256,
two network workers, 2560 x 1440 Ultra, VSync disabled, GTX 1070 and the same
installed data. The separate unarmed control uses 192 NPCs and zero item entries.
These hidden offline runs exclude networking, the live movement solver, sound
and overlays. They do not establish live-server or desktop FPS.

The preserved Build 168 source benchmark has SHA-256
`BAE9093B832D5047E14C6B9A1DC35EFCFA37BE4DBB833CD7A245479218F8D1AE`;
the candidate and its matching PDB are preserved as
`target/attachment-inputs-candidate.{exe,pdb}`, executable SHA-256
`259B3FDFD3C55B8DDE5C6B637F00185C45D4864B2B2E63B5341DD73E92C9FA8A`.

Four alternating equipped runs completed 16,384 frames:

| Median frame time (ms) | Baseline 1 | Indexed 1 | Baseline 2 | Indexed 2 |
| --- | ---: | ---: | ---: | ---: |
| Matched stationary | 14.863 | 14.449 | 14.813 | 14.581 |
| Whole stationary phase | 14.859 | 14.487 | 14.820 | 14.501 |
| Streaming | 14.790 | 14.298 | 14.666 | 14.308 |
| Orbit | 8.930 | 8.515 | 8.937 | 8.552 |
| Pointer | 15.566 | 15.184 | 15.636 | 15.158 |

Stationary matching selects the shared count tuple with the largest minimum
representation across all four runs, independently of measured times: three
resident tiles, zero new admissions, 243 WMO draws, 484 far environment shadows,
3,450 M2 draws and 41,517 bone transforms. Matching selects 401, 500, 583 and 396
frames respectively. Matched p95 values are 16.121, 15.650, 16.136 and 17.176 ms.
Thus the matched median improvements are 0.414 and 0.232 ms, with mixed tails;
the second candidate's matched p95 is higher. One first candidate streaming
frame takes 278.947 ms, versus a 77.449 ms streaming maximum in its repeat.
This is a modest observed improvement, not the requested multi-ms result or
evidence that hitches are solved.

The unarmed control pair completes another 8,192 frames. Matched stationary
medians are 5.678 versus 5.658 ms (606/605 frames); p95 is 6.650 versus 6.839 ms.
The whole stationary medians are 5.661 versus 5.651 ms. This control does not show
a material median improvement, and the candidate stationary maximum is higher
(32.766 versus 9.423 ms). No general hitch or tail-latency improvement is claimed.

A separate equipped profiled pair completes 8,192 frames, with 4,064 ordinary
frames per run. Timings below are totals divided by those ordinary frame counts;
phase scopes can contain nested work and must not be added indiscriminately.

| Boundary (ms per ordinary frame) | Baseline | Indexed |
| --- | ---: | ---: |
| Main M2 admission | 4.520 | 4.099 |
| Main M2 publication | 3.004 | 3.001 |
| Main visible receiver phase | 0.265 | 0.260 |
| Main CPU renderer | 4.682 | 4.655 |
| Renderer slot wait/upload (included above) | 2.214 | 2.201 |
| Renderer command recording (included above) | 1.950 | 1.933 |

The admission reduction is approximately 0.421 ms. Publication remains unchanged;
its candidate geometry and transparent publication subscopes cost 1.368 and
1.183 ms/frame. These are remaining concrete main-thread boundaries for the
cutover, rather than a reason to keep changing small collection operations.
The candidate stationary fixture reaches 110,724 particle vertices, 41,616 bone
transforms, 3,453 M2 draws and 2,760 primary-shadow draws. It exercises much more
than attachment membership. In 32 GPU samples, mean total GPU time is 5.289 versus
5.164 ms; this overlaps CPU execution and is not added to main-thread totals.

No new memory accounting is inferred from the CPU ledger: these request/index
collections remain outside its domain byte coverage, as the original vectors
were. The index adds retained lookup storage in exchange for fewer scans.

Raw artifacts use `target/attachment-{equipped,unarmed}-comparison-*` and
`target/attachment-equipped-profile-*`. The complete comparison and profiler
analysis are retained with the driver scripts in ignored `target/`. All runs
completed successfully without a concurrent compiler or GPU test. This checkpoint
preserves a measured scaling improvement and establishes an equipped qualification
fixture; it does not complete main-thread distribution or the full cutover.
